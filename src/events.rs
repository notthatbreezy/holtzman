//! Bounded resumable event stream. Journal tailing never consumes a partial line.
use crate::journal::{JournalDir, active_flags};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

pub struct Bus {
    inner: Mutex<Inner>,
    pub changed: tokio::sync::Notify,
}
struct Inner {
    seq: u64,
    ring: VecDeque<Value>,
    snapshot: Value,
}
impl Default for Bus {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                seq: 0,
                ring: VecDeque::new(),
                snapshot: json!({}),
            }),
            changed: tokio::sync::Notify::new(),
        }
    }
}
impl Bus {
    pub fn status_event(&self, kind: &str, data: Value, current: &Value) -> Value {
        let mut b = self.inner.lock().unwrap();
        b.snapshot["status"] = current.clone();
        b.seq += 1;
        let event = json!({"seq":b.seq,"t":chrono::Utc::now().to_rfc3339(),"source":"status","type":kind,"data":data});
        b.ring.push_back(event.clone());
        if b.ring.len() > 2000 {
            b.ring.pop_front();
        }
        drop(b);
        self.changed.notify_waiters();
        event
    }
    pub fn sync_status(&self, current: &Value) {
        self.inner.lock().unwrap().snapshot["status"] = current.clone();
    }
    pub fn publish(&self, source: &str, kind: &str, data: Value) -> Value {
        let mut b = self.inner.lock().unwrap();
        if kind == "ptt" {
            if !b.snapshot["ptt"].is_object() {
                b.snapshot["ptt"] = json!({});
            }
            if let Some(fields) = data.as_object() {
                for (key, value) in fields {
                    b.snapshot["ptt"][key] = value.clone();
                }
            }
        } else if kind == "snapshot"
            && let Some(fields) = data.as_object()
        {
            for (key, value) in fields {
                b.snapshot[key] = value.clone();
            }
        }
        b.seq += 1;
        let event = json!({"seq":b.seq,"t":chrono::Utc::now().to_rfc3339(),"source":source,"type":kind,"data":data});
        b.ring.push_back(event.clone());
        if b.ring.len() > 2000 {
            b.ring.pop_front();
        }
        drop(b);
        self.changed.notify_waiters();
        event
    }
    pub fn snapshot(&self, data: Value) {
        self.inner.lock().unwrap().snapshot = data;
    }
    pub fn since(&self, cursor: Option<u64>, types: &[String]) -> Value {
        let b = self.inner.lock().unwrap();
        let reset = cursor.is_some_and(|c| {
            c > b.seq
                || b.ring
                    .front()
                    .is_some_and(|e| c.saturating_add(1) < e["seq"].as_u64().unwrap())
        });
        let fresh = cursor.is_none() || reset;
        let events: Vec<_> = if fresh {
            vec![
                json!({"seq":b.seq,"t":chrono::Utc::now().to_rfc3339(),"source":"server","type":if reset{"reset"}else{"snapshot"},"data":b.snapshot}),
            ]
        } else {
            b.ring
                .iter()
                .filter(|e| {
                    e["seq"].as_u64().unwrap() > cursor.unwrap()
                        && (types.is_empty() || types.iter().any(|t| e["type"] == *t))
                })
                .cloned()
                .collect()
        };
        json!({"cursor":b.seq,"events":events,"reset":reset,"snapshot":b.snapshot})
    }
    pub async fn wait(&self, cursor: Option<u64>, wait_ms: u64, types: &[String]) -> Value {
        let notified = self.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let initial = self.since(cursor, types);
        if initial["events"].as_array().is_some_and(|v| !v.is_empty()) || wait_ms == 0 {
            return initial;
        }
        let _ = tokio::time::timeout(Duration::from_millis(wait_ms.min(25000)), notified).await;
        self.since(cursor, types)
    }
}

#[derive(Default)]
pub struct Tail {
    path: Option<PathBuf>,
    offset: u64,
    partial: Vec<u8>,
    initialized: bool,
}
impl Tail {
    pub fn poll(&mut self, journal: &JournalDir) -> std::io::Result<Vec<Value>> {
        let path = journal.latest_journal().map_err(std::io::Error::other)?;
        let mut file = std::fs::File::open(&path)?;
        let len = file.metadata()?.len();
        if !self.initialized {
            self.initialized = true;
            self.path = Some(path);
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            self.offset = bytes.len() as u64;
            if let Some(end) = bytes.iter().rposition(|c| *c == b'\n') {
                self.partial = bytes[end + 1..].to_vec();
            } else {
                self.partial = bytes;
            }
            return Ok(vec![]);
        }
        let mut output = Vec::new();
        // Drain the old file before switching, including a final Shutdown line.
        if self.path.as_ref() != Some(&path) {
            if let Some(old) = self.path.as_ref().and_then(|p| std::fs::File::open(p).ok()) {
                output.extend(self.read(old)?);
            }
            self.path = Some(path);
            self.offset = 0;
            self.partial.clear();
        } else if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        file.seek(SeekFrom::Start(self.offset))?;
        output.extend(self.read(file)?);
        Ok(output)
    }
    fn read(&mut self, mut file: std::fs::File) -> std::io::Result<Vec<Value>> {
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        self.offset += bytes.len() as u64;
        self.partial.extend(bytes);
        let Some(end) = self.partial.iter().rposition(|c| *c == b'\n') else {
            return Ok(vec![]);
        };
        let complete: Vec<_> = self.partial.drain(..=end).collect();
        Ok(complete
            .split(|c| *c == b'\n')
            .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
            .collect())
    }
}

pub async fn watch(ship: Arc<crate::server::Ship>) {
    let mut tail = Tail::default();
    let mut previous = Value::Null;
    let mut files = std::collections::BTreeMap::<&str, Option<SystemTime>>::new();
    let mut sampled = std::time::Instant::now();
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        interval.tick().await;
        ship.ptt.reload(&ship);
        if let Ok(current) = ship.journal.status() {
            let active = active_flags(&current);
            let live = active.contains(&"InMainShip");
            if current != previous {
                if live && !active_flags(&previous).contains(&"InMainShip") {
                    ship.events
                        .publish("status", "snapshot", json!({"status":current}));
                } else if live {
                    let old = active_flags(&previous);
                    for flag in active.iter().chain(old.iter()) {
                        if active.contains(flag) != old.contains(flag) {
                            ship.events.status_event(
                                "flag",
                                json!({"flag":flag,"on":active.contains(flag)}),
                                &current,
                            );
                        }
                    }
                    let mut changed = serde_json::Map::new();
                    for key in ["GuiFocus", "Destination"] {
                        if current[key] != previous[key] {
                            changed.insert(key.into(), current[key].clone());
                        }
                    }
                    if sampled.elapsed() >= Duration::from_secs(5)
                        || quantized_changed(&current["Fuel"], &previous["Fuel"])
                        || quantized_changed(&current["Cargo"], &previous["Cargo"])
                    {
                        for key in ["Fuel", "Cargo"] {
                            changed.insert(key.into(), current[key].clone());
                        }
                        sampled = std::time::Instant::now();
                    }
                    if !changed.is_empty() {
                        ship.events.status_event("status", json!(changed), &current);
                    }
                } else {
                    ship.events
                        .publish("status", "snapshot", json!({"status":current}));
                }
                ship.events.sync_status(&current);
                previous = current;
            }
        }

        if let Ok(events) = tail.poll(&ship.journal) {
            for event in events {
                let kind = event["event"].as_str().unwrap_or("");
                ship.log.observe(&event);
                if ![
                    "Music",
                    "ReceiveText",
                    "ShipLocker",
                    "FSSSignalDiscovered",
                    "ReservoirReplenished",
                    "Backpack",
                    "NavBeaconScan",
                ]
                .contains(&kind)
                {
                    ship.events.publish("journal", "journal", event);
                }
            }
        }
        for name in ["Cargo.json", "NavRoute.json", "Market.json"] {
            let modified = std::fs::metadata(ship.journal.dir.join(name))
                .and_then(|m| m.modified())
                .ok();
            if let Some(old) = files.insert(name, modified)
                && old != modified
            {
                ship.events
                    .publish("file", "file_changed", json!({"file":name}));
            }
        }
    }
}
fn quantized_changed(a: &Value, b: &Value) -> bool {
    if a == b {
        return false;
    }
    if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
        return (a - b).abs() >= b.abs().max(1.0) * 0.01;
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        return a
            .iter()
            .any(|(k, a)| quantized_changed(a, b.get(k).unwrap_or(&Value::Null)));
    }
    true
}

//! Read the game's journal folder: `Journal.*.log` plus the companion JSON
//! snapshots (`Status.json`, `Market.json`, `Cargo.json`, `NavRoute.json`).
//!
//! Format reference: <https://elite-journal.readthedocs.io/>

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value, json};

/// Default journal location on Windows.
pub fn default_journal_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    Some(
        PathBuf::from(home)
            .join("Saved Games")
            .join("Frontier Developments")
            .join("Elite Dangerous"),
    )
}

/// Status.json `Flags` bits.
const FLAGS: &[(u32, &str)] = &[
    (0, "Docked"),
    (1, "Landed"),
    (2, "LandingGearDown"),
    (3, "ShieldsUp"),
    (4, "Supercruise"),
    (5, "FlightAssistOff"),
    (6, "HardpointsDeployed"),
    (7, "InWing"),
    (8, "LightsOn"),
    (9, "CargoScoopDeployed"),
    (10, "SilentRunning"),
    (11, "ScoopingFuel"),
    (16, "FsdMassLocked"),
    (17, "FsdCharging"),
    (18, "FsdCooldown"),
    (19, "LowFuel"),
    (20, "OverHeating"),
    (22, "IsInDanger"),
    (23, "BeingInterdicted"),
    (24, "InMainShip"),
    (25, "InFighter"),
    (26, "InSRV"),
    (30, "FsdJump"),
];

/// Status.json `Flags2` bits (Odyssey on-foot state).
const FLAGS2: &[(u32, &str)] = &[
    (0, "OnFoot"),
    (1, "InTaxi"),
    (2, "InMulticrew"),
    (3, "OnFootInStation"),
    (4, "OnFootOnPlanet"),
    (19, "FsdHyperdriveCharging"),
    (21, "SupercruiseOverdrive"),
];

/// Status.json `GuiFocus` values.
pub mod gui {
    pub const NONE: i64 = 0;
    pub const GALAXY_MAP: i64 = 6;
    pub const SYSTEM_MAP: i64 = 7;

    pub fn name(v: i64) -> &'static str {
        match v {
            0 => "None",
            1 => "InternalPanel",
            2 => "ExternalPanel",
            3 => "CommsPanel",
            4 => "RolePanel",
            5 => "StationServices",
            6 => "GalaxyMap",
            7 => "SystemMap",
            8 => "Orrery",
            9 => "FSS",
            10 => "SAA",
            11 => "Codex",
            _ => "Unknown",
        }
    }
}

pub fn decode_flags(flags: u64, table: &'static [(u32, &'static str)]) -> Vec<&'static str> {
    table
        .iter()
        .filter(|(bit, _)| flags & (1u64 << bit) != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// Names of the set `Flags` and `Flags2` bits in a Status.json value.
fn active_flags(status: &Value) -> Vec<&'static str> {
    let flags = status.get("Flags").and_then(Value::as_u64).unwrap_or(0);
    let flags2 = status.get("Flags2").and_then(Value::as_u64).unwrap_or(0);
    let mut active = decode_flags(flags, FLAGS);
    active.extend(decode_flags(flags2, FLAGS2));
    active
}

#[derive(Debug, Clone)]
pub struct JournalDir {
    pub dir: PathBuf,
}

impl JournalDir {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn read_json(&self, name: &str) -> Result<Value> {
        let path = self.dir.join(name);
        // The game rewrites these files in place; retry once on a torn read.
        for attempt in 0..3 {
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            match serde_json::from_str(&text) {
                Ok(v) => return Ok(v),
                Err(_) if attempt < 2 => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(e) => return Err(e).with_context(|| format!("parsing {}", path.display())),
            }
        }
        unreachable!()
    }

    pub fn status(&self) -> Result<Value> {
        self.read_json("Status.json")
    }

    pub fn gui_focus(&self) -> Option<i64> {
        self.status().ok()?.get("GuiFocus")?.as_i64()
    }

    /// What Status.json says about the screen: open panel and active flags.
    pub fn ui_state(&self) -> Result<Value> {
        let status = self.status()?;
        let focus = status.get("GuiFocus").and_then(Value::as_i64).unwrap_or(0);
        Ok(json!({
            "status_timestamp": status.get("timestamp"),
            "gui_focus": gui::name(focus),
            "flags": active_flags(&status),
        }))
    }

    pub fn market(&self) -> Result<Value> {
        self.read_json("Market.json")
    }

    pub fn cargo(&self) -> Result<Value> {
        self.read_json("Cargo.json")
    }

    pub fn nav_route(&self) -> Result<Value> {
        self.read_json("NavRoute.json")
    }

    /// Final system of the currently plotted route, if any.
    pub fn route_destination(&self) -> Option<String> {
        let route = self.nav_route().ok()?;
        let last = route.get("Route")?.as_array()?.last()?;
        Some(last.get("StarSystem")?.as_str()?.to_string())
    }

    pub fn latest_journal(&self) -> Result<PathBuf> {
        let mut logs: Vec<PathBuf> = fs::read_dir(&self.dir)
            .with_context(|| format!("listing {}", self.dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap_or_default().to_string_lossy();
                n.starts_with("Journal.") && n.ends_with(".log")
            })
            .collect();
        // Names embed an ISO-ish timestamp, so lexical order is chronological.
        logs.sort();
        logs.pop()
            .ok_or_else(|| anyhow!("no Journal.*.log files in {}", self.dir.display()))
    }

    /// All events from the newest journal file, oldest first.
    pub fn events(&self) -> Result<Vec<Value>> {
        let path = self.latest_journal()?;
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        Ok(text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .collect())
    }

    /// The last `limit` events, skipping chatty ones unless asked for by name.
    pub fn recent_events(&self, limit: usize, only: &[String]) -> Result<Vec<Value>> {
        const NOISE: &[&str] = &[
            "Music",
            "ReceiveText",
            "ShipLocker",
            "FSSSignalDiscovered",
            "ReservoirReplenished",
            "Backpack",
            "NavBeaconScan",
        ];
        let events = self.events()?;
        let filtered: Vec<Value> = events
            .into_iter()
            .filter(|e| {
                let ev = e.get("event").and_then(Value::as_str).unwrap_or_default();
                if only.is_empty() {
                    !NOISE.contains(&ev)
                } else {
                    only.iter().any(|o| o.eq_ignore_ascii_case(ev))
                }
            })
            .collect();
        let start = filtered.len().saturating_sub(limit);
        Ok(filtered[start..].to_vec())
    }

    /// A compact "where am I, what am I flying" summary built from the latest
    /// journal plus Status.json.
    pub fn summary(&self) -> Result<Value> {
        let events = self.events().unwrap_or_default();
        let mut out = Map::new();
        let mut location = Map::new();

        for e in &events {
            let ev = e.get("event").and_then(Value::as_str).unwrap_or_default();
            match ev {
                "Commander" | "LoadGame" => {
                    copy(
                        e,
                        &mut out,
                        &[("Commander", "commander"), ("Name", "commander")],
                    );
                    copy(e, &mut out, &[("Credits", "credits")]);
                }
                "Location" | "FSDJump" | "CarrierJump" => {
                    location.clear();
                    copy(
                        e,
                        &mut location,
                        &[("StarSystem", "system"), ("StarPos", "star_pos")],
                    );
                    if e.get("Docked").and_then(Value::as_bool) == Some(true) {
                        copy(e, &mut location, &[("StationName", "station")]);
                    }
                }
                "SupercruiseExit" => {
                    copy(e, &mut location, &[("Body", "near")]);
                }
                "SupercruiseEntry" => {
                    location.remove("near");
                    location.remove("station");
                }
                "Docked" => {
                    copy(
                        e,
                        &mut location,
                        &[("StationName", "station"), ("StarSystem", "system")],
                    );
                }
                "Undocked" => {
                    location.remove("station");
                }
                "Loadout" => {
                    let mut ship = Map::new();
                    copy(
                        e,
                        &mut ship,
                        &[
                            ("Ship", "type"),
                            ("ShipName", "name"),
                            ("ShipIdent", "ident"),
                            ("CargoCapacity", "cargo_capacity"),
                            ("MaxJumpRange", "max_jump_range_ly"),
                            ("Rebuy", "rebuy"),
                            ("HullHealth", "hull_health"),
                        ],
                    );
                    out.insert("ship".into(), Value::Object(ship));
                }
                "Rank" | "Progress" => {
                    let key = ev.to_ascii_lowercase();
                    let mut m = e.as_object().cloned().unwrap_or_default();
                    m.remove("timestamp");
                    m.remove("event");
                    out.insert(key, Value::Object(m));
                }
                _ => {}
            }
        }
        out.insert("location".into(), Value::Object(location));

        if let Ok(status) = self.status() {
            let active = active_flags(&status);
            let focus = status.get("GuiFocus").and_then(Value::as_i64).unwrap_or(0);
            out.insert(
                "status".into(),
                json!({
                    "timestamp": status.get("timestamp"),
                    "flags": active,
                    "gui_focus": gui::name(focus),
                    "fuel": status.get("Fuel"),
                    "cargo_tons": status.get("Cargo"),
                    "legal_state": status.get("LegalState"),
                    "destination": status.get("Destination"),
                }),
            );
            if let Some(balance) = status.get("Balance") {
                out.insert("credits".into(), balance.clone());
            }
        }
        if let Some(dest) = self.route_destination() {
            out.insert("plotted_route_to".into(), Value::String(dest));
        }
        Ok(Value::Object(out))
    }

    /// Market.json trimmed to what matters for trading, optionally filtered by
    /// commodity name substring.
    pub fn market_summary(&self, filter: Option<&str>) -> Result<Value> {
        let m = self.market()?;
        let filter = filter.map(str::to_ascii_lowercase);
        let items: Vec<Value> = m
            .get("Items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| {
                        let name = i
                            .get("Name_Localised")
                            .or_else(|| i.get("Name"))
                            .and_then(Value::as_str)?
                            .to_string();
                        if let Some(f) = &filter
                            && !name.to_ascii_lowercase().contains(f)
                        {
                            return None;
                        }
                        let stock = i.get("Stock").and_then(Value::as_i64).unwrap_or(0);
                        let demand = i.get("Demand").and_then(Value::as_i64).unwrap_or(0);
                        // Without a filter, skip rows the station neither sells nor wants.
                        if filter.is_none() && stock == 0 && demand <= 1 {
                            return None;
                        }
                        Some(json!({
                            "name": name,
                            "category": i.get("Category_Localised"),
                            "buy": i.get("BuyPrice"),
                            "sell": i.get("SellPrice"),
                            "mean": i.get("MeanPrice"),
                            "stock": stock,
                            "demand": demand,
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(json!({
            "station": m.get("StationName"),
            "system": m.get("StarSystem"),
            "timestamp": m.get("timestamp"),
            "items": items,
        }))
    }
}

fn copy(src: &Value, dst: &mut Map<String, Value>, fields: &[(&str, &str)]) {
    for (from, to) in fields {
        if let Some(v) = src.get(*from)
            && !v.is_null()
            && v.as_str() != Some("")
        {
            dst.insert((*to).to_string(), v.clone());
        }
    }
}

/// Is `path` inside the journal folder? Used to sanity-check config.
pub fn looks_like_journal_dir(path: &Path) -> bool {
    path.join("Status.json").exists()
        || fs::read_dir(path)
            .map(|mut d| {
                d.any(|e| {
                    e.map(|e| e.file_name().to_string_lossy().starts_with("Journal."))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> JournalDir {
        JournalDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
    }

    #[test]
    fn decodes_flags() {
        // Docked + LandingGearDown + ShieldsUp + InMainShip
        let f = (1 << 0) | (1 << 2) | (1 << 3) | (1 << 24);
        assert_eq!(
            decode_flags(f, FLAGS),
            vec!["Docked", "LandingGearDown", "ShieldsUp", "InMainShip"]
        );
    }

    #[test]
    fn summarizes_latest_journal() {
        let s = fixture().summary().unwrap();
        assert_eq!(s["commander"], "TESTCMDR");
        assert_eq!(s["location"]["system"], "Veli");
        assert_eq!(s["location"]["station"], "Pratchett Port");
        assert_eq!(s["ship"]["type"], "type9");
        assert_eq!(s["ship"]["cargo_capacity"], 776);
        assert_eq!(s["status"]["gui_focus"], "StationServices");
        assert_eq!(s["credits"], 154870296);
        assert_eq!(s["plotted_route_to"], "Ross 248");
    }

    #[test]
    fn filters_market() {
        let m = fixture().market_summary(Some("cobalt")).unwrap();
        let items = m["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["sell"], 19770);
        let all = fixture().market_summary(None).unwrap();
        // Landmines has no stock and no demand, so it's dropped.
        assert!(
            all["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["name"] != "Landmines")
        );
    }

    #[test]
    fn recent_events_skips_noise() {
        let ev = fixture().recent_events(3, &[]).unwrap();
        assert_eq!(ev.len(), 3);
        assert!(ev.iter().all(|e| e["event"] != "Music"));
        let docked = fixture().recent_events(10, &["docked".into()]).unwrap();
        assert_eq!(docked.len(), 1);
    }
}

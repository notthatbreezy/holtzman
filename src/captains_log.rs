//! Episodic facts are computed from journals; quoted entries and narratives stay separate.
use crate::journal::JournalDir;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub range: Option<String>,
    pub include: Option<Vec<String>>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntryArgs {
    pub text: Option<String>,
    pub author: Option<String>,
    pub scratch: Option<bool>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveArgs {
    pub op: String,
    pub text: Option<String>,
    pub measure: Option<Value>,
    pub id: Option<String>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanArgs {
    pub op: String,
    pub reference: Option<String>,
}

struct CachedSessions {
    signature: Vec<(PathBuf, u64, Option<std::time::SystemTime>)>,
    rows: Vec<Value>,
}
pub struct Store {
    root: Option<PathBuf>,
    lock: Mutex<()>,
    routes: Mutex<BTreeMap<String, Value>>,
    cache: Mutex<Option<CachedSessions>>,
}
impl Store {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            lock: Mutex::new(()),
            routes: Mutex::new(BTreeMap::new()),
            cache: Mutex::new(None),
        }
    }
    fn session_facts(&self, journal: &JournalDir) -> Result<Vec<Value>> {
        let mut signature = Vec::new();
        for file in fs::read_dir(&journal.dir)? {
            let file = file?;
            let path = file.path();
            if path.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("Journal.") && n.to_string_lossy().ends_with(".log")
            }) {
                let meta = file.metadata()?;
                signature.push((path, meta.len(), meta.modified().ok()));
            }
        }
        signature.sort_by(|a, b| a.0.cmp(&b.0));
        let mut cache = self.cache.lock().unwrap();
        if let Some(prior) = &*cache
            && prior.signature == signature
        {
            return Ok(prior.rows.clone());
        }
        let rows = sessions(journal)?;
        *cache = Some(CachedSessions {
            signature,
            rows: rows.clone(),
        });
        Ok(rows)
    }
    fn root(&self) -> Result<&Path> {
        self.root
            .as_deref()
            .context("captain's log storage is not configured")
    }
    fn rows(&self, name: &str) -> Result<Vec<Value>> {
        let path = self.root()?.join(name);
        if !path.exists() {
            return Ok(vec![]);
        }
        BufReader::new(fs::File::open(path)?)
            .lines()
            .filter_map(|l| match l {
                Ok(l) if l.trim().is_empty() => None,
                other => Some(other),
            })
            .map(|l| Ok(serde_json::from_str(&l?)?))
            .collect()
    }
    fn append(&self, name: &str, value: &Value) -> Result<()> {
        let root = self.root()?;
        fs::create_dir_all(root)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join(name))?;
        writeln!(file, "{}", serde_json::to_string(value)?)?;
        file.sync_all()?;
        Ok(())
    }
    fn read(&self, name: &str) -> Result<Value> {
        let path = self.root()?.join(name);
        if !path.exists() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
    fn write(&self, name: &str, value: &Value) -> Result<()> {
        let root = self.root()?;
        fs::create_dir_all(root)?;
        let path = root.join(name);
        let tmp = root.join(format!("{name}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }
    pub fn entry(&self, args: EntryArgs, journal: &JournalDir) -> Result<Value> {
        let _guard = self.lock.lock().unwrap();
        let author = args.author.as_deref().unwrap_or("captain");
        if !["captain", "holt"].contains(&author) {
            bail!("author must be captain or holt");
        }
        if args.scratch.unwrap_or(false) {
            let rows = self.rows("entries.jsonl")?;
            let deleted: BTreeSet<_> = rows.iter().filter_map(|r| r["deletes"].as_str()).collect();
            let last = rows.iter().rev().find(|r| {
                r["author"] == "captain" && r["id"].as_str().is_some_and(|id| !deleted.contains(id))
            });
            let Some(last) = last else {
                bail!("no captain's entry to scratch");
            };
            let t = timestamp(&last["timestamp"]).context("entry timestamp invalid")?;
            if (Utc::now() - t).num_seconds() > 60 {
                bail!("last entry is older than one minute");
            }
            let receipt = json!({"deletes":last["id"],"timestamp":Utc::now().to_rfc3339()});
            self.append("entries.jsonl", &receipt)?;
            return Ok(receipt);
        }
        let text = args.text.context("text is required")?;
        if text.trim().is_empty() || text.len() > 32000 {
            bail!("entry must contain 1..32000 bytes");
        }
        let summary = journal.summary()?;
        let session_id = self.session_facts(journal)?.last().map(|s| s["id"].clone());
        let receipt = json!({"id":rand::random::<u64>().to_string(),"timestamp":Utc::now().to_rfc3339(),"session":session_id,"author":author,"text":text,
            "location":summary["location"],"ship":summary["ship"],"label":if author=="holt"{"HOLT's account"}else{"Captain's verbatim entry"}});
        self.append(
            if author == "holt" {
                "narratives.jsonl"
            } else {
                "entries.jsonl"
            },
            &receipt,
        )?;
        Ok(receipt)
    }
    pub fn query(&self, args: Query, journal: &JournalDir) -> Result<Value> {
        let sessions = self.session_facts(journal)?;
        let range = args.range.as_deref().unwrap_or("last");
        let chosen = select(&sessions, range)?;
        let include = args
            .include
            .unwrap_or_else(|| vec!["facts".into(), "entries".into(), "narrative".into()]);
        if include
            .iter()
            .any(|i| !["facts", "entries", "narrative"].contains(&i.as_str()))
        {
            bail!("include: facts, entries, narrative");
        }
        let _guard = self.lock.lock().unwrap();
        let mut result = json!({"range":range,"sessions":chosen});
        if !include.iter().any(|s| s == "facts") {
            result["sessions"] = json!(
                chosen
                    .iter()
                    .map(|s| json!({"id":s["id"],"start":s["start"],"end":s["end"]}))
                    .collect::<Vec<_>>()
            );
        }
        for (key, file) in [
            ("entries", "entries.jsonl"),
            ("narrative", "narratives.jsonl"),
        ] {
            if !include.iter().any(|s| s == key) {
                continue;
            }
            let rows = if self.root.is_some() {
                self.rows(file)?
            } else {
                vec![]
            };
            let deleted: BTreeSet<_> = rows.iter().filter_map(|r| r["deletes"].as_str()).collect();
            let entries: Vec<_> = rows
                .iter()
                .filter(|r| {
                    r["id"].as_str().is_some_and(|id| !deleted.contains(id))
                        && chosen.iter().any(|s| {
                            r["session"] == s["id"]
                                || within(r, s)
                                || (r["label"] == "Completed plan receipt"
                                    && timestamp(&r["timestamp"]).is_some_and(|t| {
                                        sessions
                                            .iter()
                                            .rev()
                                            .find(|candidate| {
                                                timestamp(&candidate["start"])
                                                    .is_some_and(|start| start <= t)
                                            })
                                            .is_some_and(|candidate| candidate["id"] == s["id"])
                                    }))
                        })
                })
                .cloned()
                .collect();
            result[key] = json!(entries);
        }
        Ok(result)
    }
    pub fn report(&self, session: Option<String>, journal: &JournalDir) -> Result<Value> {
        let all = self.session_facts(journal)?;
        let chosen = select(&all, session.as_deref().unwrap_or("last"))?;
        Ok(
            json!({"sessions":chosen,"instruction":"Journal facts take precedence over narratives. End the spoken report with the avoidable-incident count."}),
        )
    }
    pub fn objectives(&self, args: ObjectiveArgs, journal: &JournalDir) -> Result<Value> {
        let _guard = self.lock.lock().unwrap();
        let mut goals = self
            .read("objectives.json")?
            .as_array()
            .cloned()
            .unwrap_or_default();
        match args.op.as_str() {
            "list" => {}
            "set" => {
                let text = args.text.context("text required")?;
                if text.trim().is_empty() || text.len() > 2000 {
                    bail!("objective must contain 1..2000 bytes");
                }
                if args.measure.as_ref().is_some_and(|m| {
                    !matches!(
                        m["kind"].as_str(),
                        Some("credits" | "rank" | "engineer" | "materials")
                    )
                }) {
                    bail!("measure kind: credits, rank, engineer, materials");
                }
                goals.push(json!({"id":rand::random::<u64>().to_string(),"text":text,"measure":args.measure,"created":Utc::now().to_rfc3339(),"baseline":journal.summary()?,"baseline_measure":args.measure.as_ref().map(|m|crate::planning::measure(m,journal)),"state":"active"}));
                self.write("objectives.json", &json!(goals))?;
            }
            "close" => {
                let id = args.id.context("id required")?;
                let goal = goals
                    .iter_mut()
                    .find(|g| g["id"] == id)
                    .context("unknown objective")?;
                goal["state"] = json!("closed");
                self.write("objectives.json", &json!(goals))?;
            }
            _ => bail!("op: list, set, close"),
        }
        for goal in &mut goals {
            if !goal["measure"].is_null() {
                goal["progress"] = crate::planning::measure(&goal["measure"], journal);
            }
        }
        Ok(json!({"objectives":goals,"current":journal.summary()?}))
    }
    pub fn cache_route(&self, result: Value) -> String {
        let id = rand::random::<u64>().to_string();
        let mut routes = self.routes.lock().unwrap();
        routes.retain(|_, r| {
            timestamp(&r["created"]).is_some_and(|t| (Utc::now() - t).num_minutes() < 30)
        });
        if routes.len() >= 100
            && let Some(key) = routes.keys().next().cloned()
        {
            routes.remove(&key);
        }
        routes.insert(
            id.clone(),
            json!({"created":Utc::now().to_rfc3339(),"result":result}),
        );
        id
    }
    pub fn plan(&self, args: PlanArgs, journal: &JournalDir) -> Result<Value> {
        let _guard = self.lock.lock().unwrap();
        let mut plan = self.read("plan.json")?;
        if timestamp(&plan["used"]).is_some_and(|t| (Utc::now() - t).num_days() >= 7) {
            plan = Value::Null;
            self.write("plan.json", &plan)?;
        }
        match args.op.as_str() {
            "get" => {}
            "clear" => {
                plan = Value::Null;
                self.write("plan.json", &plan)?;
            }
            "set" => {
                let reference = args
                    .reference
                    .context("reference required; arbitrary legs are not accepted")?;
                let legs = if let Some(id) = reference
                    .strip_prefix("mission:")
                    .and_then(|id| id.parse::<u64>().ok())
                {
                    let facts = crate::planning::facts(journal);
                    let mission = facts["missions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|m| m["MissionID"] == id)
                        .context("mission is not active")?;
                    let system = mission["DestinationSystem"]
                        .as_str()
                        .context("mission destination not recorded; cannot derive plan")?;
                    let mut legs = Vec::new();
                    if let Some(station) = mission["DestinationStation"].as_str() {
                        legs.push(json!({"kind":"travel","system":system,"station":station}));
                    }
                    legs.push(json!({"kind":"deliver","system":system,"station":mission["DestinationStation"],"mission_id":id}));
                    legs
                } else {
                    let routes = self.routes.lock().unwrap();
                    let source = routes
                        .get(&reference)
                        .filter(|r| {
                            timestamp(&r["created"])
                                .is_some_and(|t| (Utc::now() - t).num_minutes() < 30)
                        })
                        .context("unknown or expired verified route reference")?;
                    route_legs(&source["result"])?
                };
                let summary = journal.summary()?;
                let mut index = 0;
                if summary["status"]["flags"]
                    .as_array()
                    .is_some_and(|flags| flags.iter().any(|f| f == "Docked"))
                    && legs.first().is_some_and(|leg| {
                        leg["kind"] == "travel"
                            && eq(&leg["system"], &summary["location"]["system"])
                            && eq(&leg["station"], &summary["location"]["station"])
                    })
                {
                    index = 1;
                }
                plan = json!({"id":rand::random::<u64>().to_string(),"reference":reference,"legs":legs,"index":index,"used":Utc::now().to_rfc3339(),"state":"active","at":summary["location"]});
                self.write("plan.json", &plan)?;
            }
            "skip_leg" => {
                if plan.is_null() {
                    bail!("no active plan");
                }
                plan["index"] = json!(plan["index"].as_u64().unwrap_or(0) + 1);
                plan["used"] = json!(Utc::now().to_rfc3339());
                self.finish_plan(&mut plan)?;
                self.write("plan.json", &plan)?;
            }
            _ => bail!("op: set, get, skip_leg, clear"),
        }
        let _ = journal;
        Ok(if plan.is_null() {
            json!({"state":"none","legs":[],"index":0})
        } else {
            plan
        })
    }
    fn finish_plan(&self, plan: &mut Value) -> Result<()> {
        if plan["index"].as_u64().unwrap_or(0) as usize
            >= plan["legs"].as_array().map_or(0, Vec::len)
            && plan["state"] != "finished"
        {
            plan["state"] = json!("finished");
            self.append("narratives.jsonl",&json!({"id":rand::random::<u64>().to_string(),"timestamp":Utc::now().to_rfc3339(),"author":"holt","label":"Completed plan receipt","plan":plan}))?;
        }
        Ok(())
    }
    pub fn observe(&self, event: &Value) {
        let _guard = self.lock.lock().unwrap();
        let Ok(mut plan) = self.read("plan.json") else {
            return;
        };
        if plan["state"] != "active" {
            return;
        }
        if timestamp(&plan["used"]).is_some_and(|t| (Utc::now() - t).num_days() >= 7) {
            let _ = self.write("plan.json", &Value::Null);
            return;
        }
        let kind = event["event"].as_str().unwrap_or("");
        if matches!(kind, "Docked" | "Location" | "FSDJump" | "CarrierJump") {
            if !plan["at"].is_object() {
                plan["at"] = json!({});
            }
            plan["at"]["system"] = event["StarSystem"].clone();
            plan["at"]["station"] = if kind == "Docked" || event["Docked"] == true {
                event["StationName"].clone()
            } else {
                Value::Null
            };
        } else if kind == "Undocked" {
            plan["at"]["station"] = Value::Null;
        }
        let index = plan["index"].as_u64().unwrap_or(0) as usize;
        let Some(leg) = plan["legs"].get(index).cloned() else {
            return;
        };
        let at = eq(&plan["at"]["system"], &leg["system"])
            && eq(&plan["at"]["station"], &leg["station"]);
        let mut complete = false;
        match leg["kind"].as_str() {
            Some("travel") => {
                complete = kind == "Docked" && at;
            }
            Some("buy" | "sell") => {
                let expected = if leg["kind"] == "buy" {
                    "MarketBuy"
                } else {
                    "MarketSell"
                };
                let normalize = |v: &Value| {
                    v.as_str()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .collect::<String>()
                };
                if kind == expected
                    && at
                    && normalize(&event["Type"]) == normalize(&leg["commodity"])
                {
                    let completed = leg["completed_tonnes"]
                        .as_u64()
                        .unwrap_or(0)
                        .saturating_add(event["Count"].as_u64().unwrap_or(0));
                    plan["legs"][index]["completed_tonnes"] = json!(completed);
                    complete = completed >= leg["tonnes"].as_u64().unwrap_or(u64::MAX);
                }
            }
            Some("deliver") => {
                complete = kind == "MissionCompleted" && event["MissionID"] == leg["mission_id"];
            }
            _ => {}
        }
        if complete {
            plan["index"] = json!(index + 1);
            while let Some(next) = plan["legs"].get(plan["index"].as_u64().unwrap() as usize) {
                if next["kind"] != "travel"
                    || !eq(&plan["at"]["system"], &next["system"])
                    || !eq(&plan["at"]["station"], &next["station"])
                {
                    break;
                }
                plan["index"] = json!(plan["index"].as_u64().unwrap() + 1);
            }
            plan["used"] = json!(Utc::now().to_rfc3339());
            if self.finish_plan(&mut plan).is_err() {
                return;
            }
        }
        let _ = self.write("plan.json", &plan);
    }
}
fn eq(a: &Value, b: &Value) -> bool {
    a.as_str()
        .zip(b.as_str())
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
}
fn route_legs(value: &Value) -> Result<Vec<Value>> {
    let trades = value["legs"]
        .as_array()
        .context("provider result has no verified legs")?;
    let mut legs = Vec::new();
    for trade in trades {
        for (port, kind) in [("source", "buy"), ("destination", "sell")] {
            let location = &trade[port];
            let system = location["system"]
                .as_str()
                .context("route system missing")?;
            let station = location["station"]
                .as_str()
                .context("route station missing")?;
            legs.push(json!({"kind":"travel","system":system,"station":station}));
            for item in trade["commodities"]
                .as_array()
                .context("route commodities missing")?
            {
                let commodity = item["name"].as_str().context("commodity missing")?;
                let tonnes = item["amount"].as_u64().context("tonnes missing")?;
                legs.push(json!({"kind":kind,"system":system,"station":station,"commodity":commodity,"tonnes":tonnes,"completed_tonnes":0}));
            }
        }
    }
    if legs.is_empty() {
        bail!("empty itinerary");
    }
    Ok(legs)
}
fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}
fn within(row: &Value, session: &Value) -> bool {
    let Some(t) = timestamp(&row["timestamp"]) else {
        return false;
    };
    timestamp(&session["start"]).is_some_and(|start| t >= start)
        && timestamp(&session["end"]).is_some_and(|end| t <= end)
}
fn select<'a>(sessions: &'a [Value], range: &str) -> Result<Vec<&'a Value>> {
    if range == "last" {
        return Ok(sessions.last().into_iter().collect());
    }
    if let Some(s) = sessions.iter().find(|s| s["id"] == range) {
        return Ok(vec![s]);
    }
    let today = chrono::Local::now().date_naive().to_string();
    let r = if range == "today" {
        today.as_str()
    } else {
        range
    };
    let (first, last) = r.split_once("..").unwrap_or((r, r));
    let first = chrono::NaiveDate::parse_from_str(first, "%Y-%m-%d")
        .context("range: last, today, YYYY-MM-DD, YYYY-MM-DD..YYYY-MM-DD or session id")?;
    let last = chrono::NaiveDate::parse_from_str(last, "%Y-%m-%d")?;
    if first > last {
        bail!("date range is reversed");
    }
    Ok(sessions
        .iter()
        .filter(|s| {
            timestamp(&s["start"]).is_some_and(|t| {
                let d = t.with_timezone(&chrono::Local).date_naive();
                d >= first && d <= last
            })
        })
        .collect())
}

#[derive(Default, Serialize)]
struct Session {
    id: String,
    start: Value,
    end: Value,
    ships: BTreeSet<String>,
    systems: BTreeSet<String>,
    stations: BTreeSet<String>,
    jumps: u64,
    light_years: f64,
    start_credits: Option<i64>,
    end_credits: Option<i64>,
    earnings: i64,
    spending: i64,
    discoveries: u64,
    mappings: u64,
    avoidable_incidents: Vec<Value>,
    saves: u64,
}
pub fn sessions(journal: &JournalDir) -> Result<Vec<Value>> {
    let mut paths: Vec<_> = fs::read_dir(&journal.dir)?
        .filter_map(|p| p.ok().map(|p| p.path()))
        .filter(|p| {
            p.file_name().is_some_and(|s| {
                s.to_string_lossy().starts_with("Journal.") && s.to_string_lossy().ends_with(".log")
            })
        })
        .collect();
    paths.sort();
    let mut output = Vec::<Session>::new();
    let mut current: Option<Session> = None;
    let mut system = String::new();
    let mut scanned = BTreeSet::new();
    for path in paths {
        for line in BufReader::new(fs::File::open(path)?).lines() {
            let line = line?;
            let Ok(e) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(time) = timestamp(&e["timestamp"]) else {
                continue;
            };
            let kind = e["event"].as_str().unwrap_or("");
            let gap = current
                .as_ref()
                .and_then(|s| timestamp(&s.end))
                .is_some_and(|last| (time - last).num_minutes() >= 30);
            if kind == "LoadGame" || gap {
                if let Some(mut session) = current.take() {
                    if kind == "LoadGame" {
                        session.end_credits = e["Credits"].as_i64();
                    }
                    output.push(session);
                }
                if kind == "LoadGame"
                    && let Some(last) = output.last_mut()
                    && last.end_credits.is_none()
                {
                    last.end_credits = e["Credits"].as_i64();
                }
            }
            if kind == "LoadGame" || gap {
                current = Some(Session {
                    id: format!("session-{}", time.timestamp_millis()),
                    start: e["timestamp"].clone(),
                    start_credits: if kind == "LoadGame" {
                        e["Credits"].as_i64()
                    } else {
                        None
                    },
                    ..Default::default()
                });
                scanned.clear();
            }
            let Some(s) = current.as_mut() else {
                continue;
            };
            s.end = e["timestamp"].clone();
            if let Some(ship) = e["Ship"].as_str() {
                s.ships.insert(ship.into());
            }
            if let Some(name) = e["StarSystem"].as_str() {
                system = name.into();
                s.systems.insert(system.clone());
            }
            if kind == "Docked"
                && let Some(station) = e["StationName"].as_str()
            {
                s.stations.insert(format!("{station} ({system})"));
            }
            if kind == "FSDJump" {
                s.jumps += 1;
                s.light_years += e["JumpDist"].as_f64().unwrap_or(0.0);
            }
            let amount = |fields: &[&str]| fields.iter().find_map(|f| e[*f].as_i64()).unwrap_or(0);
            s.earnings += match kind {
                "MarketSell" => amount(&["TotalSale"]),
                "MissionCompleted" | "RedeemVoucher" => amount(&["Reward", "Amount"]),
                "SellExplorationData" | "MultiSellExplorationData" => e["TotalEarnings"]
                    .as_i64()
                    .unwrap_or_else(|| amount(&["BaseValue"]) + amount(&["Bonus"])),
                _ => 0,
            };
            s.spending += match kind {
                "MarketBuy" => amount(&["TotalCost"]),
                "ModuleBuy" => amount(&["BuyPrice"]) - amount(&["SellPrice"]),
                "ShipyardBuy" => amount(&["ShipPrice"]) - amount(&["SellPrice"]),
                "Repair" | "RepairAll" | "RefuelAll" | "RefuelPartial" | "BuyAmmo"
                | "BuyDrones" | "PayFines" | "PayBounties" => amount(&["Cost", "Amount"]),
                _ => 0,
            };
            if kind == "Scan" {
                if e["WasDiscovered"] == false {
                    s.discoveries += 1;
                }
                if e["WasMapped"] == false {
                    scanned.insert((system.clone(), e["BodyID"].to_string()));
                }
            }
            if kind == "SAAScanComplete"
                && scanned.remove(&(system.clone(), e["BodyID"].to_string()))
            {
                s.mappings += 1;
            }
            if matches!(
                kind,
                "Died" | "HeatDamage" | "DockingCancelled" | "DockingTimeout"
            ) || kind == "Interdicted" && e["Submitted"] == true
                || kind == "HullDamage"
                    && e["Health"].as_f64().is_some_and(|h| h <= 0.2)
                    && e["PlayerPilot"] != false
                    && e["Fighter"] != true
            {
                s.avoidable_incidents
                    .push(json!({"event":kind,"timestamp":e["timestamp"],"system":system}));
            }
            if kind == "EscapeInterdiction" {
                s.saves += 1;
            }
            if kind == "Shutdown" {
                output.push(current.take().unwrap());
            }
        }
    }
    if let Some(s) = current {
        output.push(s);
    }
    Ok(output
        .into_iter()
        .map(|mut s| {
            if s.end_credits.is_none() {
                s.end_credits = s.start_credits.map(|start| start + s.earnings - s.spending);
            }
            json!(s)
        })
        .collect())
}

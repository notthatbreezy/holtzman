//! Reconcile recorded fleet ownership, rather than treating old Loadout events as owned ships.
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::journal::JournalDir;

#[derive(Default)]
struct ShipRecord {
    data: serde_json::Map<String, Value>,
    loadout: Option<Value>,
    changed: Option<Value>,
}

#[derive(Default)]
struct Fleet {
    ships: BTreeMap<u64, ShipRecord>,
    current: Option<u64>,
    location: Value,
    stations: BTreeMap<u64, Value>,
    snapshot: Option<Value>,
    warnings: Vec<String>,
}

fn id(event: &Value, key: &str) -> Option<u64> {
    event.get(key)?.as_u64()
}
fn text<'a>(event: &'a Value, key: &str) -> Option<&'a str> {
    event.get(key)?.as_str()
}
fn stamp(event: &Value) -> Value {
    json!({"timestamp": event["timestamp"], "event": event["event"]})
}
fn copy(src: &Value, dst: &mut serde_json::Map<String, Value>, fields: &[(&str, &str)]) {
    for (from, to) in fields {
        if let Some(value) = src.get(*from).filter(|value| !value.is_null()) {
            dst.insert((*to).into(), value.clone());
        }
    }
}
fn location(event: &Value) -> Value {
    json!({"system": event.get("StarSystem").or_else(|| event.get("System")),
        "station": event.get("StationName"), "market_id": event.get("MarketID"),
        "observed": stamp(event)})
}
fn own_ship(event: &Value) -> bool {
    !text(event, "Ship").is_some_and(|ship| ship.ends_with("_fighter"))
        && !["Taxi", "Multicrew"].iter().any(|key| event[*key] == true)
        && !matches!(
            text(event, "Ship"),
            Some("testbuggy" | "combat_srv" | "taxi")
        )
}

impl Fleet {
    fn observe(&mut self, ship_id: u64, event: &Value) -> &mut ShipRecord {
        let record = self.ships.entry(ship_id).or_default();
        record.data.insert("ship_id".into(), json!(ship_id));
        record.data.insert("last_observed".into(), stamp(event));
        copy(
            event,
            &mut record.data,
            &[
                ("Ship", "type"),
                ("ShipType", "type"),
                ("ShipType_Localised", "type_name"),
                ("ShipName", "name"),
                ("ShipIdent", "ident"),
                ("Value", "value"),
                ("Hot", "hot"),
            ],
        );
        record
    }

    fn set_current(&mut self, ship_id: u64, event: &Value) {
        self.current = Some(ship_id);
        let loc = self.location.clone();
        let record = self.observe(ship_id, event);
        record.data.remove("transfer");
        record.data.remove("ownership_uncertain");
        if !loc.is_null() {
            record.data.insert("location".into(), loc);
        }
    }

    fn old_ship(&mut self, event: &Value) {
        if let Some(sold) = id(event, "SellShipID").or_else(|| id(event, "SellShipId")) {
            self.ships.remove(&sold);
            if self.current == Some(sold) {
                self.current = None;
            }
        }
        if let Some(stored) = id(event, "StoreShipID") {
            let loc = self.location.clone();
            let record = self.observe(
                stored,
                &json!({"timestamp":event["timestamp"],
                "event":event["event"], "ShipType":event["StoreOldShip"]}),
            );
            if !loc.is_null() {
                record.data.insert("location".into(), loc);
            }
        }
    }

    fn apply(&mut self, event: &Value) {
        let kind = text(event, "event").unwrap_or_default();
        match kind {
            "LoadGame" => {
                // A new session does not establish a fresh location.
                self.location = Value::Null;
                self.current = None;
                if own_ship(event)
                    && let Some(ship_id) = id(event, "ShipID")
                {
                    self.set_current(ship_id, event);
                }
            }
            "Loadout" if own_ship(event) => {
                if let Some(ship_id) = id(event, "ShipID") {
                    self.set_current(ship_id, event);
                    let record = self.ships.get_mut(&ship_id).expect("observed ship");
                    record.loadout = Some(event.clone());
                    record.changed = None;
                }
            }
            "Location" | "Docked" | "FSDJump" | "CarrierJump" => {
                self.location = location(event);
                if event["Docked"] == false || matches!(kind, "FSDJump") {
                    self.location["station"] = Value::Null;
                    self.location["market_id"] = Value::Null;
                }
                if let Some(market) = id(event, "MarketID") {
                    self.stations.insert(market, location(event));
                }
                if own_ship(event)
                    && event["OnFoot"] != true
                    && let Some(ship_id) = self.current
                {
                    let loc = self.location.clone();
                    let record = self.observe(ship_id, event);
                    record.data.insert("location".into(), loc);
                }
            }
            "Undocked" | "SupercruiseEntry" => {
                if !self.location.is_null() {
                    self.location["station"] = Value::Null;
                    self.location["market_id"] = Value::Null;
                    self.location["observed"] = stamp(event);
                    if let Some(ship_id) = self.current {
                        let loc = self.location.clone();
                        self.observe(ship_id, event)
                            .data
                            .insert("location".into(), loc);
                    }
                }
            }
            "ShipyardBuy" | "ShipyardSwap" => {
                self.old_ship(event);
                if kind == "ShipyardBuy" {
                    // ShipyardBuy has no ID for the new hull. Wait for ShipyardNew/Loadout.
                    self.current = None;
                } else if let Some(ship_id) = id(event, "ShipID") {
                    self.set_current(ship_id, event);
                }
            }
            "ShipyardNew" => {
                if let Some(ship_id) = id(event, "NewShipID") {
                    self.set_current(ship_id, event);
                }
            }
            "ShipyardSell" | "SellShipOnRebuy" => self.old_ship(event),
            "SetUserShipName" => {
                if let Some(ship_id) = id(event, "ShipID") {
                    self.observe(ship_id, event);
                }
            }
            "StoredShips" => {
                let here = event["ShipsHere"].as_array();
                let remote = event["ShipsRemote"].as_array();
                // Never use a torn/partial snapshot to delete ownership records.
                let complete = here.is_some()
                    && remote.is_some()
                    && here
                        .into_iter()
                        .flatten()
                        .chain(remote.into_iter().flatten())
                        .all(|entry| id(entry, "ShipID").is_some());
                let present: Vec<u64> = here
                    .into_iter()
                    .flatten()
                    .chain(remote.into_iter().flatten())
                    .filter_map(|entry| id(entry, "ShipID"))
                    .collect();
                if complete {
                    self.ships.retain(|ship_id, _| {
                        Some(*ship_id) == self.current || present.contains(ship_id)
                    });
                    self.snapshot = Some(stamp(event));
                } else {
                    self.warnings.push(
                        "A partial StoredShips snapshot was not used to remove ships.".into(),
                    );
                }
                if let Some(market) = id(event, "MarketID") {
                    self.stations.insert(market, location(event));
                }
                for (entries, is_here) in [(here, true), (remote, false)] {
                    for entry in entries.into_iter().flatten() {
                        let Some(ship_id) = id(entry, "ShipID") else {
                            continue;
                        };
                        let record = self.observe(ship_id, entry);
                        record.data.remove("ownership_uncertain");
                        copy(entry, &mut record.data, &[("Name", "name")]);
                        record.data.insert("last_observed".into(), stamp(event));
                        let mut loc = if is_here {
                            location(event)
                        } else {
                            json!({"system": entry["StarSystem"], "station": entry["StationName"],
                                "market_id":entry["ShipMarketID"], "observed":stamp(event)})
                        };
                        if entry["InTransit"] == true {
                            loc = Value::Null;
                            record.data.insert("transfer".into(), json!({"completion_confirmed":false,
                                "last_observed":stamp(event), "note":"Reported in transit; arrival is unconfirmed."}));
                        } else {
                            record.data.remove("transfer");
                        }
                        record.data.insert("location".into(), loc);
                    }
                }
            }
            "ShipyardTransfer" => {
                if let Some(ship_id) = id(event, "ShipID") {
                    let to = self.location.clone();
                    let record = self.observe(ship_id, event);
                    record.data.insert("location".into(), Value::Null);
                    record.data.insert("transfer".into(), json!({"requested":stamp(event),
                        "from":{"system":event["System"], "market_id":event["ShipMarketID"]},
                        "to":to, "estimated_duration_seconds":event["TransferTime"],
                        "completion_confirmed":false,
                        "note":"Transfer requested, not confirmed arrived. Elapsed ETA is not proof of arrival."}));
                }
            }
            "ModuleBuy"
            | "ModuleSell"
            | "ModuleStore"
            | "ModuleRetrieve"
            | "ModuleSwap"
            | "MassModuleStore"
            | "EngineerCraft"
            | "EngineerLegacyConvert" => {
                if let Some(ship_id) = id(event, "ShipID").or(self.current)
                    && let Some(record) = self.ships.get_mut(&ship_id)
                {
                    record.changed = Some(stamp(event));
                }
            }
            "Died" => {
                // Rebuy may replace the hull; do not certify its old fit or location in the interim.
                if let Some(record) = self
                    .current
                    .and_then(|ship_id| self.ships.get_mut(&ship_id))
                {
                    record.changed = Some(stamp(event));
                    record.data.insert("location".into(), Value::Null);
                    record
                        .data
                        .insert("ownership_uncertain".into(), json!(true));
                }
            }
            _ => {}
        }
    }

    fn result(mut self, ship_id: Option<u64>, include_modules: bool) -> Vec<Value> {
        self.ships.iter_mut().filter(|(key, _)| ship_id.is_none_or(|wanted| wanted == **key))
            .map(|(key, record)| {
                let current = self.current == Some(*key);
                record.data.insert("is_current".into(), json!(current));
                if let Some(loc) = record.data.get_mut("location")
                    && loc["station"].is_null()
                    && let Some(market) = loc["market_id"].as_u64()
                    && let Some(station) = self.stations.get(&market)
                { loc["station"] = station["station"].clone(); }
                let loadout = record.loadout.as_ref().map(|snapshot| {
                    let modules = snapshot["Modules"].as_array();
                    let compact: Vec<Value> = modules.into_iter().flatten().map(|module| {
                        let mut value = serde_json::Map::new();
                        copy(module, &mut value, &[("Slot","slot"), ("Item","item"),
                            ("Engineering","engineering"), ("On","enabled"), ("Priority","priority")]);
                        Value::Object(value)
                    }).collect();
                    let equipment: Vec<_> = compact.iter().filter(|module| {
                        let slot = text(module, "slot").unwrap_or_default();
                        slot.contains("Hardpoint") || matches!(slot,"Armour"|"PowerPlant"|"PowerDistributor"|"MainEngines")
                            || text(module,"item").unwrap_or_default().contains("shieldgenerator")
                    }).cloned().collect();
                    let age = text(snapshot,"timestamp").and_then(|t|DateTime::parse_from_rfc3339(t).ok())
                        .map(|t|(Utc::now()-t.with_timezone(&Utc)).num_seconds().max(0));
                    json!({"observed":stamp(snapshot), "age_seconds":age,
                        "complete_snapshot":modules.is_some(), "last_change_after_snapshot":record.changed,
                        "possibly_outdated":record.changed.is_some(), "module_count":modules.map(Vec::len),
                        "combat_relevant_equipment":equipment,
                        "modules":if include_modules {Some(compact)} else {None}})
                });
                record.data.insert("loadout".into(), json!(loadout));
                Value::Object(record.data.clone())
            }).collect()
    }
}

fn relevant(kind: &str) -> bool {
    matches!(
        kind,
        "LoadGame"
            | "Loadout"
            | "Location"
            | "Docked"
            | "Undocked"
            | "FSDJump"
            | "CarrierJump"
            | "SupercruiseEntry"
            | "ShipyardBuy"
            | "ShipyardNew"
            | "ShipyardSwap"
            | "ShipyardSell"
            | "SellShipOnRebuy"
            | "StoredShips"
            | "ShipyardTransfer"
            | "SetUserShipName"
            | "Died"
            | "ModuleBuy"
            | "ModuleSell"
            | "ModuleStore"
            | "ModuleRetrieve"
            | "ModuleSwap"
            | "MassModuleStore"
            | "EngineerCraft"
            | "EngineerLegacyConvert"
    )
}

/// Scan all available journals, scoped to the most recently observed commander and galaxy.
/// Only relevant events are retained; no game input, external service or persistent cache.
pub fn inventory(
    journal: &JournalDir,
    ship_id: Option<u64>,
    include_modules: bool,
) -> Result<Value> {
    let mut paths = fs::read_dir(&journal.dir)
        .context("listing fleet journals")?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("Journal.") && name.ends_with(".log")
            })
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut rows = Vec::new();
    let mut identity: Option<(String, String)> = None;
    let mut galaxy = "unknown".to_string();
    let mut latest = None;
    let mut skipped = 0usize;
    let mut unreadable = 0usize;
    let mut unscoped = 0usize;
    let mut ordinal = 0usize;
    for path in &paths {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        for line in BufReader::new(file).lines() {
            ordinal += 1;
            let Ok(line) = line else {
                skipped += 1;
                continue;
            };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(&line) else {
                skipped += 1;
                continue;
            };
            let kind = text(&event, "event").unwrap_or_default();
            if kind == "Fileheader" {
                if event["part"].as_u64().unwrap_or(1) == 1 {
                    identity = None;
                }
                galaxy = match text(&event, "gameversion") {
                    Some(version) if version.starts_with("3.") => "legacy",
                    Some(version) if version.starts_with("4.") => "live",
                    _ => "unknown",
                }
                .into();
            }
            let Some(timestamp) =
                text(&event, "timestamp").and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            else {
                skipped += 1;
                continue;
            };
            if kind == "Commander" || kind == "LoadGame" {
                let name = text(&event, "Name").or_else(|| text(&event, "Commander"));
                if let Some(fid) = text(&event, "FID") {
                    identity = Some((fid.to_string(), name.unwrap_or_default().to_string()));
                } else if let Some(name) = name
                    && identity.as_ref().is_none_or(|(_, known)| known != name)
                {
                    identity = Some((format!("name:{name}"), name.to_string()));
                }
                if let Some(who) = &identity {
                    let candidate = (timestamp, ordinal, who.clone(), galaxy.clone());
                    if latest.as_ref().is_none_or(|existing| &candidate > existing) {
                        latest = Some(candidate);
                    }
                }
            }
            if relevant(kind) {
                if let Some(who) = &identity {
                    rows.push((timestamp, ordinal, who.0.clone(), galaxy.clone(), event));
                } else {
                    unscoped += 1;
                }
            }
        }
    }
    let mut fleet = Fleet::default();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let commander = latest.as_ref().map(|(_, _, who, _)| who.1.clone());
    let selected_galaxy = latest.as_ref().map(|(_, _, _, galaxy)| galaxy.clone());
    let mut first = None;
    let mut last = None;
    let mut events = 0usize;
    if let Some((_, _, who, scope)) = latest {
        for (_, _, fid, galaxy, event) in rows {
            if fid == who.0 && galaxy == scope {
                first.get_or_insert(event["timestamp"].clone());
                last = Some(event["timestamp"].clone());
                events += 1;
                fleet.apply(&event);
            }
        }
    }
    let snapshot = fleet.snapshot.clone();
    let current = fleet.current;
    fleet.warnings.push("Last-known journal evidence, not a live account query. Missing journals can hide sales, purchases or later modifications. Absence of a loadout is unknown equipment, not an empty fit.".into());
    if snapshot.is_none() {
        fleet.warnings.push("No complete StoredShips snapshot for this commander/galaxy. Fleet completeness is unknown; visit the shipyard to record a new inventory.".into());
    }
    if unreadable > 0 || skipped > 0 || unscoped > 0 {
        fleet.warnings.push("Some files/lines could not be read, parsed, timestamped or assigned to a commander; coverage is incomplete.".into());
    }
    fleet.warnings.sort();
    fleet.warnings.dedup();
    let warnings = fleet.warnings.clone();
    let ships = fleet.result(ship_id, include_modules);
    Ok(
        json!({"commander":commander,"galaxy":selected_galaxy,"retrieved_at_utc":Utc::now().to_rfc3339(),
        "current_ship_id":current,"ship_count":ships.len(),"ships":ships,
        "coverage":{"journal_files":paths.len(),"reconciled_events":events,"first_event":first,
            "last_event":last,"latest_complete_stored_ships":snapshot,
            "unreadable_files":unreadable,"skipped_lines":skipped,"unscoped_events":unscoped,
            "complete_account_history":false},"warnings":warnings}),
    )
}

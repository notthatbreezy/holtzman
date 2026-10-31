//! Read-only briefing and measurable objective facts; never performs a game action.
use crate::journal::JournalDir;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn facts(journal: &JournalDir) -> Value {
    let mut missions = BTreeMap::<u64, Value>::new();
    let mut rank = json!({});
    let mut progress = json!({});
    let mut engineers = json!([]);
    let mut materials = BTreeMap::<String, u64>::new();
    let mut paths: Vec<_> = std::fs::read_dir(&journal.dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy().starts_with("Journal.") && n.to_string_lossy().ends_with(".log")
            })
        })
        .collect();
    paths.sort();
    let mut commander = None;
    let history = paths.into_iter().flat_map(|path| {
        std::fs::read_to_string(path)
            .ok()
            .into_iter()
            .flat_map(|text| {
                text.lines()
                    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                    .collect::<Vec<_>>()
            })
    });
    for event in history {
        if event["event"] == "Commander" {
            let identity = event["FID"]
                .as_str()
                .map(String::from)
                .or_else(|| event["Name"].as_str().map(String::from));
            if commander.is_some() && identity != commander {
                missions.clear();
                rank = json!({});
                progress = json!({});
                engineers = json!([]);
                materials.clear();
            }
            commander = identity;
        }
        match event["event"].as_str().unwrap_or("") {
            "Missions" => {
                let active: Vec<_> = event["Active"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m["MissionID"].as_u64())
                    .collect();
                missions.retain(|id, _| active.contains(id));
                for id in active {
                    missions.entry(id).or_insert(
                        json!({"MissionID":id,"details":"not recorded in current journal"}),
                    );
                }
            }
            "MissionAccepted" => {
                if let Some(id) = event["MissionID"].as_u64() {
                    missions.insert(id, event.clone());
                }
            }
            "MissionCompleted" | "MissionAbandoned" | "MissionFailed" => {
                if let Some(id) = event["MissionID"].as_u64() {
                    missions.remove(&id);
                }
            }
            "Rank" | "Promotion" => {
                if let Some(values) = event.as_object() {
                    for (k, v) in values {
                        if k != "timestamp" && k != "event" {
                            rank[k] = v.clone();
                        }
                    }
                }
            }
            "Progress" => {
                progress = event.clone();
            }
            "EngineerProgress" => {
                if let Some(rows) = event["Engineers"].as_array() {
                    engineers = json!(rows);
                } else if let Some(rows) = engineers.as_array_mut() {
                    let name = event["Engineer"].clone();
                    rows.retain(|e| e["Engineer"] != name);
                    rows.push(event.clone());
                }
            }
            "Materials" => {
                materials.clear();
                for class in ["Raw", "Manufactured", "Encoded"] {
                    for item in event[class].as_array().into_iter().flatten() {
                        if let Some(name) = item["Name"].as_str() {
                            materials.insert(
                                name.to_ascii_lowercase(),
                                item["Count"].as_u64().unwrap_or(0),
                            );
                        }
                    }
                }
            }
            "MaterialCollected" => {
                if let Some(name) = event["Name"].as_str() {
                    *materials.entry(name.to_ascii_lowercase()).or_default() +=
                        event["Count"].as_u64().unwrap_or(0);
                }
            }
            "MaterialDiscarded" => {
                if let Some(name) = event["Name"].as_str() {
                    let count = materials.entry(name.to_ascii_lowercase()).or_default();
                    *count = count.saturating_sub(event["Count"].as_u64().unwrap_or(0));
                }
            }
            "EngineerCraft" => {
                for item in event["Ingredients"].as_array().into_iter().flatten() {
                    if let Some(name) = item["Name"].as_str() {
                        let count = materials.entry(name.to_ascii_lowercase()).or_default();
                        *count = count.saturating_sub(item["Count"].as_u64().unwrap_or(0));
                    }
                }
            }
            _ => {}
        }
    }
    let mut missions: Vec<_> = missions.into_values().collect();
    missions.sort_by_key(|m| m["Expiry"].as_str().unwrap_or("9999").to_string());
    json!({"missions":missions,"rank":rank,"rank_progress":progress,"engineers":engineers,"materials":materials})
}

pub fn measure(measure: &Value, journal: &JournalDir) -> Value {
    let facts = facts(journal);
    let value = match measure["kind"].as_str() {
        Some("credits") => journal
            .summary()
            .ok()
            .map(|s| s["credits"].clone())
            .unwrap_or(Value::Null),
        Some("rank") => {
            let name = measure["name"].as_str().unwrap_or("");
            json!({"rank":facts["rank"][name],"progress":facts["rank_progress"][name]})
        }
        Some("engineer") => facts["engineers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["Engineer"] == measure["name"])
            .cloned()
            .unwrap_or(Value::Null),
        Some("materials") => {
            facts["materials"][measure["name"].as_str().unwrap_or("").to_ascii_lowercase()].clone()
        }
        _ => Value::Null,
    };
    json!({"value":value,"target":measure["target"],"source":"journal/status; absent values are unknown"})
}

pub fn briefing(journal: &JournalDir, objectives: Value, plan: Value) -> Value {
    let status = journal.summary().unwrap_or(json!({}));
    let facts = facts(journal);
    let spendable = status["credits"]
        .as_u64()
        .zip(status["ship"]["rebuy"].as_u64())
        .map(|(credits, rebuy)| credits.saturating_sub(rebuy));
    let mut options = Vec::new();
    for mission in facts["missions"].as_array().into_iter().flatten().take(2) {
        options.push(json!({"kind":"mission","reference":mission["MissionID"].as_u64().map(|id|format!("mission:{id}")),"destination":mission["DestinationSystem"],"station":mission["DestinationStation"],"payoff_credits":mission["Reward"],"observed":mission["timestamp"]}));
    }
    if options.len() < 3 && plan["state"] == "active" {
        options.push(json!({"kind":"resume_plan","plan":plan["id"],"observed":plan["used"]}));
    }
    if options.len() < 3
        && let Some(goal) = objectives["objectives"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|g| g["state"] == "active")
    {
        options.push(json!({"kind":"objective","text":goal["text"],"progress":goal["progress"],"observed":goal["created"]}));
    }
    json!({"ship":status["ship"],"location":status["location"],"spendable_after_rebuy":spendable,"cargo":journal.cargo().ok(),"missions":facts["missions"],"objectives":objectives,"options":options,"instruction":"Offer only grounded available choices. Captain chooses; no input is authorized by this briefing."})
}

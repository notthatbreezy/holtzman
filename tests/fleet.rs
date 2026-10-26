//! Historical fleet replay invariants: identity, ownership, freshness and transfers.
use holtzman::{fleet::inventory, journal::JournalDir};
use serde_json::{Value, json};
use std::fs;

fn event(at: u32, kind: &str, fields: Value) -> Value {
    let mut value = fields.as_object().cloned().unwrap();
    value.insert(
        "timestamp".into(),
        json!(format!("2026-01-01T00:{:02}:{:02}Z", at / 60, at % 60)),
    );
    value.insert("event".into(), json!(kind));
    Value::Object(value)
}
fn commander(at: u32, fid: &str) -> Value {
    event(at, "Commander", json!({"FID":fid,"Name":fid}))
}
fn loadout(at: u32, ship_id: u64) -> Value {
    event(
        at,
        "Loadout",
        json!({"ShipID":ship_id,"Ship":"vulture","ShipName":"Hunter",
        "Modules":[{"Slot":"LargeHardpoint1","Item":"hpt_pulselaser_fixed_large",
            "Engineering":{"BlueprintName":"Weapon_Efficient"}},
            {"Slot":"Slot01_Size5","Item":"int_shieldgenerator_size5_class5"}]}),
    )
}
fn snapshot(at: u32, ids: &[u64]) -> Value {
    event(
        at,
        "StoredShips",
        json!({"StarSystem":"Sol","StationName":"Galileo","MarketID":42,
        "ShipsHere":ids.iter().map(|id|json!({"ShipID":id,"ShipType":"vulture"})).collect::<Vec<_>>(),
        "ShipsRemote":[]}),
    )
}
fn write(dir: &std::path::Path, name: &str, rows: &[Value]) {
    fs::write(
        dir.join(name),
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
}
fn run(rows: Vec<Value>) -> Value {
    let temp = tempfile::tempdir().unwrap();
    write(temp.path(), "Journal.2026-01-01.log", &rows);
    inventory(&JournalDir::new(temp.path()), None, false).unwrap()
}
fn ids(result: &Value) -> Vec<u64> {
    result["ships"]
        .as_array()
        .unwrap()
        .iter()
        .map(|ship| ship["ship_id"].as_u64().unwrap())
        .collect()
}

#[test]
fn sold_ship_is_not_owned_even_if_its_loadout_is_in_another_file() {
    let temp = tempfile::tempdir().unwrap();
    // Event time wins over filename ordering. Sale's file sorts before the older loadout file.
    write(
        temp.path(),
        "Journal.a.log",
        &[
            commander(20, "A"),
            event(30, "ShipyardSell", json!({"SellShipID":7})),
        ],
    );
    write(
        temp.path(),
        "Journal.z.log",
        &[commander(1, "A"), loadout(2, 7)],
    );
    let result = inventory(&JournalDir::new(temp.path()), None, false).unwrap();
    assert!(ids(&result).is_empty());
}

#[test]
fn trade_in_and_rebuy_sale_remove_old_hulls_without_guessing_new_id() {
    let result = run(vec![
        commander(0, "A"),
        loadout(1, 1),
        event(
            2,
            "ShipyardBuy",
            json!({"ShipType":"anaconda","SellOldShip":"vulture","SellShipID":1}),
        ),
        event(
            3,
            "ShipyardNew",
            json!({"ShipType":"anaconda","NewShipID":2}),
        ),
        event(
            4,
            "ShipyardSwap",
            json!({"ShipType":"cobra","ShipID":3,"SellShipID":2}),
        ),
        event(5, "SellShipOnRebuy", json!({"SellShipId":3})),
    ]);
    assert!(ids(&result).is_empty());
    assert!(result["current_ship_id"].is_null());
}

#[test]
fn complete_snapshot_reconciles_missing_sales_but_preserves_current_ship() {
    let result = run(vec![
        commander(0, "A"),
        loadout(1, 1),
        loadout(2, 2),
        snapshot(3, &[3]),
    ]);
    assert_eq!(ids(&result), vec![2, 3]);
    assert_eq!(result["ships"][1]["location"]["station"], "Galileo");
    assert!(result["ships"][1]["loadout"].is_null());
}

#[test]
fn partial_snapshot_never_deletes_ownership() {
    let result = run(vec![
        commander(0, "A"),
        loadout(1, 1),
        loadout(2, 2),
        event(3, "StoredShips", json!({"ShipsHere":[]})),
    ]);
    assert_eq!(ids(&result), vec![1, 2]);
    assert!(result["coverage"]["latest_complete_stored_ships"].is_null());
}

#[test]
fn commander_and_live_legacy_scopes_do_not_mix_same_ship_id() {
    let result = run(vec![
        event(0, "Fileheader", json!({"part":1,"gameversion":"3.8"})),
        commander(1, "B"),
        loadout(2, 9),
        event(3, "Fileheader", json!({"part":1,"gameversion":"4.0"})),
        commander(4, "A"),
        loadout(5, 1),
        event(6, "Fileheader", json!({"part":1,"gameversion":"4.0"})),
        commander(7, "B"),
        event(
            8,
            "LoadGame",
            json!({"ShipID":1,"Ship":"sidewinder","Commander":"B"}),
        ),
    ]);
    assert_eq!(result["commander"], "B");
    assert_eq!(ids(&result), vec![1]);
    assert!(result["ships"][0]["loadout"].is_null());
    assert!(result["ships"][0]["name"].is_null());
}

#[test]
fn transfer_eta_does_not_become_confirmed_arrival() {
    let result = run(vec![
        commander(0, "A"),
        loadout(1, 1),
        event(
            2,
            "Docked",
            json!({"StarSystem":"Sol","StationName":"Galileo","MarketID":42}),
        ),
        event(
            3,
            "ShipyardTransfer",
            json!({"ShipID":2,"ShipType":"vulture","System":"Achenar","TransferTime":1}),
        ),
    ]);
    let ship = &result["ships"][1];
    assert!(ship["location"].is_null());
    assert_eq!(ship["transfer"]["completion_confirmed"], false);
    assert_eq!(ship["transfer"]["to"]["station"], "Galileo");
}

#[test]
fn later_snapshot_confirms_location_and_clears_pending_transfer() {
    let result = run(vec![
        commander(0, "A"),
        loadout(1, 1),
        event(2, "ShipyardTransfer", json!({"ShipID":2})),
        snapshot(3, &[2]),
    ]);
    assert_eq!(result["ships"][1]["location"]["system"], "Sol");
    assert!(result["ships"][1]["transfer"].is_null());
}

#[test]
fn module_change_invalidates_snapshot_until_new_loadout() {
    let mut rows = vec![
        commander(0, "A"),
        loadout(1, 1),
        event(
            2,
            "ModuleSell",
            json!({"ShipID":1,"Slot":"LargeHardpoint1"}),
        ),
    ];
    let result = run(rows.clone());
    assert_eq!(result["ships"][0]["loadout"]["possibly_outdated"], true);
    rows.push(loadout(3, 1));
    let result = run(rows);
    assert_eq!(result["ships"][0]["loadout"]["possibly_outdated"], false);
}

#[test]
fn filter_full_modules_and_independent_location_timestamps() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "Journal.a.log",
        &[
            commander(0, "A"),
            loadout(1, 1),
            event(
                2,
                "Docked",
                json!({"StarSystem":"Sol","StationName":"Galileo","MarketID":42}),
            ),
            event(3, "Undocked", json!({})),
        ],
    );
    let result = inventory(&JournalDir::new(temp.path()), Some(1), true).unwrap();
    let ship = &result["ships"][0];
    assert_eq!(
        ship["loadout"]["modules"][0]["engineering"]["BlueprintName"],
        "Weapon_Efficient"
    );
    assert_ne!(
        ship["location"]["observed"]["timestamp"],
        ship["loadout"]["observed"]["timestamp"]
    );
    assert!(ship["location"]["station"].is_null());
    assert_eq!(
        ship["loadout"]["combat_relevant_equipment"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let absent = inventory(&JournalDir::new(temp.path()), Some(99), false).unwrap();
    assert_eq!(absent["ship_count"], 0);
}

#[test]
fn torn_lines_do_not_hide_known_ships_or_claim_complete_coverage() {
    let temp = tempfile::tempdir().unwrap();
    write(
        temp.path(),
        "Journal.a.log",
        &[commander(0, "A"), loadout(1, 1)],
    );
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join("Journal.a.log"))
        .unwrap()
        .write_all(b"{torn")
        .unwrap();
    let result = inventory(&JournalDir::new(temp.path()), None, false).unwrap();
    assert_eq!(ids(&result), vec![1]);
    assert_eq!(result["coverage"]["skipped_lines"], 1);
    assert_eq!(result["coverage"]["complete_account_history"], false);
}

#[test]
fn stored_old_hull_keeps_fit_and_later_positive_reuse_does_not_resurrect_sold_fit() {
    let result = run(vec![
        commander(0, "A"),
        loadout(1, 1),
        event(
            2,
            "ShipyardSwap",
            json!({"ShipID":2,"ShipType":"cobra","StoreShipID":1,"StoreOldShip":"vulture"}),
        ),
        event(3, "ShipyardSell", json!({"SellShipID":1})),
        event(
            4,
            "ShipyardNew",
            json!({"NewShipID":1,"ShipType":"anaconda"}),
        ),
    ]);
    assert_eq!(result["ships"][0]["type"], "anaconda");
    assert!(result["ships"][0]["loadout"].is_null());
}

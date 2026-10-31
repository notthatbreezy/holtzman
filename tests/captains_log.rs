use holtzman::{
    captains_log::{EntryArgs, PlanArgs, Query, Store, sessions},
    journal::JournalDir,
};
use serde_json::{Value, json};
fn journal(dir: &std::path::Path, events: Vec<Value>) -> JournalDir {
    std::fs::write(
        dir.join("Journal.2026-01-01.log"),
        events.iter().map(|e| format!("{e}\n")).collect::<String>(),
    )
    .unwrap();
    JournalDir::new(dir)
}
#[test]
fn sessions_credit_earnings_incidents_and_gap_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let j = journal(
        dir.path(),
        vec![
            json!({"event":"LoadGame","timestamp":"2026-01-01T00:00:00Z","Credits":1000,"Ship":"cobra"}),
            json!({"event":"FSDJump","timestamp":"2026-01-01T00:01:00Z","StarSystem":"Sol","JumpDist":12.5}),
            json!({"event":"MarketSell","timestamp":"2026-01-01T00:02:00Z","TotalSale":200}),
            json!({"event":"MarketBuy","timestamp":"2026-01-01T00:03:00Z","TotalCost":50}),
            json!({"event":"HullDamage","timestamp":"2026-01-01T00:04:00Z","Health":0.2,"PlayerPilot":true}),
            json!({"event":"Shutdown","timestamp":"2026-01-01T00:05:00Z"}),
            json!({"event":"LoadGame","timestamp":"2026-01-02T00:00:00Z","Credits":1234}),
            json!({"event":"FSDJump","timestamp":"2026-01-02T01:00:00Z","StarSystem":"Achenar","JumpDist":20}),
        ],
    );
    let s = sessions(&j).unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(s[0]["end_credits"], 1234);
    assert_eq!(s[0]["earnings"], 200);
    assert_eq!(s[0]["spending"], 50);
    assert_eq!(s[0]["avoidable_incidents"].as_array().unwrap().len(), 1);
    assert_eq!(s[2]["jumps"], 1);
}
#[test]
fn idle_entries_are_attached_verbatim_and_scratch_is_durable() {
    let dir = tempfile::tempdir().unwrap();
    let j = journal(
        dir.path(),
        vec![json!({"event":"LoadGame","timestamp":"2026-01-01T00:00:00Z","Credits":1000})],
    );
    let store = Store::new(Some(dir.path().join("log")));
    let text = "AD Leonis, at last!";
    store
        .entry(
            EntryArgs {
                text: Some(text.into()),
                author: Some("captain".into()),
                scratch: None,
            },
            &j,
        )
        .unwrap();
    let query = || Query {
        range: Some("last".into()),
        include: None,
    };
    assert_eq!(
        store.query(query(), &j).unwrap()["entries"][0]["text"],
        text
    );
    store
        .entry(
            EntryArgs {
                text: None,
                author: None,
                scratch: Some(true),
            },
            &j,
        )
        .unwrap();
    assert!(
        store.query(query(), &j).unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn plans_use_real_provider_shape_and_never_accept_arbitrary_reference() {
    let dir = tempfile::tempdir().unwrap();
    let j = journal(dir.path(), vec![]);
    let store = Store::new(Some(dir.path().join("log")));
    assert!(
        store
            .plan(
                PlanArgs {
                    op: "set".into(),
                    reference: Some("invented".into())
                },
                &j
            )
            .is_err()
    );
    let reference=store.cache_route(json!({"legs":[{"source":{"system":"Sol","station":"Galileo"},"destination":{"system":"Alpha Centauri","station":"Hutton Orbital"},"commodities":[{"name":"Tea","amount":10}]}]}));
    let plan = store
        .plan(
            PlanArgs {
                op: "set".into(),
                reference: Some(reference),
            },
            &j,
        )
        .unwrap();
    assert_eq!(plan["legs"].as_array().unwrap().len(), 4);
    store.observe(&json!({"event":"Docked","StarSystem":"Sol","StationName":"Galileo"}));
    let plan = store
        .plan(
            PlanArgs {
                op: "get".into(),
                reference: None,
            },
            &j,
        )
        .unwrap();
    assert_eq!(plan["index"], 1);
}

#[test]
fn multi_leg_plan_counts_partial_trades_only_at_the_right_station() {
    let dir = tempfile::tempdir().unwrap();
    let j = journal(dir.path(), vec![]);
    let store = Store::new(Some(dir.path().join("log")));
    let reference=store.cache_route(json!({"legs":[
        {"source":{"system":"Sol","station":"A"},"destination":{"system":"Alpha","station":"B"},"commodities":[{"name":"Tea","amount":10}]},
        {"source":{"system":"Alpha","station":"B"},"destination":{"system":"Beta","station":"C"},"commodities":[{"name":"Gold","amount":5}]}
    ]}));
    store
        .plan(
            PlanArgs {
                op: "set".into(),
                reference: Some(reference),
            },
            &j,
        )
        .unwrap();
    let get = || {
        store
            .plan(
                PlanArgs {
                    op: "get".into(),
                    reference: None,
                },
                &j,
            )
            .unwrap()
    };
    store.observe(&json!({"event":"Docked","StarSystem":"Sol","StationName":"A"}));
    store.observe(&json!({"event":"MarketBuy","Type":"Tea","Count":4}));
    assert_eq!(get()["index"], 1);
    store.observe(&json!({"event":"Undocked"}));
    store.observe(&json!({"event":"MarketBuy","Type":"Tea","Count":6}));
    assert_eq!(get()["index"], 1);
    store.observe(&json!({"event":"Docked","StarSystem":"Sol","StationName":"A"}));
    store.observe(&json!({"event":"MarketBuy","Type":"Tea","Count":6}));
    assert_eq!(get()["index"], 2);
    store.observe(&json!({"event":"Docked","StarSystem":"Alpha","StationName":"B"}));
    store.observe(&json!({"event":"MarketSell","Type":"Tea","Count":10}));
    assert_eq!(
        get()["legs"][get()["index"].as_u64().unwrap() as usize]["kind"],
        "buy"
    );
    store.observe(&json!({"event":"MarketBuy","Type":"Gold","Count":5}));
    store.observe(&json!({"event":"Docked","StarSystem":"Beta","StationName":"C"}));
    store.observe(&json!({"event":"MarketSell","Type":"Gold","Count":5}));
    assert_eq!(get()["state"], "finished");
}

#[test]
fn completed_plan_receipt_is_visible_before_the_next_journal_line() {
    let dir = tempfile::tempdir().unwrap();
    let j = journal(
        dir.path(),
        vec![json!({"event":"LoadGame","timestamp":"2026-01-01T00:00:00Z","Credits":1000})],
    );
    let store = Store::new(Some(dir.path().join("log")));
    let reference=store.cache_route(json!({"legs":[{"source":{"system":"Sol","station":"A"},"destination":{"system":"Alpha","station":"B"},"commodities":[{"name":"Tea","amount":1}]}]}));
    store
        .plan(
            PlanArgs {
                op: "set".into(),
                reference: Some(reference),
            },
            &j,
        )
        .unwrap();
    for event in [
        json!({"event":"Docked","StarSystem":"Sol","StationName":"A"}),
        json!({"event":"MarketBuy","Type":"Tea","Count":1}),
        json!({"event":"Docked","StarSystem":"Alpha","StationName":"B"}),
        json!({"event":"MarketSell","Type":"Tea","Count":1}),
    ] {
        store.observe(&event);
    }
    let result = store
        .query(
            Query {
                range: Some("last".into()),
                include: None,
            },
            &j,
        )
        .unwrap();
    assert_eq!(result["narrative"][0]["label"], "Completed plan receipt");
}

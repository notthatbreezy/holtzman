use holtzman::{
    events::{Bus, Tail},
    journal::JournalDir,
};
use serde_json::json;
use std::io::Write;
#[test]
fn ring_resume_reset_and_ptt_snapshot_are_consistent() {
    let bus = Bus::default();
    bus.snapshot(json!({"ptt":{"down":false}}));
    bus.publish("input", "ptt", json!({"down":true}));
    assert_eq!(
        bus.since(None, &[])["events"][0]["data"]["ptt"]["down"],
        true
    );
    bus.status_event(
        "flag",
        json!({"flag":"LightsOn","on":true}),
        &json!({"Flags":(1<<24)|(1<<8),"GuiFocus":0}),
    );
    assert_eq!(
        bus.since(None, &[])["snapshot"]["status"]["Flags"],
        (1 << 24) | (1 << 8)
    );
    assert_eq!(
        bus.since(Some(0), &[])["events"].as_array().unwrap().len(),
        2
    );
    for _ in 0..2001 {
        bus.publish("test", "test", json!({}));
    }
    assert_eq!(bus.since(Some(0), &[])["reset"], true);
    assert_eq!(bus.since(Some(9999), &[])["reset"], true);
}
#[test]
fn tail_keeps_partial_lines_and_drains_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let journal = JournalDir::new(dir.path());
    let old = dir.path().join("Journal.2026-01-01.log");
    std::fs::write(&old, "{\"event\":\"Startup\"}\n{\"event\":\"Partial").unwrap();
    let mut tail = Tail::default();
    assert!(tail.poll(&journal).unwrap().is_empty());
    let mut file = std::fs::OpenOptions::new().append(true).open(&old).unwrap();
    writeln!(file, "\"}}").unwrap();
    assert_eq!(tail.poll(&journal).unwrap()[0]["event"], "Partial");
    writeln!(file, "{{\"event\":\"Shutdown\"}}").unwrap();
    std::fs::write(
        dir.path().join("Journal.2026-01-02.log"),
        "{\"event\":\"LoadGame\"}\n",
    )
    .unwrap();
    let events = tail.poll(&journal).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event"], "Shutdown");
    assert_eq!(events[1]["event"], "LoadGame");
}

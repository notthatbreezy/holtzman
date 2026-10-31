use holtzman::{
    events::Bus,
    ptt::{Config, Reader, validate_input},
};
use std::sync::Arc;
#[test]
fn fallback_and_scripted_edges_do_not_capture_devices() {
    let bus = Arc::new(Bus::default());
    let reader = Reader::new(
        Config {
            inputs: vec!["Joy:T16000M:5".into(), "Mouse5".into()],
            ..Default::default()
        },
        bus.clone(),
    );
    reader.select(&["Mouse5".into()]);
    assert_eq!(reader.snapshot()["input"], "Mouse5");
    reader.edge("Mouse5", true);
    assert_eq!(bus.since(None, &[])["snapshot"]["ptt"]["down"], true);
    reader.select(&["Mouse5".into(), "Joy:T16000M".into()]);
    assert_eq!(reader.snapshot()["input"], "Joy:T16000M:5");
    assert_eq!(reader.snapshot()["down"], false);
    for input in [
        "Key_A",
        "Key_Enter",
        "Key_Space",
        "Key_LeftAlt",
        "Key_LeftShift",
        "Key_7",
        "Joy:Thing:0",
    ] {
        assert!(validate_input(input).is_err());
    }
    reader.learn();
    reader.edge("Key_A", true);
    assert!(reader.snapshot()["learned"].is_null());
    reader.edge("Key_F13", true);
    assert_eq!(reader.snapshot()["learned"], "Key_F13");
}

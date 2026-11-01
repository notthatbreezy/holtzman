use holtzman::{
    events::Bus,
    ptt::{Config, Reader, keyboard_input, validate_input},
};
use std::sync::Arc;

#[test]
fn joystick_learning_ignores_held_levels_and_requires_release_then_press() {
    let reader = Reader::new(Config::default(), Arc::new(Bus::default()));
    let held = "Joy:Saitek Pro Flight X-56 Rhino Stick:15";
    let wanted = "Joy:Saitek Pro Flight X-56 Rhino Stick:5";
    reader.learn();
    // The first report appears after learn mode registers the HID class.
    for _ in 0..3 {
        reader.edge(held, true);
        reader.edge(wanted, false);
    }
    assert!(reader.snapshot()["learned"].is_null());
    reader.edge(wanted, true);
    assert_eq!(reader.snapshot()["learned"], wanted);
    // Starting learn again while controls remain down cannot reuse old releases.
    reader.learn();
    reader.edge(held, true);
    reader.edge(wanted, true);
    assert!(reader.snapshot()["learned"].is_null());
    reader.edge(wanted, false);
    reader.edge(wanted, true);
    assert_eq!(reader.snapshot()["learned"], wanted);
}

#[test]
fn a_first_held_hid_report_and_keyboard_repeat_are_not_learning_presses() {
    let reader = Reader::new(Config::default(), Arc::new(Bus::default()));
    reader.edge("Key_F13", true);
    reader.learn();
    reader.edge("Key_F13", true);
    reader.edge("Joy:New device:5", true);
    assert!(reader.snapshot()["learned"].is_null());
    reader.edge("Key_F13", false);
    reader.edge("Key_F13", true);
    assert_eq!(reader.snapshot()["learned"], "Key_F13");
}

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

#[test]
fn keypad_enter_is_distinct_on_press_and_release() {
    assert!(validate_input("Key_NumpadEnter").is_ok());
    assert!(validate_input("Key_Enter").is_err());
    assert_eq!(
        keyboard_input(0x0d, 0x02).as_deref(),
        Some("Key_NumpadEnter")
    );
    assert_eq!(
        keyboard_input(0x0d, 0x03).as_deref(),
        Some("Key_NumpadEnter")
    );
    for flags in [0, 1, 4, 5, 6, 7] {
        assert_eq!(keyboard_input(0x0d, flags), None);
    }
    assert_eq!(keyboard_input(0x7c, 0).as_deref(), Some("Key_F13"));
    let bus = Arc::new(Bus::default());
    let reader = Reader::new(
        Config {
            inputs: vec!["Key_NumpadEnter".into()],
            ..Default::default()
        },
        bus.clone(),
    );
    reader.select(&["Key_NumpadEnter".into()]);
    for flags in [2, 3] {
        let name = keyboard_input(0x0d, flags).unwrap();
        reader.edge(&name, flags & 1 == 0);
        assert_eq!(reader.snapshot()["down"], flags == 2);
    }
    assert_eq!(
        bus.since(Some(1), &[])["events"].as_array().unwrap().len(),
        2
    );
}

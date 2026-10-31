//! All input is recorded, including simulated Status.json updates. No Windows input APIs.
use holtzman::{
    actions::{self, Args, State},
    bindings::Bindings,
    config::Config,
    input::{CursorPos, KeySender, Recorder},
    journal::JournalDir,
    keys::ScanCode,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

struct Cockpit {
    recorder: Recorder,
    status: std::path::PathBuf,
    flip: bool,
    modifiers: bool,
    change_on_focus: Option<u64>,
}
impl KeySender for Cockpit {
    fn modifiers_down(&self) -> bool {
        self.modifiers
    }
    fn key_down(&self, key: ScanCode) -> anyhow::Result<()> {
        self.recorder.key_down(key)
    }
    fn key_up(&self, key: ScanCode) -> anyhow::Result<()> {
        self.recorder.key_up(key)?;
        if self.flip {
            let mut status: Value = serde_json::from_slice(&std::fs::read(&self.status)?)?;
            status["Flags"] = json!(status["Flags"].as_u64().unwrap() ^ (1 << 6));
            std::fs::write(&self.status, status.to_string())?;
        }
        Ok(())
    }
    fn focus_game(&self) -> anyhow::Result<()> {
        self.recorder.focus_game()?;
        if let Some(flags) = self.change_on_focus {
            std::fs::write(
                &self.status,
                json!({"Flags":flags,"GuiFocus":0}).to_string(),
            )?;
        }
        Ok(())
    }
    fn set_clipboard(&self, s: &str) -> anyhow::Result<()> {
        self.recorder.set_clipboard(s)
    }
    fn move_mouse(&self, x: f64, y: f64) -> anyhow::Result<()> {
        self.recorder.move_mouse(x, y)
    }
    fn left_button(&self, down: bool) -> anyhow::Result<()> {
        self.recorder.left_button(down)
    }
    fn cursor(&self) -> anyhow::Result<CursorPos> {
        self.recorder.cursor()
    }
}
fn fixture(status: Value) -> (tempfile::TempDir, Config, Bindings, Cockpit) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Status.json");
    std::fs::write(&path, status.to_string()).unwrap();
    let mut config = Config::generate();
    config.dry_run = true;
    config.tap_ms = 1;
    let bindings=Bindings::parse(r#"<Root><DeployHardpointToggle><Primary Device="Keyboard" Key="Key_H"/><ToggleOn Value="1"/></DeployHardpointToggle><DeployHeatSink><Primary Device="Keyboard" Key="Key_V"/></DeployHeatSink><LandingGearToggle><Primary Device="Keyboard" Key="Key_G"/></LandingGearToggle></Root>"#).unwrap();
    (
        dir,
        config,
        bindings,
        Cockpit {
            recorder: Recorder::default(),
            status: path,
            flip: true,
            modifiers: false,
            change_on_focus: None,
        },
    )
}
#[tokio::test]
async fn toggles_once_and_already_does_not_press() {
    let (dir, config, bindings, sender) = fixture(json!({"Flags":1<<24,"GuiFocus":0}));
    let journal = JournalDir::new(dir.path());
    let args = Args {
        action: "hardpoints".into(),
        state: Some(State::On),
    };
    let result = actions::execute(&args, &config, &bindings, &journal, &sender).await;
    assert_eq!(result["outcome"], "done");
    assert_eq!(
        sender
            .recorder
            .entries()
            .iter()
            .filter(|s| s.starts_with("down "))
            .count(),
        1
    );
    let count = sender.recorder.entries().len();
    let result = actions::execute(&args, &config, &bindings, &journal, &sender).await;
    assert_eq!(result["outcome"], "already");
    assert_eq!(sender.recorder.entries().len(), count);
}
#[tokio::test]
async fn unsafe_and_incomplete_states_never_press() {
    for status in [
        json!({}),
        json!({"Flags":1<<24}),
        json!({"Flags":0,"GuiFocus":0}),
        json!({"Flags":1<<26,"GuiFocus":0}),
        json!({"Flags":(1<<24)|1,"GuiFocus":0}),
        json!({"Flags":(1<<24)|2,"GuiFocus":0}),
        json!({"Flags":1<<24,"GuiFocus":6}),
        json!({"Flags":(1<<24)|(1<<17),"GuiFocus":0}),
    ] {
        let (dir, c, b, s) = fixture(status);
        let result = actions::execute(
            &Args {
                action: "hardpoints".into(),
                state: Some(State::On),
            },
            &c,
            &b,
            &JournalDir::new(dir.path()),
            &s,
        )
        .await;
        assert_eq!(result["outcome"], "refused", "{result}");
        assert!(s.recorder.entries().is_empty());
    }
}
#[tokio::test]
async fn binding_hold_modifier_and_focus_races_fail_safely() {
    let (dir, c, mut b, mut s) = fixture(json!({"Flags":1<<24,"GuiFocus":0}));
    let j = JournalDir::new(dir.path());
    let args = Args {
        action: "hardpoints".into(),
        state: Some(State::On),
    };
    b.hold_mode.insert("DeployHardpointToggle".into());
    assert_eq!(
        actions::execute(&args, &c, &b, &j, &s).await["outcome"],
        "refused"
    );
    b.hold_mode.clear();
    s.modifiers = true;
    assert_eq!(
        actions::execute(&args, &c, &b, &j, &s).await["outcome"],
        "refused"
    );
    s.modifiers = false;
    s.change_on_focus = Some((1 << 24) | (1 << 6));
    assert_eq!(
        actions::execute(&args, &c, &b, &j, &s).await["outcome"],
        "already"
    );
    assert!(!s.recorder.entries().iter().any(|s| s.starts_with("down ")));
    assert_eq!(
        actions::execute(
            &Args {
                action: "chaff".into(),
                state: None
            },
            &c,
            &b,
            &j,
            &s
        )
        .await["outcome"],
        "unbound"
    );
}
#[tokio::test]
async fn cancelling_a_chord_releases_all_inputs() {
    let sender = Arc::new(Recorder::default());
    let copy = sender.clone();
    let task = tokio::spawn(async move {
        holtzman::input::press(
            copy.as_ref(),
            holtzman::keys::scan_code("Key_H").unwrap(),
            &[holtzman::keys::scan_code("Key_LeftControl").unwrap()],
            Duration::from_secs(30),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    task.abort();
    let _ = task.await;
    let entries = sender.entries();
    assert_eq!(
        entries.iter().filter(|s| s.starts_with("down ")).count(),
        entries.iter().filter(|s| s.starts_with("up ")).count()
    );
}

#[test]
fn cancellation_records_remain_bound_to_the_target_id() {
    let registry = Arc::new(actions::Registry::default());
    let mut first = registry.start("plot_route");
    let id = registry.cancel().unwrap();
    first.finish("cancelled");
    let second = registry.start("ship_action");
    assert_eq!(registry.record(&id)["state"], "cancelled");
    assert_eq!(registry.status()["state"], "running");
    assert!(!second.token.is_cancelled());
}

//! Run keypress recipes against the game.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::{
    bindings::Bindings,
    config::{Config, Step},
    input::{KeySender, click, press},
    journal::{JournalDir, gui},
    keys::scan_code,
};

pub struct Pilot<'a> {
    pub config: &'a Config,
    pub bindings: &'a Bindings,
    pub journal: &'a JournalDir,
    pub sender: &'a dyn KeySender,
}

impl Pilot<'_> {
    fn tap(&self, hold_ms: Option<u64>) -> Duration {
        Duration::from_millis(hold_ms.unwrap_or(self.config.tap_ms))
    }

    pub async fn press_binding(&self, name: &str, hold_ms: Option<u64>) -> Result<String> {
        let chord = self.bindings.get(name)?;
        let (key, mods) = chord.scan_codes()?;
        press(self.sender, key, &mods, self.tap(hold_ms)).await?;
        Ok(format!("pressed {name} ({chord})"))
    }

    async fn run_step(&self, step: &Step, target: &str) -> Result<()> {
        match step {
            Step::Binding { name, hold_ms } => {
                self.press_binding(name, *hold_ms).await?;
            }
            Step::Key {
                key,
                modifiers,
                hold_ms,
            } => {
                let k = scan_code(key).ok_or_else(|| anyhow::anyhow!("unknown key {key}"))?;
                let mods = modifiers
                    .iter()
                    .map(|m| scan_code(m).ok_or_else(|| anyhow::anyhow!("unknown key {m}")))
                    .collect::<Result<Vec<_>>>()?;
                press(self.sender, k, &mods, self.tap(*hold_ms)).await?;
            }
            Step::PasteTarget => {
                self.sender.set_clipboard(target)?;
                let ctrl = scan_code("Key_LeftControl").expect("ctrl");
                let v = scan_code("Key_V").expect("v");
                press(self.sender, v, &[ctrl], self.tap(None)).await?;
            }
            Step::Wait { ms } => tokio::time::sleep(Duration::from_millis(*ms)).await,
            Step::Click { target, hold_ms } => {
                let t = self.config.click_targets.get(target).ok_or_else(|| {
                    anyhow::anyhow!("click target `{target}` hasn't been recorded")
                })?;
                click(self.sender, t.x, t.y, self.tap(*hold_ms)).await?;
            }
            Step::WaitForGui { focus, timeout_ms } => {
                let deadline = Instant::now() + Duration::from_millis(*timeout_ms);
                loop {
                    if self.journal.gui_focus() == Some(*focus) {
                        break;
                    }
                    if self.config.dry_run || cfg!(not(windows)) {
                        // No game to wait on.
                        break;
                    }
                    if Instant::now() > deadline {
                        let now = self.journal.gui_focus().unwrap_or(-1);
                        bail!(
                            "timed out waiting for GUI focus {} (currently {})",
                            gui::name(*focus),
                            gui::name(now)
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        Ok(())
    }

    /// Plot a route to `system` via the galaxy map, then confirm it landed in
    /// NavRoute.json.
    pub async fn plot_route(&self, system: &str) -> Result<String> {
        let system = system.trim();
        if system.is_empty() {
            bail!("no system name given");
        }
        if self.journal.gui_focus().is_some_and(|f| f != gui::NONE) {
            bail!(
                "a panel or map is open ({}); close it first so the recipe starts from the cockpit",
                gui::name(self.journal.gui_focus().unwrap_or(0))
            );
        }
        let missing = self.config.missing_click_targets();
        if !missing.is_empty() {
            bail!(
                "click target(s) {missing:?} not recorded yet. Open the galaxy map on a system, \
                 hover the mouse over the button, and call record_click_target with that name."
            );
        }
        let before = std::fs::metadata(self.journal.dir.join("NavRoute.json"))
            .and_then(|m| m.modified())
            .ok();

        self.sender.focus_game()?;
        let mut log = Vec::new();
        for (i, step) in self.config.plot_route.steps.iter().enumerate() {
            if let Err(e) = self.run_step(step, system).await {
                bail!(
                    "step {} ({step:?}) failed: {e:#}. Steps done: {log:?}",
                    i + 1
                );
            }
            log.push(format!("{step:?}"));
        }

        if self.config.dry_run || cfg!(not(windows)) {
            return Ok(format!(
                "dry run: would plot to {system} ({} steps)",
                log.len()
            ));
        }

        let deadline =
            Instant::now() + Duration::from_millis(self.config.plot_route.verify_timeout_ms);
        loop {
            let updated = std::fs::metadata(self.journal.dir.join("NavRoute.json"))
                .and_then(|m| m.modified())
                .ok();
            if updated != before
                && let Some(dest) = self.journal.route_destination()
            {
                if dest.eq_ignore_ascii_case(system) {
                    let jumps = self
                        .journal
                        .nav_route()
                        .ok()
                        .and_then(|r| {
                            r.get("Route")?
                                .as_array()
                                .map(|a| a.len().saturating_sub(1))
                        })
                        .unwrap_or(0);
                    return Ok(format!("route plotted to {dest}: {jumps} jumps"));
                }
                bail!(
                    "a route was plotted, but to {dest} instead of {system}; check the search step"
                );
            }
            if Instant::now() > deadline {
                bail!(
                    "keys were sent but no new route appeared in NavRoute.json. The recipe probably \
                     needs tuning for your game version; use press_binding to step through it."
                );
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Recorder;

    #[tokio::test]
    async fn dry_run_plot_sends_expected_keys() {
        let mut config = Config::generate();
        config.dry_run = true;
        config.tap_ms = 1;
        config.click_targets.insert(
            crate::config::PLOT_ROUTE_BUTTON.into(),
            crate::config::ClickTarget { x: 0.8, y: 0.3 },
        );
        // Shrink waits so the test is fast.
        for s in &mut config.plot_route.steps {
            if let Step::Wait { ms } = s {
                *ms = 1;
            }
            if let Step::Binding {
                hold_ms: Some(h), ..
            } = s
            {
                *h = 1;
            }
        }
        let bindings = Bindings::parse(
            r#"<Root>
                <GalaxyMapOpen><Primary Device="Keyboard" Key="Key_M" /></GalaxyMapOpen>
                <UI_Up><Primary Device="Keyboard" Key="Key_W" /></UI_Up>
                <UI_Select><Primary Device="Keyboard" Key="Key_Space" /></UI_Select>
            </Root>"#,
        )
        .unwrap();
        // A journal dir whose Status.json says GuiFocus 0.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Status.json"), r#"{"GuiFocus":0}"#).unwrap();
        let journal = JournalDir::new(dir.path());
        let rec = Recorder::default();
        let pilot = Pilot {
            config: &config,
            bindings: &bindings,
            journal: &journal,
            sender: &rec,
        };
        let out = pilot.plot_route("Ross 248").await.unwrap();
        assert!(out.contains("dry run"), "{out}");
        let log = rec.entries();
        assert_eq!(log[0], "focus");
        assert_eq!(log[1], "down 0x32"); // M
        assert!(log.contains(&"clipboard Ross 248".to_string()));
        assert!(log.contains(&"down 0x1d".to_string())); // ctrl for paste
        assert!(log.contains(&"down 0x1c".to_string())); // enter
        let mv = log.iter().position(|l| l == "move 0.800,0.300").unwrap();
        assert_eq!(log[mv + 1], "click down");
        assert_eq!(log[mv + 2], "click up");
    }

    #[tokio::test]
    async fn refuses_without_recorded_click_target() {
        let config = Config::generate();
        let bindings = Bindings::default();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Status.json"), r#"{"GuiFocus":0}"#).unwrap();
        let journal = JournalDir::new(dir.path());
        let rec = Recorder::default();
        let pilot = Pilot {
            config: &config,
            bindings: &bindings,
            journal: &journal,
            sender: &rec,
        };
        let err = pilot.plot_route("Sol").await.unwrap_err().to_string();
        assert!(err.contains("record_click_target"), "{err}");
        assert!(rec.entries().is_empty());
    }

    #[tokio::test]
    async fn refuses_to_start_with_a_panel_open() {
        let config = Config::generate();
        let bindings = Bindings::default();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Status.json"), r#"{"GuiFocus":5}"#).unwrap();
        let journal = JournalDir::new(dir.path());
        let rec = Recorder::default();
        let pilot = Pilot {
            config: &config,
            bindings: &bindings,
            journal: &journal,
            sender: &rec,
        };
        let err = pilot.plot_route("Sol").await.unwrap_err().to_string();
        assert!(err.contains("StationServices"), "{err}");
        assert!(rec.entries().is_empty());
    }
}

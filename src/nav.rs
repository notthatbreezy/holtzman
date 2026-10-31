//! Lock an in-system target (station, carrier) from the left panel's
//! Navigation tab, as a short supervised exchange:
//!
//! 1. `start` checks Status.json, opens the panel and shows the list.
//! 2. `move_cursor` switches tabs or moves the highlight and shows the list
//!    again, until the caller reports the station highlighted.
//! 3. `confirm` presses the lock or supercruise assist button, checks
//!    Status.json, and closes the panel.
//!
//! The server can't read the list: the caller looks at the screenshots and
//! says what is highlighted. The server owns the order of the steps and
//! every check it can make itself.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use image::RgbaImage;
use rand::Rng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    automation::Pilot,
    config::NavTarget,
    journal::gui,
    screen::{Screen, changed_fraction},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NavAction {
    /// Lock the station as the navigation target.
    Lock,
    /// Clear the lock (the same button as `lock`).
    Unlock,
    /// Toggle supercruise assist to the station.
    SupercruiseAssist,
}

/// One start → move → confirm exchange. Expires if left idle.
#[derive(Debug, Clone)]
pub struct NavSession {
    pub id: String,
    pub station: String,
    pub action: NavAction,
    touched: Instant,
    /// The caller's last move reported the station as highlighted.
    highlighted: bool,
}

const SESSION_TTL: Duration = Duration::from_secs(180);
const MAX_ROWS: i32 = 60;
const MAX_TABS: i32 = 8;

/// Flags during which nobody should be opening panels.
const BUSY_FLAGS: &[&str] = &["IsInDanger", "BeingInterdicted", "FsdJump", "FsdCharging"];

impl NavSession {
    /// The live session matching `id`, or why there isn't one.
    pub fn resume(current: Option<&NavSession>, id: &str) -> Result<NavSession> {
        match current {
            Some(s) if s.id == id && s.touched.elapsed() < SESSION_TTL => Ok(s.clone()),
            Some(s) if s.id == id => bail!("nav session {id} expired; start again"),
            _ => bail!("no nav session {id}; call nav_target_start"),
        }
    }
}

/// What a step reports, plus the picture to show (if any).
#[derive(Debug)]
pub struct NavView {
    pub info: Value,
    pub frame: Option<RgbaImage>,
}

pub struct Nav<'a> {
    pub pilot: Pilot<'a>,
    pub screen: Arc<dyn Screen>,
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl Nav<'_> {
    fn cfg(&self) -> &NavTarget {
        &self.pilot.config.nav_target
    }

    fn dry_run(&self) -> bool {
        self.pilot.config.dry_run || cfg!(not(windows))
    }

    fn panel_open(&self) -> bool {
        self.dry_run() || self.pilot.journal.gui_focus() == Some(self.cfg().panel_gui_focus)
    }

    fn focus_name(&self) -> &'static str {
        gui::name(self.pilot.journal.gui_focus().unwrap_or(-1))
    }

    async fn capture(&self) -> Result<RgbaImage> {
        let screen = self.screen.clone();
        tokio::task::spawn_blocking(move || screen.capture()).await?
    }

    async fn settle(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    /// Check the game is in a state to open the panel, then open it and
    /// show the list.
    pub async fn start(&self, station: &str, action: NavAction) -> Result<(NavSession, NavView)> {
        let station = station.trim();
        if station.is_empty() {
            bail!("no station name given");
        }
        let journal = self.pilot.journal;
        if journal.status().is_err() {
            bail!("can't read Status.json; is the game running?");
        }
        if journal.gui_focus().is_some_and(|f| f != gui::NONE) {
            bail!(
                "a panel or map is open ({}); close it so this starts from the cockpit",
                self.focus_name()
            );
        }
        let flags = journal.flags();
        if let Some(busy) = flags.iter().find(|f| BUSY_FLAGS.contains(f)) {
            bail!("not now: the ship reports {busy}");
        }
        let current = journal.destination_name();
        let locked = current.as_deref().is_some_and(|d| same_name(d, station));
        match action {
            NavAction::Lock if locked => {
                bail!("{station} is already the locked target; nothing to do")
            }
            NavAction::Unlock if !locked => bail!(
                "{station} isn't the locked target (current: {}); nothing to unlock",
                current.as_deref().unwrap_or("none")
            ),
            _ => {}
        }

        self.pilot.sender.focus_game()?;
        self.pilot.run_steps(&self.cfg().open_steps).await?;
        if !self.panel_open() {
            bail!(
                "opened the left panel, but Status.json shows {}; check nav_target.open_steps",
                self.focus_name()
            );
        }
        let frame = self.capture().await?;
        let session = NavSession {
            id: format!("{:08x}", rand::rng().random::<u32>()),
            station: station.to_string(),
            action,
            touched: Instant::now(),
            highlighted: false,
        };
        let info = json!({
            "session": session.id,
            "station": station,
            "action": action,
            "current_destination": current,
            "station_is_locked": locked,
            "next": "Read the list in the image. If the Navigation tab isn't showing, call \
                     nav_target_move with `tabs`. Find the station and call nav_target_move with \
                     `rows` (positive = down) and `expect` = the name that will then be \
                     highlighted. If it is already highlighted, use rows 0. If it isn't in view, \
                     move in steps to scroll. Call nav_target_cancel to back out.",
        });
        Ok((
            session,
            NavView {
                info,
                frame: Some(frame),
            },
        ))
    }

    /// Switch tabs and/or move the highlight, then show the list.
    pub async fn move_cursor(
        &self,
        session: &mut NavSession,
        rows: i32,
        tabs: i32,
        expect: Option<&str>,
    ) -> Result<NavView> {
        if rows.abs() > MAX_ROWS || tabs.abs() > MAX_TABS {
            bail!("move at most {MAX_ROWS} rows and {MAX_TABS} tabs per call");
        }
        if !self.panel_open() {
            bail!(
                "the left panel is no longer open ({}); start again",
                self.focus_name()
            );
        }
        let cfg = self.cfg();
        let mut changed = None;
        if rows != 0 || tabs != 0 {
            self.pilot.sender.focus_game()?;
            let before = self.capture().await?;
            let tab = if tabs > 0 {
                &cfg.next_tab
            } else {
                &cfg.previous_tab
            };
            for index in 0..tabs.abs() {
                self.pilot.sender.progress(
                    index as usize + 1,
                    (tabs.abs() + rows.abs()) as usize,
                    "moving navigation tab",
                );
                self.pilot.press_binding(tab, None).await?;
                self.settle(cfg.step_gap_ms).await;
            }
            let row = if rows > 0 { &cfg.row_down } else { &cfg.row_up };
            for index in 0..rows.abs() {
                self.pilot.sender.progress(
                    (tabs.abs() + index) as usize + 1,
                    (tabs.abs() + rows.abs()) as usize,
                    "moving navigation row",
                );
                self.pilot.press_binding(row, None).await?;
                self.settle(cfg.step_gap_ms).await;
            }
            self.settle(cfg.settle_ms).await;
            let after = self.capture().await?;
            changed = Some((changed_fraction(&before, &after) * 1000.0).round() / 10.0);
            return Ok(self.after_move(session, rows, tabs, expect, changed, after));
        }
        let frame = self.capture().await?;
        Ok(self.after_move(session, rows, tabs, expect, changed, frame))
    }

    fn after_move(
        &self,
        session: &mut NavSession,
        rows: i32,
        tabs: i32,
        expect: Option<&str>,
        changed: Option<f64>,
        frame: RgbaImage,
    ) -> NavView {
        session.touched = Instant::now();
        session.highlighted = expect.is_some_and(|e| same_name(e, &session.station));
        let next = if session.highlighted {
            format!(
                "If the image really shows {} highlighted, call nav_target_confirm. If not, \
                 move again.",
                session.station
            )
        } else {
            format!(
                "Keep moving until {} is highlighted, then report it with `expect`.",
                session.station
            )
        };
        NavView {
            info: json!({
                "session": session.id,
                "rows": rows,
                "tabs": tabs,
                "expect": expect,
                "expect_is_target": session.highlighted,
                "screen_changed_pct": changed,
                "next": next,
            }),
            frame: Some(frame),
        }
    }

    /// Press the button for the session's action, verify, and close the panel.
    pub async fn confirm(&self, session: &NavSession) -> Result<NavView> {
        if !session.highlighted {
            bail!(
                "report {} highlighted first: nav_target_move with `expect`",
                session.station
            );
        }
        if !self.panel_open() {
            bail!(
                "the left panel is no longer open ({}); start again",
                self.focus_name()
            );
        }
        let cfg = self.cfg();
        let steps = match session.action {
            NavAction::Lock | NavAction::Unlock => &cfg.lock_steps,
            NavAction::SupercruiseAssist => &cfg.assist_steps,
        };
        self.pilot.sender.focus_game()?;
        self.pilot.run_steps(steps).await?;
        let verified = self.verify(session).await;
        self.settle(cfg.settle_ms).await;
        let closed = self.close().await;
        let destination = self.pilot.journal.destination_name();

        let verified = verified.map_err(|e| match &closed {
            Ok(()) => e.context("panel closed"),
            Err(c) => e.context(format!("closing the panel also failed: {c:#}")),
        })?;
        let mut info = json!({
            "station": session.station,
            "action": session.action,
            "result": verified,
            "destination": destination,
            "panel_closed": closed.is_ok(),
        });
        if let Err(e) = &closed {
            info["close_error"] = format!("{e:#}").into();
        }
        // Supercruise assist has no journal signal: show the HUD instead.
        let frame = match session.action {
            NavAction::SupercruiseAssist => {
                info["next"] = "Check the HUD in the image for the supercruise assist \
                                indicator before reporting success."
                    .into();
                Some(self.capture().await?)
            }
            _ => None,
        };
        Ok(NavView { info, frame })
    }

    /// Wait for Status.json to agree with the action.
    async fn verify(&self, session: &NavSession) -> Result<String> {
        if self.dry_run() {
            return Ok("dry run: not verified".into());
        }
        let want_locked = match session.action {
            NavAction::Lock => true,
            NavAction::Unlock => false,
            NavAction::SupercruiseAssist => return Ok("pressed; see screenshot".into()),
        };
        let deadline = Instant::now() + Duration::from_millis(self.cfg().verify_timeout_ms);
        loop {
            let dest = self.pilot.journal.destination_name();
            let locked = dest
                .as_deref()
                .is_some_and(|d| same_name(d, &session.station));
            if locked == want_locked {
                return Ok(if want_locked {
                    format!("locked {}", session.station)
                } else {
                    format!("unlocked {}", session.station)
                });
            }
            if Instant::now() > deadline {
                bail!(
                    "pressed the button, but Status.json shows destination {}; the wrong row or \
                     button may have been selected. Not retrying.",
                    dest.as_deref().unwrap_or("none")
                );
            }
            self.settle(150).await;
        }
    }

    /// Close the left panel and confirm the cockpit is back.
    pub async fn close(&self) -> Result<()> {
        self.pilot.sender.focus_game()?;
        self.pilot.run_steps(&self.cfg().close_steps).await?;
        if !self.dry_run() && self.pilot.journal.gui_focus() != Some(gui::NONE) {
            bail!("panel still open ({}) after closing", self.focus_name());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bindings::Bindings,
        config::{Config, Step},
        input::Recorder,
        journal::JournalDir,
    };

    struct Blank;
    impl Screen for Blank {
        fn capture(&self) -> Result<RgbaImage> {
            Ok(RgbaImage::new(64, 32))
        }
    }

    fn setup(status: &str) -> (Config, Bindings, tempfile::TempDir) {
        let mut config = Config::generate();
        config.dry_run = true;
        config.tap_ms = 1;
        let t = &mut config.nav_target;
        t.step_gap_ms = 1;
        t.settle_ms = 1;
        for steps in [
            &mut t.open_steps,
            &mut t.lock_steps,
            &mut t.assist_steps,
            &mut t.close_steps,
        ] {
            for s in steps.iter_mut() {
                if let Step::Wait { ms } = s {
                    *ms = 1;
                }
            }
        }
        let bindings = Bindings::parse(
            r#"<Root>
                <FocusLeftPanel><Primary Device="Keyboard" Key="Key_1" /></FocusLeftPanel>
                <CycleNextPanel><Primary Device="Keyboard" Key="Key_E" /></CycleNextPanel>
                <CyclePreviousPanel><Primary Device="Keyboard" Key="Key_Q" /></CyclePreviousPanel>
                <UI_Up><Primary Device="Keyboard" Key="Key_UpArrow" /></UI_Up>
                <UI_Down><Primary Device="Keyboard" Key="Key_DownArrow" /></UI_Down>
                <UI_Select><Primary Device="Keyboard" Key="Key_Space" /></UI_Select>
            </Root>"#,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Status.json"), status).unwrap();
        (config, bindings, dir)
    }

    fn nav<'a>(
        config: &'a Config,
        bindings: &'a Bindings,
        journal: &'a JournalDir,
        rec: &'a Recorder,
    ) -> Nav<'a> {
        Nav {
            pilot: Pilot {
                config,
                bindings,
                journal,
                sender: rec,
            },
            screen: Arc::new(Blank),
        }
    }

    const IN_SPACE: &str = r#"{"GuiFocus":0,"Flags":16777216,"Destination":{"System":1,"Body":3,"Name":"Other Port"}}"#;

    #[tokio::test]
    async fn start_move_confirm_locks_in_order() {
        let (config, bindings, dir) = setup(IN_SPACE);
        let journal = JournalDir::new(dir.path());
        let rec = Recorder::default();
        let nav = nav(&config, &bindings, &journal, &rec);

        let (mut session, view) = nav.start("Homer Platform", NavAction::Lock).await.unwrap();
        assert_eq!(view.info["current_destination"], "Other Port");
        assert!(view.frame.is_some());

        // Reporting the wrong row doesn't arm confirm.
        nav.move_cursor(&mut session, 2, 0, Some("Some Carrier"))
            .await
            .unwrap();
        assert!(nav.confirm(&session).await.is_err());

        let view = nav
            .move_cursor(&mut session, 1, 1, Some("homer platform"))
            .await
            .unwrap();
        assert_eq!(view.info["expect_is_target"], true);
        let done = nav.confirm(&session).await.unwrap();
        assert_eq!(done.info["result"], "dry run: not verified");
        assert_eq!(done.info["panel_closed"], true);

        let log = rec.entries();
        let downs = log.iter().filter(|l| *l == "down 0x50").count(); // Down arrow
        assert_eq!(downs, 3);
        assert_eq!(log.iter().filter(|l| *l == "down 0x12").count(), 1); // E
        // Open, lock (Select twice), close.
        assert_eq!(log.iter().filter(|l| *l == "down 0x02").count(), 2); // 1
        assert_eq!(log.iter().filter(|l| *l == "down 0x39").count(), 2); // Space
    }

    #[tokio::test]
    async fn refuses_to_lock_what_is_already_locked() {
        let (config, bindings, dir) = setup(IN_SPACE);
        let journal = JournalDir::new(dir.path());
        let rec = Recorder::default();
        let nav = nav(&config, &bindings, &journal, &rec);
        let err = nav
            .start("other port", NavAction::Lock)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("already"), "{err}");
        let err = nav
            .start("Homer Platform", NavAction::Unlock)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("nothing to unlock"), "{err}");
        assert!(rec.entries().is_empty(), "no keys pressed");
    }

    #[tokio::test]
    async fn refuses_with_a_panel_open_or_in_danger() {
        for (status, want) in [
            (r#"{"GuiFocus":6}"#, "GalaxyMap"),
            (r#"{"GuiFocus":0,"Flags":8388608}"#, "BeingInterdicted"),
        ] {
            let (config, bindings, dir) = setup(status);
            let journal = JournalDir::new(dir.path());
            let rec = Recorder::default();
            let nav = nav(&config, &bindings, &journal, &rec);
            let err = nav
                .start("Homer Platform", NavAction::Lock)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains(want), "{err}");
            assert!(rec.entries().is_empty());
        }
    }

    #[test]
    fn sessions_must_match_and_be_fresh() {
        let s = NavSession {
            id: "abc".into(),
            station: "X".into(),
            action: NavAction::Lock,
            touched: Instant::now(),
            highlighted: false,
        };
        assert!(NavSession::resume(Some(&s), "abc").is_ok());
        assert!(NavSession::resume(Some(&s), "nope").is_err());
        assert!(NavSession::resume(None, "abc").is_err());
    }
}

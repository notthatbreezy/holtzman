//! `holtzman.toml`: where to listen, who may connect, where the game files
//! are, and the keypress recipes for automated chores.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

use crate::screen::{Format, Region};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Address to bind, e.g. `0.0.0.0:7777` for the LAN.
    pub listen: String,
    /// Shared secret; clients send `Authorization: Bearer <token>`.
    pub token: String,
    /// Host names clients use to reach this machine (DNS-rebinding guard).
    /// Include the PC's name and LAN IP.
    pub allowed_hosts: Vec<String>,
    /// Journal folder. Defaults to `%USERPROFILE%\Saved Games\Frontier Developments\Elite Dangerous`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_dir: Option<PathBuf>,
    /// Bindings folder. Defaults to `%LOCALAPPDATA%\Frontier Developments\Elite Dangerous\Options\Bindings`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bindings_dir: Option<PathBuf>,
    /// Log keypresses instead of sending them.
    #[serde(default)]
    pub dry_run: bool,
    /// How long a tap holds a key down. Elite misses taps shorter than ~50 ms.
    #[serde(default = "default_tap_ms")]
    pub tap_ms: u64,
    /// Bindings the `press_binding` tool may trigger. Keep weapons and
    /// anything destructive off this list.
    #[serde(default = "default_pressable")]
    pub pressable: Vec<String>,
    /// Markdown file to serve as the `ship_computer` prompt instead of the
    /// built-in one (re-read on every request; relative to this file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ship_prompt: Option<PathBuf>,
    /// Folder of your own Markdown guides (`<name>.md`) to serve alongside
    /// the built-in ones; a file named like a built-in guide replaces it.
    /// Relative to this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guides_dir: Option<PathBuf>,
    pub plot_route: PlotRoute,
    /// Named screen positions for `click` steps, as fractions (0..1) of the
    /// game window, so they survive resolution changes. Record them with the
    /// `record_click_target` tool while hovering the mouse over the button.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub click_targets: BTreeMap<String, ClickTarget>,
    /// Defaults for the `screenshot` tool.
    #[serde(default)]
    pub screenshot: ScreenshotDefaults,
    /// Named parts of the game window for `screenshot`, as fractions (0..1),
    /// e.g. a panel on your screen layout. Adds to (or overrides) the built-in
    /// `full`, `center`, `left`, `right`, `top`, `bottom`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub screen_regions: BTreeMap<String, Region>,
    /// Keys and checks for the `nav_target_*` tools (lock an in-system
    /// station from the left panel's Navigation tab).
    #[serde(default)]
    pub nav_target: NavTarget,
}

/// How the `nav_target_*` tools drive the left panel. Every key sequence is
/// a recipe so it can be recalibrated without a rebuild.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct NavTarget {
    /// Opens the left panel. Must end with the panel open.
    pub open_steps: Vec<Step>,
    /// Status.json `GuiFocus` while the left panel is open (2 = ExternalPanel).
    pub panel_gui_focus: i64,
    /// Bindings that switch tabs and move the list highlight.
    pub next_tab: String,
    pub previous_tab: String,
    pub row_down: String,
    pub row_up: String,
    /// Pause between repeated presses, and before a screenshot.
    pub step_gap_ms: u64,
    pub settle_ms: u64,
    /// Where the Navigation list is on screen (window fractions), for the
    /// screenshots returned to the caller.
    pub list_region: Region,
    /// From the highlighted row: press the lock/unlock button.
    pub lock_steps: Vec<Step>,
    /// From the highlighted row: press the supercruise assist button.
    pub assist_steps: Vec<Step>,
    /// Closes the left panel. Should end back in the cockpit.
    pub close_steps: Vec<Step>,
    /// How long to wait for Status.json to show the new destination.
    pub verify_timeout_ms: u64,
}

impl Default for NavTarget {
    /// Uncalibrated guesses: the row's Select shows its buttons with the
    /// first (lock/unlock) highlighted, and supercruise assist is one below.
    fn default() -> Self {
        use Step::*;
        let b = |name: &str| Binding {
            name: name.into(),
            hold_ms: None,
        };
        Self {
            open_steps: vec![
                b("FocusLeftPanel"),
                WaitForGui {
                    focus: 2,
                    timeout_ms: 3000,
                },
                Wait { ms: 700 },
            ],
            panel_gui_focus: 2,
            next_tab: "CycleNextPanel".into(),
            previous_tab: "CyclePreviousPanel".into(),
            row_down: "UI_Down".into(),
            row_up: "UI_Up".into(),
            step_gap_ms: 150,
            settle_ms: 500,
            list_region: Region::builtin("left").expect("built-in region"),
            lock_steps: vec![b("UI_Select"), Wait { ms: 600 }, b("UI_Select")],
            assist_steps: vec![
                b("UI_Select"),
                Wait { ms: 600 },
                b("UI_Down"),
                Wait { ms: 250 },
                b("UI_Select"),
            ],
            close_steps: vec![
                b("FocusLeftPanel"),
                WaitForGui {
                    focus: 0,
                    timeout_ms: 3000,
                },
            ],
            verify_timeout_ms: 3000,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotDefaults {
    /// Longest side of the returned image in pixels. Vision models downscale
    /// past ~1568 anyway; smaller local models may want less.
    #[serde(default = "default_max_edge")]
    pub max_edge: u32,
    #[serde(default)]
    pub format: Format,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
}

impl Default for ScreenshotDefaults {
    fn default() -> Self {
        Self {
            max_edge: default_max_edge(),
            format: Format::default(),
            jpeg_quality: default_jpeg_quality(),
        }
    }
}

fn default_max_edge() -> u32 {
    1568
}

fn default_jpeg_quality() -> u8 {
    85
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClickTarget {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotRoute {
    /// How long to wait for NavRoute.json to show the new destination.
    pub verify_timeout_ms: u64,
    pub steps: Vec<Step>,
}

/// One step of a keypress recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Press whatever key the game has bound to an action, e.g. `GalaxyMapOpen`.
    Binding {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hold_ms: Option<u64>,
    },
    /// Press a raw key by Elite name, e.g. `Key_Enter`, with optional modifiers.
    Key {
        key: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hold_ms: Option<u64>,
    },
    /// Put the target system name on the clipboard and press Ctrl+V.
    PasteTarget,
    /// Sleep.
    Wait { ms: u64 },
    /// Poll Status.json until `GuiFocus` matches (6 = galaxy map), or fail.
    WaitForGui { focus: i64, timeout_ms: u64 },
    /// Left-click a named position from `[click_targets]`.
    Click {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hold_ms: Option<u64>,
    },
}

/// The click target the default plot-route recipe uses.
pub const PLOT_ROUTE_BUTTON: &str = "plot_route_button";

fn default_tap_ms() -> u64 {
    80
}

fn default_pressable() -> Vec<String> {
    [
        "GalaxyMapOpen",
        "SystemMapOpen",
        "UI_Up",
        "UI_Down",
        "UI_Left",
        "UI_Right",
        "UI_Select",
        "UI_Back",
        "CycleNextPanel",
        "CyclePreviousPanel",
        "FocusLeftPanel",
        "FocusRightPanel",
        "TargetNextRouteSystem",
        "SetSpeedZero",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

impl Default for PlotRoute {
    /// The Odyssey galaxy map recipe, as calibrated on a real game (4.4):
    /// UI_Up focuses the search box (UI keys must not share keys with map
    /// camera panning), Select opens it, paste + Enter flies to the system and
    /// opens its info panel, and the Plot Route button there only takes a
    /// mouse click.
    fn default() -> Self {
        use Step::*;
        let b = |name: &str| Binding {
            name: name.into(),
            hold_ms: None,
        };
        Self {
            verify_timeout_ms: 8000,
            steps: vec![
                b("GalaxyMapOpen"),
                WaitForGui {
                    focus: 6,
                    timeout_ms: 5000,
                },
                // Let the map finish its opening animation.
                Wait { ms: 1500 },
                // Focus the search box, open it, paste, search.
                b("UI_Up"),
                Wait { ms: 200 },
                b("UI_Select"),
                Wait { ms: 400 },
                PasteTarget,
                Wait { ms: 300 },
                Key {
                    key: "Key_Enter".into(),
                    modifiers: vec![],
                    hold_ms: None,
                },
                // The camera flies to the system and its info panel opens.
                Wait { ms: 3000 },
                Click {
                    target: PLOT_ROUTE_BUTTON.into(),
                    hold_ms: None,
                },
                Wait { ms: 1000 },
                b("GalaxyMapOpen"),
            ],
        }
    }
}

impl Config {
    pub fn generate() -> Self {
        let token: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let host = std::env::var("COMPUTERNAME")
            .map(|h| h.to_ascii_lowercase())
            .unwrap_or_else(|_| "skippy".into());
        Self {
            listen: "0.0.0.0:7777".into(),
            token,
            allowed_hosts: vec!["localhost".into(), "127.0.0.1".into(), host],
            journal_dir: None,
            bindings_dir: None,
            dry_run: false,
            tap_ms: default_tap_ms(),
            pressable: default_pressable(),
            ship_prompt: None,
            guides_dir: None,
            plot_route: PlotRoute::default(),
            click_targets: BTreeMap::new(),
            screenshot: ScreenshotDefaults::default(),
            screen_regions: BTreeMap::new(),
            nav_target: NavTarget::default(),
        }
    }

    /// Look up a screen region by name: `[screen_regions]` first, then built-ins.
    pub fn screen_region(&self, name: &str) -> Result<Region> {
        if let Some(r) = self.screen_regions.get(name) {
            return Ok(*r);
        }
        Region::builtin(name).ok_or_else(|| {
            let mut known: Vec<&str> = Region::BUILTIN_NAMES.to_vec();
            known.extend(self.screen_regions.keys().map(String::as_str));
            anyhow::anyhow!(
                "unknown screen region `{name}`; known: {}",
                known.join(", ")
            )
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {} (run `holtzman init` first)", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Write atomically-ish: temp file, then rename over the original.
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_toml()?)
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }

    /// Click targets referenced by the plot-route recipe that haven't been recorded.
    pub fn missing_click_targets(&self) -> Vec<String> {
        self.plot_route
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Click { target, .. } if !self.click_targets.contains_key(target) => {
                    Some(target.clone())
                }
                _ => None,
            })
            .collect()
    }

    pub fn journal_dir(&self) -> Option<PathBuf> {
        self.journal_dir
            .clone()
            .or_else(crate::journal::default_journal_dir)
    }

    pub fn bindings_dir(&self) -> Option<PathBuf> {
        self.bindings_dir.clone().or_else(|| {
            let local = std::env::var_os("LOCALAPPDATA")?;
            Some(
                PathBuf::from(local)
                    .join("Frontier Developments")
                    .join("Elite Dangerous")
                    .join("Options")
                    .join("Bindings"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_config_round_trips() {
        let c = Config::generate();
        assert_eq!(c.token.len(), 32);
        let text = c.to_toml().unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.plot_route.steps, c.plot_route.steps);
        assert!(text.contains("do = \"wait_for_gui\""));
        assert_eq!(
            c.missing_click_targets(),
            vec![PLOT_ROUTE_BUTTON.to_string()]
        );
    }

    #[test]
    fn old_configs_without_click_targets_still_load() {
        let mut c = Config::generate();
        c.plot_route
            .steps
            .retain(|s| !matches!(s, Step::Click { .. }));
        let text = c.to_toml().unwrap();
        assert!(!text.contains("click_targets"));
        let back: Config = toml::from_str(&text).unwrap();
        assert!(back.click_targets.is_empty());
        assert!(back.missing_click_targets().is_empty());
    }

    #[test]
    fn saves_and_reloads_click_targets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("holtzman.toml");
        let mut c = Config::generate();
        c.click_targets
            .insert(PLOT_ROUTE_BUTTON.into(), ClickTarget { x: 0.82, y: 0.31 });
        c.save(&path).unwrap();
        let back = Config::load(&path).unwrap();
        assert_eq!(back.click_targets, c.click_targets);
        assert!(back.missing_click_targets().is_empty());
    }

    #[test]
    fn screen_regions_override_builtins_and_old_configs_load() {
        let mut c = Config::generate();
        let text = c.to_toml().unwrap();
        // A config written before screenshots existed.
        let old = text.split("[screenshot]").next().unwrap().to_string();
        let back: Config = toml::from_str(&old).unwrap();
        assert_eq!(back.screenshot, ScreenshotDefaults::default());

        let panel = Region {
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
        };
        c.screen_regions.insert("left".into(), panel);
        assert_eq!(c.screen_region("left").unwrap(), panel);
        assert_eq!(c.screen_region("full").unwrap(), Region::FULL);
        assert!(c.screen_region("nope").is_err());
    }
}

//! `holtzman.toml`: where to listen, who may connect, where the game files
//! are, and the keypress recipes for automated chores.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

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
    pub plot_route: PlotRoute,
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
}

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
    /// A starting recipe for the Odyssey galaxy map. Menu layouts shift between
    /// game updates, so expect to tune this on your machine.
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
                // The camera flies to the system.
                Wait { ms: 2500 },
                // Hold select on "Plot Route".
                Binding {
                    name: "UI_Select".into(),
                    hold_ms: Some(1200),
                },
                Wait { ms: 800 },
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
            plot_route: PlotRoute::default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {} (run `holtzman init` first)", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
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
    }
}

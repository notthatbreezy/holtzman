//! Bounded ship-system input and cancellation shared by every transport.
use crate::{
    bindings::Bindings,
    config::Config,
    input::{KeySender, press},
    journal::{JournalDir, active_flags},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Override {
    pub enabled: Option<bool>,
    pub hold_ms: Option<u64>,
    pub allow_docked: Option<bool>,
    pub allow_landed: Option<bool>,
    pub supercruise: Option<bool>,
}

#[derive(Clone, Copy)]
pub struct Action {
    pub name: &'static str,
    pub binding: &'static str,
    pub flag: Option<&'static str>,
    pub inverted: bool,
}
pub const TABLE: &[Action] = &[
    Action {
        name: "hardpoints",
        binding: "DeployHardpointToggle",
        flag: Some("HardpointsDeployed"),
        inverted: false,
    },
    Action {
        name: "landing_gear",
        binding: "LandingGearToggle",
        flag: Some("LandingGearDown"),
        inverted: false,
    },
    Action {
        name: "cargo_scoop",
        binding: "ToggleCargoScoop",
        flag: Some("CargoScoopDeployed"),
        inverted: false,
    },
    Action {
        name: "lights",
        binding: "ShipSpotLightToggle",
        flag: Some("LightsOn"),
        inverted: false,
    },
    Action {
        name: "silent_running",
        binding: "ToggleButtonUpInput",
        flag: Some("SilentRunning"),
        inverted: false,
    },
    Action {
        name: "night_vision",
        binding: "NightVisionToggle",
        flag: Some("NightVision"),
        inverted: false,
    },
    Action {
        name: "flight_assist",
        binding: "ToggleFlightAssist",
        flag: Some("FlightAssistOff"),
        inverted: true,
    },
    Action {
        name: "heat_sink",
        binding: "DeployHeatSink",
        flag: None,
        inverted: false,
    },
    Action {
        name: "chaff",
        binding: "FireChaffLauncher",
        flag: None,
        inverted: false,
    },
    Action {
        name: "shield_cell",
        binding: "UseShieldCell",
        flag: None,
        inverted: false,
    },
];

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub action: String,
    pub state: Option<State>,
}
#[derive(Debug, Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum State {
    On,
    Off,
    Toggle,
}

impl Action {
    pub fn enabled(&self, config: &Config) -> bool {
        config
            .ship_actions
            .get(self.name)
            .and_then(|o| o.enabled)
            .unwrap_or(true)
    }
    pub fn state(&self, status: &Value) -> Option<bool> {
        self.flag
            .map(|flag| active_flags(status).contains(&flag) != self.inverted)
    }
    pub fn precondition(
        &self,
        config: &Config,
        bindings: &Bindings,
        status: &Value,
    ) -> Result<(), String> {
        if !self.enabled(config) {
            return Err("action disabled".into());
        }
        if self.flag.is_some() && bindings.hold_mode.contains(self.binding) {
            return Err(format!(
                "{} is in hold mode; configure a toggle binding",
                self.binding
            ));
        }
        if status.get("Flags").and_then(Value::as_u64).is_none()
            || status.get("GuiFocus").and_then(Value::as_i64).is_none()
        {
            return Err("cockpit status unavailable or incomplete".into());
        }
        let flags = active_flags(status);
        let has = |name| flags.contains(&name);
        if !has("InMainShip")
            || has("InSRV")
            || has("InFighter")
            || has("OnFoot")
            || has("InTaxi")
            || has("InMulticrew")
        {
            return Err("not piloting the main ship".into());
        }
        let settings = config
            .ship_actions
            .get(self.name)
            .cloned()
            .unwrap_or_default();
        let cosmetic = matches!(self.name, "lights" | "night_vision");
        if has("Docked") && !settings.allow_docked.unwrap_or(cosmetic) {
            return Err("ship is docked".into());
        }
        if has("Landed") && !settings.allow_landed.unwrap_or(cosmetic) {
            return Err("ship is landed".into());
        }
        if status["GuiFocus"].as_i64() != Some(0) {
            return Err("a panel or map is open".into());
        }
        if has("FsdCharging") || has("FsdJump") || has("FsdHyperdriveCharging") {
            return Err("FSD is charging or jumping".into());
        }
        if has("Supercruise") {
            let allowed = settings.supercruise.unwrap_or(match self.name {
                "landing_gear" | "cargo_scoop" => false,
                "hardpoints" => has("HudInAnalysisMode"),
                _ => true,
            });
            if !allowed {
                return Err("action unavailable in supercruise".into());
            }
        }
        Ok(())
    }
}

pub fn by_binding(binding: &str) -> Option<&'static Action> {
    TABLE.iter().find(|a| a.binding == binding)
}
pub fn coverage(config: &Config, bindings: &Bindings) -> Vec<Value> {
    TABLE.iter().filter(|a|a.enabled(config)).map(|a|json!({"action":a.name,"binding":a.binding,
        "status":if bindings.get(a.binding).is_err(){"MISSING"} else if bindings.hold_mode.contains(a.binding) && a.flag.is_some(){"HOLD_MODE"}else{"bound"}})).collect()
}

pub async fn execute(
    args: &Args,
    config: &Config,
    bindings: &Bindings,
    journal: &JournalDir,
    sender: &dyn KeySender,
) -> Value {
    let started = Instant::now();
    let mut result = json!({"action":args.action,"outcome":"refused","pressed":false,"before":null,"after":null,"elapsed_ms":0});
    let Some(action) = TABLE.iter().find(|a| a.name == args.action) else {
        result["reason"] = json!("unknown ship action");
        return result;
    };
    let operation = async {
        if !action.enabled(config) {
            return Err("action disabled".into());
        }
        if (action.flag.is_some() && args.state.is_none())
            || (action.flag.is_none() && args.state.is_some())
        {
            return Err("toggles require on, off or toggle; one-shots take no state".into());
        }
        let chord = match bindings.get(action.binding) {
            Ok(c) => c,
            Err(e) => {
                result["outcome"] = json!("unbound");
                return Err(e.to_string());
            }
        };
        let status = journal
            .status()
            .map_err(|_| "cockpit status unavailable".to_string())?;
        action.precondition(config, bindings, &status)?;
        let before = action.state(&status);
        result["before"] = json!(before);
        let expected = before.map(|s| match args.state {
            Some(State::On) => true,
            Some(State::Off) => false,
            _ => !s,
        });
        if before.is_some() && before == expected {
            result["outcome"] = json!("already");
            result["after"] = json!(before);
            return Ok(());
        }
        if sender.modifiers_down() {
            return Err("release Shift, Ctrl and Alt first".into());
        }
        let (key, mods) = chord.scan_codes().map_err(|e| e.to_string())?;
        sender.focus_game().map_err(|e| e.to_string())?;
        // Focusing can take time; check again immediately before the key goes down.
        let fresh = journal.status().map_err(|_| "cockpit status unavailable")?;
        action.precondition(config, bindings, &fresh)?;
        let before = action.state(&fresh);
        result["before"] = json!(before);
        let expected = before.map(|s| match args.state {
            Some(State::On) => true,
            Some(State::Off) => false,
            _ => !s,
        });
        if before.is_some() && before == expected {
            result["outcome"] = json!("already");
            result["after"] = json!(before);
            return Ok(());
        }
        if sender.modifiers_down() {
            return Err("release Shift, Ctrl and Alt first".into());
        }
        let hold = config
            .ship_actions
            .get(action.name)
            .and_then(|o| o.hold_ms)
            .unwrap_or(config.tap_ms)
            .clamp(1, 1000);
        press(sender, key, &mods, Duration::from_millis(hold))
            .await
            .map_err(|e| e.to_string())?;
        result["pressed"] = json!(true);
        result["outcome"] = json!(if action.flag.is_none() {
            "pressed_unverified"
        } else {
            "not_confirmed"
        });
        if let Some(expected) = expected {
            let deadline = Instant::now() + Duration::from_millis(1500);
            while Instant::now() < deadline {
                if let Ok(s) = journal.status() {
                    let after = action.state(&s);
                    result["after"] = json!(after);
                    if active_flags(&s).contains(&"InMainShip")
                        && s.get("Flags").and_then(Value::as_u64).is_some()
                        && after == Some(expected)
                    {
                        result["outcome"] = json!("done");
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        Ok::<(), String>(())
    }
    .await;
    if let Err(reason) = operation {
        result["reason"] = json!(reason);
    }
    result["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
    result
}

#[derive(Default)]
pub struct Registry {
    inner: Mutex<Option<Record>>,
    history: Mutex<std::collections::VecDeque<Value>>,
}
struct Record {
    value: Value,
    token: CancellationToken,
}
pub struct Running {
    registry: Arc<Registry>,
    pub token: CancellationToken,
    finished: bool,
}
impl Registry {
    pub fn start(self: &Arc<Self>, tool: &str) -> Running {
        let token = CancellationToken::new();
        let mut current = self.inner.lock().unwrap();
        if let Some(previous) = current.take() {
            let mut history = self.history.lock().unwrap();
            history.push_back(previous.value);
            if history.len() > 100 {
                history.pop_front();
            }
        }
        *current = Some(Record {
            value: json!({"id":rand::random::<u64>().to_string(),"tool":tool,"started":chrono::Utc::now().to_rfc3339(),"state":"running","step":0}),
            token: token.clone(),
        });
        Running {
            registry: self.clone(),
            token,
            finished: false,
        }
    }
    pub fn status(&self) -> Value {
        self.inner
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.value.clone())
            .unwrap_or(json!({"state":"idle"}))
    }
    pub fn cancel(&self) -> Option<String> {
        let r = self.inner.lock().unwrap();
        let r = r.as_ref().filter(|r| r.value["state"] == "running")?;
        r.token.cancel();
        r.value["id"].as_str().map(String::from)
    }
    pub fn record(&self, id: &str) -> Value {
        let current = self.inner.lock().unwrap();
        if let Some(record) = current.as_ref().filter(|r| r.value["id"] == id) {
            return record.value.clone();
        }
        self.history
            .lock()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .cloned()
            .unwrap_or(json!({"id":id,"state":"unknown"}))
    }
    pub fn step(&self, step: usize, total: usize, description: &str) {
        if let Some(r) = self.inner.lock().unwrap().as_mut() {
            r.value["step"] = json!(step);
            r.value["total_steps"] = json!(total);
            r.value["description"] = json!(description);
        }
    }
}
impl Running {
    pub fn finish(&mut self, state: &str) {
        if let Some(r) = self.registry.inner.lock().unwrap().as_mut() {
            r.value["state"] = json!(state);
        }
        self.finished = true;
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(if self.token.is_cancelled() {
                "cancelled"
            } else {
                "failed"
            });
        }
    }
}

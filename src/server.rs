//! The MCP server: tool definitions plus the HTTP plumbing (bearer auth,
//! host allow-list).

use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, GetPromptRequestParams, GetPromptResponse, GetPromptResult,
        ListPromptsResult, ListResourcesResult, PaginatedRequestParams, Prompt, PromptMessage,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, Role, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{
    automation::Pilot,
    bindings::Bindings,
    config::{ClickTarget, Config},
    guides,
    input::{KeySender, platform_sender},
    journal::JournalDir,
    nav::{Nav, NavAction, NavSession, NavView},
    screen::{Format, Region, Screen, platform_screen},
    spansh::{CommodityArgs, MarketSearch, RouteArgs, Spansh},
};
use image::RgbaImage;

/// Shared state behind every MCP session.
pub struct Ship {
    /// Live config; tools like `record_click_target` update it and persist
    /// it to `config_path`.
    pub config: RwLock<Config>,
    pub config_path: Option<PathBuf>,
    pub journal: JournalDir,
    pub sender: Arc<dyn KeySender>,
    /// Only one keypress recipe may run at a time.
    pub controls: Mutex<()>,
    /// Captures the game window for the `screenshot` tool.
    pub screen: Arc<dyn Screen>,
    pub spansh: Spansh,
    /// The open `nav_target_*` exchange, if any.
    pub nav: std::sync::Mutex<Option<NavSession>>,
}

impl Ship {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Result<Self> {
        let dir = config
            .journal_dir()
            .ok_or_else(|| anyhow::anyhow!("set journal_dir in holtzman.toml"))?;
        let sender = platform_sender(config.dry_run);
        Ok(Self {
            journal: JournalDir::new(dir),
            sender,
            controls: Mutex::new(()),
            screen: platform_screen(),
            spansh: Spansh::new()?,
            nav: std::sync::Mutex::new(None),
            config: RwLock::new(config),
            config_path,
        })
    }

    /// A copy of the current config (cheap; keeps locks off await points).
    pub fn config(&self) -> Config {
        self.config.read().expect("config lock").clone()
    }

    /// Bindings are re-read on every use so control changes apply without a restart.
    fn bindings(&self) -> Result<Bindings, String> {
        let dir = self
            .config()
            .bindings_dir()
            .ok_or("set bindings_dir in holtzman.toml")?;
        Bindings::discover(&dir).map_err(|e| format!("{e:#}"))
    }
}

#[derive(Clone)]
pub struct Holtzman {
    ship: Arc<Ship>,
    #[allow(dead_code, reason = "read by the #[tool_handler] macro")]
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketArgs {
    #[schemars(description = "Only commodities whose name contains this text (case-insensitive)")]
    pub commodity: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EventsArgs {
    #[schemars(description = "How many events to return (default 25, max 200)")]
    pub limit: Option<usize>,
    #[schemars(description = "Only these event types, e.g. [\"MarketSell\", \"FSDJump\"]")]
    pub only: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlotArgs {
    #[schemars(description = "Exact star system name, e.g. \"Ross 248\"")]
    pub system: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordArgs {
    #[schemars(description = "Target name, e.g. \"plot_route_button\"")]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PressArgs {
    #[schemars(description = "Elite binding name, e.g. \"UI_Select\" or \"GalaxyMapOpen\"")]
    pub binding: String,
    #[schemars(description = "Hold duration in ms (default: tap)")]
    pub hold_ms: Option<u64>,
    #[schemars(
        description = "Return a screenshot taken after the press, plus screen_changed_pct (how much of the window changed; ~0 means the key likely did nothing)"
    )]
    pub observe: Option<bool>,
    #[schemars(
        description = "With observe: ms to wait for the UI to settle before capturing (default 500, max 5000)"
    )]
    pub settle_ms: Option<u64>,
    #[schemars(
        description = "With observe: screen region to return, as for the screenshot tool (default full)"
    )]
    pub region: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GuideArgs {
    #[schemars(
        description = "Guide to read, e.g. \"fuel\" or \"exploration\". Omit to list them all."
    )]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NavStartArgs {
    #[schemars(description = "Station or carrier name as the Navigation list shows it")]
    pub station: String,
    #[schemars(
        description = "lock, unlock, or supercruise_assist (toggles assist to the station)"
    )]
    pub action: NavAction,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NavMoveArgs {
    #[schemars(description = "Session id from nav_target_start")]
    pub session: String,
    #[schemars(description = "Rows to move the highlight: positive = down, negative = up")]
    pub rows: Option<i32>,
    #[schemars(description = "Tabs to switch first: positive = next, negative = previous")]
    pub tabs: Option<i32>,
    #[schemars(
        description = "Name you expect to be highlighted after the move. Must equal the station before nav_target_confirm will run."
    )]
    pub expect: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NavSessionArgs {
    #[schemars(description = "Session id from nav_target_start")]
    pub session: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenshotArgs {
    #[schemars(
        description = "Named part of the game window: full (default), center, left, right, top, bottom, or a name from [screen_regions] in holtzman.toml"
    )]
    pub region: Option<String>,
    #[schemars(
        description = "Custom crop as fractions of the window; overrides `region`. Crop to read small HUD text."
    )]
    pub rect: Option<Region>,
    #[schemars(description = "Longest side of the image in pixels (default from config, 1568)")]
    pub max_edge: Option<u32>,
    #[schemars(description = "jpeg (default, smaller) or png (sharper text)")]
    pub format: Option<Format>,
}

fn pretty(v: serde_json::Value) -> String {
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tool_router]
impl Holtzman {
    pub fn new(ship: Arc<Ship>) -> Self {
        Self {
            ship,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Commander snapshot: location (system/station), ship and cargo capacity, credits, fuel, active status flags, open GUI panel, and plotted route destination. Read from the live game journal."
    )]
    async fn status(&self) -> Result<String, String> {
        self.ship.journal.summary().map(pretty).map_err(err)
    }

    #[tool(
        description = "Commodity market of the station where the commander last opened the market screen (from Market.json). Includes buy/sell price, stock, demand. Check `station` and `timestamp` to be sure it's the current station."
    )]
    async fn market(&self, Parameters(args): Parameters<MarketArgs>) -> Result<String, String> {
        self.ship
            .journal
            .market_summary(args.commodity.as_deref())
            .map(pretty)
            .map_err(err)
    }

    #[tool(description = "Current ship cargo hold contents (from Cargo.json).")]
    async fn cargo(&self) -> Result<String, String> {
        self.ship.journal.cargo().map(pretty).map_err(err)
    }

    #[tool(description = "The currently plotted route, system by system (from NavRoute.json).")]
    async fn nav_route(&self) -> Result<String, String> {
        self.ship.journal.nav_route().map(pretty).map_err(err)
    }

    #[tool(
        description = "Recent journal events from the current session, newest last. Noise (music, chat, signal discoveries) is skipped unless requested via `only`."
    )]
    async fn recent_events(
        &self,
        Parameters(args): Parameters<EventsArgs>,
    ) -> Result<String, String> {
        let limit = args.limit.unwrap_or(25).min(200);
        let only = args.only.unwrap_or_default();
        self.ship
            .journal
            .recent_events(limit, &only)
            .map(|v| pretty(serde_json::Value::Array(v)))
            .map_err(err)
    }

    #[tool(
        description = "Look up canonical Spansh commodity names by case-insensitive substring. Read-only external request; no game input or commander identity is sent. Use before find_commodity_market when a name is uncertain.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn trade_commodities(
        &self,
        Parameters(args): Parameters<CommodityArgs>,
    ) -> Result<String, String> {
        self.ship
            .spansh
            .commodities(args)
            .await
            .map(pretty)
            .map_err(err)
    }

    #[tool(
        description = "Find price-ranked nearby stations where the commander can BUY or SELL a named commodity. Sends only explicit search arguments to Spansh. Requires a reference system; defaults to large pads, no carriers/planetary stations, and markets at most 24 hours old. Returns prices, supply/demand, market ages and distances. Read-only advice; never plots, trades or sends game input.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn find_commodity_market(
        &self,
        Parameters(args): Parameters<MarketSearch>,
    ) -> Result<String, String> {
        self.ship
            .spansh
            .markets(args)
            .await
            .map(pretty)
            .map_err(err)
    }

    #[tool(
        description = "Plan a short profitable commodity-trading itinerary from an explicit system AND station, with explicit empty cargo space and purchasing budget. Sends those parameters to Spansh; may take up to 45 seconds. Returns trade legs, prices, quantities, estimated profits and market ages. Leg distance is not ship jump range. Read-only advice; never moves the ship, buys/sells or plots automatically. Ask for missing budget/constraints rather than assuming all credits are spendable.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn plan_trade_route(
        &self,
        Parameters(args): Parameters<RouteArgs>,
    ) -> Result<String, String> {
        self.ship.spansh.route(args).await.map(pretty).map_err(err)
    }

    #[tool(
        description = "Plot a route to a star system: opens the galaxy map, searches for the system, plots, closes the map, then confirms via NavRoute.json. Takes control of the keyboard for ~10 s, so tell the commander first. The game must be focused with no panel open."
    )]
    async fn plot_route(&self, Parameters(args): Parameters<PlotArgs>) -> Result<String, String> {
        let _guard = self
            .ship
            .controls
            .try_lock()
            .map_err(|_| "another keypress sequence is already running".to_string())?;
        let bindings = self.ship.bindings()?;
        let config = self.ship.config();
        let pilot = Pilot {
            config: &config,
            bindings: &bindings,
            journal: &self.ship.journal,
            sender: self.ship.sender.as_ref(),
        };
        pilot.plot_route(&args.system).await.map_err(err)
    }

    #[tool(
        description = "Press one allow-listed game binding (see `pressable` in holtzman.toml). Useful for stepping through menus and tuning recipes. Set `observe` to get a screenshot of the result and how much of the screen changed, in the same call."
    )]
    async fn press_binding(
        &self,
        Parameters(args): Parameters<PressArgs>,
    ) -> Result<CallToolResult, String> {
        if !self
            .ship
            .config()
            .pressable
            .iter()
            .any(|p| p == &args.binding)
        {
            return Err(format!(
                "`{}` is not in the pressable allow-list in holtzman.toml",
                args.binding
            ));
        }
        let _guard = self
            .ship
            .controls
            .try_lock()
            .map_err(|_| "another keypress sequence is already running".to_string())?;
        let bindings = self.ship.bindings()?;
        // Resolve the view and grab the "before" frame first, so a bad region
        // or failed capture stops us before any key is pressed.
        let observe = match args.observe.unwrap_or(false) {
            true => {
                let view = self.view(args.region.as_deref(), None)?;
                Some((view, self.grab().await?))
            }
            false => None,
        };
        self.ship.sender.focus_game().map_err(err)?;
        let config = self.ship.config();
        let pilot = Pilot {
            config: &config,
            bindings: &bindings,
            journal: &self.ship.journal,
            sender: self.ship.sender.as_ref(),
        };
        let pressed = pilot
            .press_binding(&args.binding, args.hold_ms)
            .await
            .map_err(err)?;
        let Some(((name, region), before)) = observe else {
            return Ok(CallToolResult::success(vec![ContentBlock::text(pressed)]));
        };
        let settle = args.settle_ms.unwrap_or(500).min(5000);
        tokio::time::sleep(std::time::Duration::from_millis(settle)).await;
        let after = self.grab().await?;
        let changed = crate::screen::changed_fraction(&before, &after);
        self.picture(
            &after,
            &name,
            region,
            None,
            None,
            serde_json::json!({
                "pressed": pressed,
                "settle_ms": settle,
                "screen_changed_pct": (changed * 1000.0).round() / 10.0,
            }),
        )
    }

    #[tool(
        description = "Look at the game: a picture of the Elite Dangerous window (never the rest of the desktop) plus the open panel and status flags from Status.json. Use it to orient before UI actions, e.g. to see which tab or row is highlighted. Crop with `region`/`rect` to read small text; rect coordinates are game-window fractions, the same space as click targets and cursor_position. Read-only; sends no input. Images cost context: prefer press_binding's `observe` while stepping through menus.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn screenshot(
        &self,
        Parameters(args): Parameters<ScreenshotArgs>,
    ) -> Result<CallToolResult, String> {
        let (name, region) = self.view(args.region.as_deref(), args.rect)?;
        let frame = self.grab().await?;
        self.picture(
            &frame,
            &name,
            region,
            args.max_edge,
            args.format,
            serde_json::json!({}),
        )
    }

    #[tool(
        description = "Lock (or unlock, or toggle supercruise assist to) a station or carrier in the current system, step 1 of 3. Checks Status.json (refuses if a panel is open, in danger, or the lock is already as asked), opens the left panel, and returns a screenshot of the Navigation list plus a session id. Presses keys: tell the commander first. Then use nav_target_move until the station is highlighted, then nav_target_confirm."
    )]
    async fn nav_target_start(
        &self,
        Parameters(args): Parameters<NavStartArgs>,
    ) -> Result<CallToolResult, String> {
        let _guard = self.controls()?;
        let bindings = self.ship.bindings()?;
        let config = self.ship.config();
        let nav = self.nav(&config, &bindings);
        let (session, view) = nav.start(&args.station, args.action).await.map_err(err)?;
        *self.ship.nav.lock().expect("nav lock") = Some(session);
        self.nav_view(view, config.nav_target.list_region)
    }

    #[tool(
        description = "Step 2 of 3: switch tabs (`tabs`) and/or move the Navigation list highlight (`rows`), then return a fresh screenshot of the list. Say in `expect` which name will be highlighted afterwards; when that is the target station, nav_target_confirm is allowed. Use rows 0 to just report what is highlighted now."
    )]
    async fn nav_target_move(
        &self,
        Parameters(args): Parameters<NavMoveArgs>,
    ) -> Result<CallToolResult, String> {
        let _guard = self.controls()?;
        let mut session = NavSession::resume(
            self.ship.nav.lock().expect("nav lock").as_ref(),
            &args.session,
        )
        .map_err(err)?;
        let bindings = self.ship.bindings()?;
        let config = self.ship.config();
        let nav = self.nav(&config, &bindings);
        let view = nav
            .move_cursor(
                &mut session,
                args.rows.unwrap_or(0),
                args.tabs.unwrap_or(0),
                args.expect.as_deref(),
            )
            .await
            .map_err(err)?;
        *self.ship.nav.lock().expect("nav lock") = Some(session);
        self.nav_view(view, config.nav_target.list_region)
    }

    #[tool(
        description = "Step 3 of 3: with the target station highlighted, press the lock/unlock or supercruise assist button, verify the lock in Status.json, and close the panel. For supercruise assist, returns a HUD screenshot to check. Ends the session either way; on failure it does not retry."
    )]
    async fn nav_target_confirm(
        &self,
        Parameters(args): Parameters<NavSessionArgs>,
    ) -> Result<CallToolResult, String> {
        let _guard = self.controls()?;
        let id = args.session.ok_or("session is required")?;
        let session = NavSession::resume(self.ship.nav.lock().expect("nav lock").as_ref(), &id)
            .map_err(err)?;
        let bindings = self.ship.bindings()?;
        let config = self.ship.config();
        let nav = self.nav(&config, &bindings);
        let result = nav.confirm(&session).await;
        // One confirm per session, whatever happened.
        self.ship.nav.lock().expect("nav lock").take();
        self.nav_view(result.map_err(err)?, Region::FULL)
    }

    #[tool(
        description = "Back out of a nav_target exchange: close the left panel (if open) and end the session. Use when something looks wrong."
    )]
    async fn nav_target_cancel(
        &self,
        Parameters(_args): Parameters<NavSessionArgs>,
    ) -> Result<String, String> {
        let _guard = self.controls()?;
        self.ship.nav.lock().expect("nav lock").take();
        let bindings = self.ship.bindings()?;
        let config = self.ship.config();
        let nav = self.nav(&config, &bindings);
        if self.ship.journal.gui_focus() != Some(config.nav_target.panel_gui_focus) {
            return Ok("session ended; the left panel wasn't open".into());
        }
        nav.close().await.map_err(err)?;
        Ok("session ended and the left panel closed".into())
    }

    #[tool(
        description = "Field guides to Elite Dangerous: game concepts (fuel, heat, jump range, interdiction, rebuy, engineering) and mission profiles (trading, exploration, exploration for profit, mining). With no name, lists them. Read the matching profile when the commander starts an activity, and check a concept guide before answering mechanics questions from memory.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn read_guide(&self, Parameters(args): Parameters<GuideArgs>) -> Result<String, String> {
        let dir = self.guides_dir();
        match args
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            Some(name) => guides::read(dir.as_deref(), name).map_err(err),
            None => Ok(pretty(
                serde_json::to_value(guides::list(dir.as_deref())).unwrap_or_default(),
            )),
        }
    }

    #[tool(
        description = "Where the mouse cursor is on the gaming PC, as fractions (0..1) of the game window plus pixels. Use while the commander hovers over a button."
    )]
    async fn cursor_position(&self) -> Result<String, String> {
        let pos = self.ship.sender.cursor().map_err(err)?;
        Ok(serde_json::to_string_pretty(&pos).unwrap_or_default())
    }

    #[tool(
        description = "Save the current mouse position (relative to the game window) as a named click target and persist it to holtzman.toml. Ask the commander to hover over the button first. The plot_route recipe uses `plot_route_button`."
    )]
    async fn record_click_target(
        &self,
        Parameters(args): Parameters<RecordArgs>,
    ) -> Result<String, String> {
        let name = args.name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err("name must be lowercase letters, digits and underscores".into());
        }
        let pos = self.ship.sender.cursor().map_err(err)?;
        if !(0.0..=1.0).contains(&pos.x) || !(0.0..=1.0).contains(&pos.y) {
            return Err(format!(
                "the cursor is outside the game window ({:.3}, {:.3}); hover over the button in-game",
                pos.x, pos.y
            ));
        }
        let snapshot = {
            let mut config = self.ship.config.write().expect("config lock");
            config
                .click_targets
                .insert(name.to_string(), ClickTarget { x: pos.x, y: pos.y });
            config.clone()
        };
        let saved = match &self.ship.config_path {
            Some(path) => {
                snapshot.save(path).map_err(err)?;
                format!("saved to {}", path.display())
            }
            None => "not persisted (no config file)".into(),
        };
        Ok(format!(
            "recorded `{name}` at ({:.3}, {:.3}) of a {}x{} window; {saved}",
            pos.x, pos.y, pos.window_width, pos.window_height
        ))
    }
}

/// Screenshot plumbing shared by `screenshot` and `press_binding`.
impl Holtzman {
    /// A named region (default `full`) or a custom rect.
    fn view(&self, region: Option<&str>, rect: Option<Region>) -> Result<(String, Region), String> {
        if let Some(rect) = rect {
            rect.validate().map_err(err)?;
            return Ok(("custom".into(), rect));
        }
        let name = region.unwrap_or("full");
        let region = self.ship.config().screen_region(name).map_err(err)?;
        Ok((name.into(), region))
    }

    /// Capture the game window off the async runtime (GDI calls block).
    async fn grab(&self) -> Result<RgbaImage, String> {
        let screen = self.ship.screen.clone();
        tokio::task::spawn_blocking(move || screen.capture())
            .await
            .map_err(|e| e.to_string())?
            .map_err(err)
    }

    /// The keyboard, if no other keypress sequence holds it.
    fn controls(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        self.ship
            .controls
            .try_lock()
            .map_err(|_| "another keypress sequence is already running".to_string())
    }

    fn nav<'a>(&'a self, config: &'a Config, bindings: &'a Bindings) -> Nav<'a> {
        Nav {
            pilot: Pilot {
                config,
                bindings,
                journal: &self.ship.journal,
                sender: self.ship.sender.as_ref(),
            },
            screen: self.ship.screen.clone(),
        }
    }

    /// A nav step's report, with its screenshot cropped to `region`.
    fn nav_view(&self, view: NavView, region: Region) -> Result<CallToolResult, String> {
        match view.frame {
            Some(frame) => self.picture(&frame, "nav_target", region, None, None, view.info),
            None => Ok(CallToolResult::success(vec![ContentBlock::text(pretty(
                view.info,
            ))])),
        }
    }

    /// Encode a frame as an image block, preceded by a text block describing
    /// it: `extra` fields, the crop, and what Status.json says.
    fn picture(
        &self,
        frame: &RgbaImage,
        name: &str,
        region: Region,
        max_edge: Option<u32>,
        format: Option<Format>,
        mut extra: serde_json::Value,
    ) -> Result<CallToolResult, String> {
        let defaults = self.ship.config().screenshot;
        let max_edge = max_edge.unwrap_or(defaults.max_edge).clamp(64, 4096);
        let format = format.unwrap_or(defaults.format);
        let shot = crate::screen::render(frame, region, max_edge, format, defaults.jpeg_quality)
            .map_err(err)?;

        let info = extra.as_object_mut().expect("extra is a JSON object");
        info.insert(
            "captured_at".into(),
            chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                .into(),
        );
        info.insert(
            "region".into(),
            serde_json::json!({ "name": name, "x": region.x, "y": region.y, "w": region.w, "h": region.h }),
        );
        info.insert(
            "window".into(),
            serde_json::json!([shot.window_width, shot.window_height]),
        );
        info.insert("image".into(), serde_json::json!([shot.width, shot.height]));
        if let Ok(ui) = self.ship.journal.ui_state() {
            info.insert("journal".into(), ui);
        }
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(&shot.bytes);
        Ok(CallToolResult::success(vec![
            ContentBlock::text(pretty(extra)),
            ContentBlock::image(data, format.mime()),
        ]))
    }
}

/// The ship computer's personality and rules of engagement, served as the
/// `ship_computer` prompt. `ship_prompt` in holtzman.toml can replace it.
pub const SHIP_COMPUTER_PROMPT: &str = include_str!("../prompts/ship-computer.md");
const SHIP_COMPUTER: &str = "ship_computer";

impl Holtzman {
    /// Paths in holtzman.toml are relative to the file itself.
    fn config_relative(&self, path: PathBuf) -> PathBuf {
        match self.ship.config_path.as_deref().and_then(|c| c.parent()) {
            Some(dir) if path.is_relative() => dir.join(path),
            _ => path,
        }
    }

    /// The prompt text: the configured override file if set, else the built-in one.
    fn ship_prompt(&self) -> Result<String, ErrorData> {
        let Some(path) = self.ship.config().ship_prompt else {
            return Ok(SHIP_COMPUTER_PROMPT.to_string());
        };
        let path = self.config_relative(path);
        std::fs::read_to_string(&path).map_err(|e| {
            ErrorData::internal_error(format!("reading ship_prompt {}: {e}", path.display()), None)
        })
    }

    fn guides_dir(&self) -> Option<PathBuf> {
        self.ship
            .config()
            .guides_dir
            .map(|d| self.config_relative(d))
    }
}

#[tool_handler]
impl ServerHandler for Holtzman {
    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let mut prompt = Prompt::new(
            SHIP_COMPUTER,
            Some(
                "The ship's droid: personality, chain of command, and rules of engagement \
                 for the controls. Use it as the system prompt.",
            ),
            None,
        );
        prompt.title = Some("Ship computer".into());
        Ok(ListPromptsResult::with_all_items(vec![prompt]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        if request.name != SHIP_COMPUTER {
            return Err(ErrorData::invalid_params(
                format!("unknown prompt `{}`", request.name),
                None,
            ));
        }
        // MCP prompts have no system role; harnesses use this as their system prompt.
        let text = self.ship_prompt()?;
        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
                .with_description("Ship computer system prompt")
                .into(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = guides::list(self.guides_dir().as_deref())
            .into_iter()
            .map(|g| {
                Resource::new(format!("{}{}", guides::URI_PREFIX, g.name), g.name)
                    .with_description(format!("{} guide: {}", g.kind, g.summary))
                    .with_mime_type("text/markdown")
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let name = request
            .uri
            .strip_prefix(guides::URI_PREFIX)
            .ok_or_else(|| ErrorData::resource_not_found(request.uri.clone(), None))?;
        let text = guides::read(self.guides_dir().as_deref(), name)
            .map_err(|e| ErrorData::resource_not_found(format!("{e:#}"), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, request.uri).with_mime_type("text/markdown"),
        ])
        .into())
    }

    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "Ship computer for an Elite Dangerous commander. If your client supports MCP \
             prompts, load `ship_computer` as the system prompt.\n\n\
             READING STATE: status, cargo, nav_route, market and recent_events read the live \
             game journal. They can't see the screen: Status.json says which panel is open, \
             not which tab, row or button is highlighted.\n\n\
             SEEING THE SCREEN: `screenshot` returns a picture of the game window only (never \
             the desktop) plus the open panel and status flags. Crop with `region` (full, \
             center, left, right, top, bottom, or a name from [screen_regions]) or `rect` \
             {x, y, w, h} in window fractions (0..1, the same space as click targets); crop \
             and use format png to read small text. Look before acting whenever you're unsure \
             what is highlighted. Images cost context: don't screenshot after every step.\n\n\
             ACTING: action tools press keys in the running game. Say what you're about to do \
             before calling them, and never chain them without the commander in the loop. \
             press_binding with observe=true returns the screen after the press plus \
             screen_changed_pct; near 0 means the press probably didn't land (often the game \
             lost focus), so look and ask rather than retry. plot_route plots to another \
             system; nav_target_start/move/confirm/cancel lock a station in this system.\n\n\
             GUIDES: read_guide (or resources guide://<name>) holds game concepts and mission \
             profiles. Read the matching profile when the commander starts an activity.\n\n\
             PLANNING: Spansh tools send explicit planning arguments externally and return \
             advice; never chain them to game actions automatically. Ask for the trading \
             budget and cargo space when unknown.",
        )
    }
}

async fn require_token(State(token): State<Arc<String>>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.as_bytes(), token.as_bytes()));
    if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "missing or wrong bearer token").into_response()
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Build the axum app: `/mcp` behind bearer auth.
pub fn app(ship: Arc<Ship>) -> Router {
    let config = ship.config();
    let mcp_config =
        StreamableHttpServerConfig::default().with_allowed_hosts(config.allowed_hosts.clone());
    let token = Arc::new(config.token.clone());
    let service: StreamableHttpService<Holtzman, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(Holtzman::new(ship.clone())),
        Default::default(),
        mcp_config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(token, require_token))
}

pub async fn serve(config: Config, config_path: PathBuf) -> Result<()> {
    let listen = config.listen.clone();
    let ship = Arc::new(Ship::new(config, Some(config_path))?);
    let missing = ship.config().missing_click_targets();
    if !missing.is_empty() {
        tracing::warn!(
            ?missing,
            "plot_route needs click targets; record them with the record_click_target tool"
        );
    }
    tracing::info!(journal = %ship.journal.dir.display(), "journal folder");
    if !crate::journal::looks_like_journal_dir(&ship.journal.dir) {
        tracing::warn!("that doesn't look like an Elite journal folder; set journal_dir");
    }
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!("holtzman listening on http://{listen}/mcp");
    axum::serve(listener, app(ship))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

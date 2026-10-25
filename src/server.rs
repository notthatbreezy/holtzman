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
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig},
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
    input::{KeySender, platform_sender},
    journal::JournalDir,
    screen::{Format, Region, Screen, platform_screen},
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
        description = "Look at the game: a picture of the Elite Dangerous window (never the rest of the desktop) plus the open panel and status flags from Status.json. Use it to orient before UI actions, e.g. to see which tab or row is highlighted. Crop with `region`/`rect` to read small text; rect coordinates are game-window fractions, the same space as click targets and cursor_position. Read-only; sends no input. Images cost context: prefer press_binding's `observe` while stepping through menus."
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

#[tool_handler]
impl ServerHandler for Holtzman {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Ship computer for an Elite Dangerous commander. Read tools reflect the live game \
             journal on the gaming PC; `screenshot` shows the game window when the journal \
             can't tell you what is on screen. Action tools press keys in the running game: say what \
             you're about to do before calling them, and never chain them without the \
             commander in the loop.",
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

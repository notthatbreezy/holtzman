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
    model::{ServerCapabilities, ServerConfig},
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
    spansh::{CommodityArgs, MarketSearch, RouteArgs, Spansh},
};

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
    pub spansh: Spansh,
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
            spansh: Spansh::new()?,
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
        description = "Press one allow-listed game binding (see `pressable` in holtzman.toml). Useful for stepping through menus and tuning recipes."
    )]
    async fn press_binding(
        &self,
        Parameters(args): Parameters<PressArgs>,
    ) -> Result<String, String> {
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
        self.ship.sender.focus_game().map_err(err)?;
        let config = self.ship.config();
        let pilot = Pilot {
            config: &config,
            bindings: &bindings,
            journal: &self.ship.journal,
            sender: self.ship.sender.as_ref(),
        };
        pilot
            .press_binding(&args.binding, args.hold_ms)
            .await
            .map_err(err)
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

#[tool_handler]
impl ServerHandler for Holtzman {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Ship computer for an Elite Dangerous commander. Read tools reflect the live game \
             journal on the gaming PC. Spansh trade tools send explicit planning arguments \
             externally and return advice; never automatically chain them to game actions. \
             Ask for the trading budget and cargo space when unknown. Action tools press keys in the running game: say what \
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

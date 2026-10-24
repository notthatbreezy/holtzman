//! The MCP server: tool definitions plus the HTTP plumbing (bearer auth,
//! host allow-list).

use std::sync::Arc;

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
    config::Config,
    input::{KeySender, platform_sender},
    journal::JournalDir,
};

/// Shared state behind every MCP session.
pub struct Ship {
    pub config: Config,
    pub journal: JournalDir,
    pub sender: Arc<dyn KeySender>,
    /// Only one keypress recipe may run at a time.
    pub controls: Mutex<()>,
}

impl Ship {
    pub fn new(config: Config) -> Result<Self> {
        let dir = config
            .journal_dir()
            .ok_or_else(|| anyhow::anyhow!("set journal_dir in holtzman.toml"))?;
        let sender = platform_sender(config.dry_run);
        Ok(Self {
            journal: JournalDir::new(dir),
            sender,
            controls: Mutex::new(()),
            config,
        })
    }

    /// Bindings are re-read on every use so control changes apply without a restart.
    fn bindings(&self) -> Result<Bindings, String> {
        let dir = self
            .config
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
        description = "Plot a route to a star system: opens the galaxy map, searches for the system, plots, closes the map, then confirms via NavRoute.json. Takes control of the keyboard for ~10 s, so tell the commander first. The game must be focused with no panel open."
    )]
    async fn plot_route(&self, Parameters(args): Parameters<PlotArgs>) -> Result<String, String> {
        let _guard = self
            .ship
            .controls
            .try_lock()
            .map_err(|_| "another keypress sequence is already running".to_string())?;
        let bindings = self.ship.bindings()?;
        let pilot = Pilot {
            config: &self.ship.config,
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
            .config
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
        let pilot = Pilot {
            config: &self.ship.config,
            bindings: &bindings,
            journal: &self.ship.journal,
            sender: self.ship.sender.as_ref(),
        };
        pilot
            .press_binding(&args.binding, args.hold_ms)
            .await
            .map_err(err)
    }
}

#[tool_handler]
impl ServerHandler for Holtzman {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Ship computer for an Elite Dangerous commander. Read tools reflect the live game \
             journal on the gaming PC. Action tools press keys in the running game: say what \
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
    let mcp_config =
        StreamableHttpServerConfig::default().with_allowed_hosts(ship.config.allowed_hosts.clone());
    let token = Arc::new(ship.config.token.clone());
    let service: StreamableHttpService<Holtzman, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(Holtzman::new(ship.clone())),
        Default::default(),
        mcp_config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(token, require_token))
}

pub async fn serve(config: Config) -> Result<()> {
    let listen = config.listen.clone();
    let ship = Arc::new(Ship::new(config)?);
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

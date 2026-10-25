//! Read-only access to the HTTP interface used by Spansh's public website.
//! This is an observed interface, not a guaranteed versioned API contract.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::{Client, RequestBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Semaphore};

const ORIGIN: &str = "https://spansh.co.uk";
const MAX_BODY: usize = 8 * 1024 * 1024;
const WARNING: &str = "Community market reports can be stale or depleted. Verify price, stock, access and legality in-game. Trade-leg distance is not a hyperspace jump count. Planning never sends game input.";

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommodityArgs {
    /// Case-insensitive part of a commodity name; empty lists the first matches.
    #[serde(default)]
    pub query: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeAction {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarketSearch {
    /// Reference star system supplied by the commander. Sent to Spansh.
    pub system: String,
    /// Exact commodity name (use trade_commodities to look it up).
    pub commodity: String,
    /// buy = commander purchases from station; sell = commander sells to station.
    pub action: TradeAction,
    /// Minimum supply (buy) or demand (sell), in tonnes. Default 1.
    #[serde(default = "one")]
    pub amount: u32,
    /// Search radius in light-years, NOT ship jump range. Default 50, maximum 500.
    #[serde(default = "radius")]
    pub radius_ly: f64,
    /// Maximum station distance from arrival in light-seconds. Default 10000.
    #[serde(default = "arrival")]
    pub max_arrival_ls: u32,
    /// Maximum reported market age in hours. Default 24, maximum 720.
    #[serde(default = "age")]
    pub max_age_hours: u32,
    /// Require a large landing pad; defaults true for bulk haulers.
    #[serde(default = "yes")]
    pub large_pad: bool,
    #[serde(default)]
    pub include_planetary: bool,
    /// Include fleet carriers with public docking access only. Default false.
    #[serde(default)]
    pub include_carriers: bool,
    /// Number of price-ranked results, default 10, maximum 20.
    #[serde(default = "limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouteArgs {
    /// Starting system supplied by the commander. Sent to Spansh.
    pub system: String,
    /// Starting station/market name, required by Spansh's trade optimizer.
    pub station: String,
    /// Empty cargo space to allocate, in tonnes. Not total ship capacity if partly loaded.
    pub cargo_tonnes: u32,
    /// Explicit purchasing budget in credits; do not silently use the entire balance.
    pub budget_credits: u64,
    /// Number of trade legs (NOT hyperspace jumps), default 2, maximum 6.
    #[serde(default = "legs")]
    pub max_legs: u32,
    /// Maximum distance between trading systems in ly; not a per-jump limit.
    #[serde(default = "radius")]
    pub max_leg_ly: f64,
    #[serde(default = "arrival")]
    pub max_arrival_ls: u32,
    #[serde(default = "age")]
    pub max_age_hours: u32,
    #[serde(default = "yes")]
    pub large_pad: bool,
    #[serde(default)]
    pub include_planetary: bool,
    /// Include publicly accessible player-owned markets. Default false.
    #[serde(default)]
    pub include_carriers: bool,
}

fn one() -> u32 {
    1
}
fn radius() -> f64 {
    50.0
}
fn arrival() -> u32 {
    10_000
}
fn age() -> u32 {
    24
}
fn yes() -> bool {
    true
}
fn limit() -> usize {
    10
}
fn legs() -> u32 {
    2
}

fn name<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let value = value.trim();
    ensure!(
        !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control),
        "{label} must be a nonempty name of at most 128 bytes without control characters"
    );
    Ok(value)
}

fn constraints(distance: f64, arrival: u32, age: u32) -> Result<()> {
    ensure!(
        distance.is_finite() && distance > 0.0 && distance <= 500.0,
        "distance must be greater than 0 and at most 500 ly"
    );
    ensure!(
        (1..=1_000_000).contains(&arrival),
        "max_arrival_ls must be 1..1000000"
    );
    ensure!((1..=720).contains(&age), "max_age_hours must be 1..720");
    Ok(())
}

impl MarketSearch {
    fn validate(&self) -> Result<()> {
        name(&self.system, "system")?;
        name(&self.commodity, "commodity")?;
        ensure!(
            (1..=10_000).contains(&self.amount),
            "amount must be 1..10000 tonnes"
        );
        ensure!((1..=20).contains(&self.limit), "limit must be 1..20");
        constraints(self.radius_ly, self.max_arrival_ls, self.max_age_hours)
    }

    fn body(&self, commodity: &str, types: &[String], now: DateTime<Utc>) -> Value {
        let (quantity, price, direction) = match self.action {
            TradeAction::Buy => ("supply", "buy_price", "asc"),
            TradeAction::Sell => ("demand", "sell_price", "desc"),
        };
        let mut filters = json!({
            "distance": {"min": 0, "max": self.radius_ly},
            "has_market": {"value": true},
            "distance_to_arrival": {"comparison": "<=>", "value": [0, self.max_arrival_ls]},
            "market_updated_at": {"comparison": "<=>", "value": [
                (now - chrono::Duration::hours(self.max_age_hours.into())).to_rfc3339_opts(SecondsFormat::Secs, true),
                now.to_rfc3339_opts(SecondsFormat::Secs, true)
            ]},
            "market": [{"name": commodity,
                quantity: {"comparison": "<=>", "value": [self.amount, 1_000_000_000]},
                price: {"comparison": "<=>", "value": [1, 1_000_000_000]}
            }]
        });
        if self.large_pad {
            filters["has_large_pad"] = json!({"value": true});
        }
        if !self.include_planetary {
            filters["is_planetary"] = json!({"value": false});
        }
        if !self.include_carriers {
            filters["type"] = json!({"value": types.iter().filter(|s| !s.to_ascii_lowercase().contains("carrier")).collect::<Vec<_>>()});
        }
        json!({"reference_system": self.system.trim(), "filters": filters,
            "sort": [{format!("market_{price}"): [{"name": commodity, "direction": direction}]}, {"distance": {"direction": "asc"}}],
            "size": 20, "page": 0})
    }
}

impl RouteArgs {
    fn validate(&self) -> Result<()> {
        name(&self.system, "system")?;
        name(&self.station, "station")?;
        ensure!(
            (1..=10_000).contains(&self.cargo_tonnes),
            "cargo_tonnes must be 1..10000"
        );
        ensure!(
            (1..=1_000_000_000_000).contains(&self.budget_credits),
            "budget_credits must be 1..1000000000000"
        );
        ensure!((1..=6).contains(&self.max_legs), "max_legs must be 1..6");
        constraints(self.max_leg_ly, self.max_arrival_ls, self.max_age_hours)
    }

    fn form(&self) -> Value {
        json!({"system": self.system.trim(), "station": self.station.trim(),
            "max_cargo": self.cargo_tonnes, "starting_capital": self.budget_credits,
            "max_hops": self.max_legs, "max_hop_distance": self.max_leg_ly,
            "max_system_distance": self.max_arrival_ls, "max_price_age": self.max_age_hours * 3600,
            "requires_large_pad": u8::from(self.large_pad), "allow_planetary": u8::from(self.include_planetary),
            "allow_player_owned": u8::from(self.include_carriers), "allow_restricted_access": 0,
            "allow_prohibited": 0, "permit": 0, "unique": 0})
    }
}

#[derive(Clone)]
pub struct Spansh {
    client: Client,
    origin: String,
    references: Arc<Mutex<Option<References>>>,
    requests: Arc<Semaphore>,
    deadline: Duration,
    poll: Duration,
}

struct References {
    at: Instant,
    commodities: Vec<String>,
    station_types: Vec<String>,
}

impl Spansh {
    pub fn new() -> Result<Self> {
        Self::at(ORIGIN)
    }

    fn at(origin: &str) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent(concat!("holtzman/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .build()?,
            origin: origin.into(),
            references: Arc::new(Mutex::new(None)),
            requests: Arc::new(Semaphore::new(2)),
            deadline: Duration::from_secs(45),
            poll: Duration::from_secs(1),
        })
    }

    async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let mut response = request
            .send()
            .await
            .context("Spansh connection failed or timed out")?;
        let status = response.status();
        if status.as_u16() == 429 {
            bail!("Spansh rate limit reached; wait before requesting another plan");
        }
        ensure!(
            status.is_success(),
            "Spansh returned HTTP {status}; check system/station names and constraints, or try later"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.context("reading Spansh response")? {
            ensure!(
                bytes.len() + chunk.len() <= MAX_BODY,
                "Spansh response exceeded the size limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).context("Spansh response format changed or was invalid JSON")
    }

    async fn reference_data(&self) -> Result<(Vec<String>, Vec<String>)> {
        let mut cached = self.references.lock().await;
        if let Some(r) = &*cached
            && r.at.elapsed() < Duration::from_secs(3600)
        {
            return Ok((r.commodities.clone(), r.station_types.clone()));
        }
        #[derive(Deserialize)]
        struct Values {
            values: Vec<String>,
        }
        let commodities: Values = self
            .json(
                self.client
                    .get(format!("{}/api/stations/field_values/market", self.origin)),
            )
            .await?;
        let types: Values = self
            .json(
                self.client
                    .get(format!("{}/api/stations/field_values/type", self.origin)),
            )
            .await?;
        ensure!(
            !commodities.values.is_empty() && !types.values.is_empty(),
            "Spansh returned empty reference data"
        );
        let result = (commodities.values.clone(), types.values.clone());
        *cached = Some(References {
            at: Instant::now(),
            commodities: commodities.values,
            station_types: types.values,
        });
        Ok(result)
    }

    pub async fn commodities(&self, args: CommodityArgs) -> Result<Value> {
        ensure!(args.query.len() <= 128, "query must be at most 128 bytes");
        let _permit = self
            .requests
            .try_acquire()
            .context("Spansh is busy with other planning requests; try again shortly")?;
        let (names, _) = self.reference_data().await?;
        let query = args.query.trim().to_lowercase();
        let matches: Vec<_> = names
            .into_iter()
            .filter(|s| s.to_lowercase().contains(&query))
            .collect();
        Ok(
            json!({"provider": "Spansh", "total_matches": matches.len(), "commodities": matches.into_iter().take(50).collect::<Vec<_>>()}),
        )
    }

    pub async fn markets(&self, args: MarketSearch) -> Result<Value> {
        args.validate()?;
        let _permit = self
            .requests
            .try_acquire()
            .context("Spansh is busy with other planning requests; try again shortly")?;
        let (names, types) = self.reference_data().await?;
        let commodity = names
            .iter()
            .find(|s| s.eq_ignore_ascii_case(args.commodity.trim()))
            .context("unknown commodity; use trade_commodities to find its exact name")?;
        let now = Utc::now();
        let body = args.body(commodity, &types, now);
        let response: StationResults = self
            .json(
                self.client
                    .post(format!("{}/api/stations/search", self.origin))
                    .json(&body),
            )
            .await?;
        ensure!(
            response
                .reference
                .name
                .eq_ignore_ascii_case(args.system.trim()),
            "Spansh resolved a different reference system; check its exact name"
        );
        let searched = response.results.len();
        let markets: Vec<_> = response
            .results
            .into_iter()
            .filter_map(|s| s.offer(&args, commodity, now))
            .take(args.limit)
            .collect();
        Ok(
            json!({"provider": "Spansh", "query": args, "commodity": commodity,
            "provider_matches": response.count, "candidates_checked": searched,
            "markets": markets, "note": "Price-ranked candidates; nonpublic carriers, unknown/stale market ages and prohibited markets are omitted. Ordinary-station permit/access requirements remain unverified. Empty results may need a wider radius or older price limit.", "warning": WARNING}),
        )
    }

    pub async fn route(&self, args: RouteArgs) -> Result<Value> {
        args.validate()?;
        let _permit = self
            .requests
            .try_acquire()
            .context("Spansh is busy with other planning requests; try again shortly")?;
        tokio::time::timeout(self.deadline, self.route_inner(args)).await
            .context("Spansh trade calculation timed out; try a shorter itinerary or less restrictive filters")?
    }

    async fn route_inner(&self, args: RouteArgs) -> Result<Value> {
        let submission: Job = self
            .json(
                self.client
                    .post(format!("{}/api/trade/route", self.origin))
                    .form(&args.form()),
            )
            .await?;
        ensure!(
            submission.status == "queued",
            "Spansh rejected the trade calculation: {}",
            submission.error.as_deref().unwrap_or("unexpected status")
        );
        let job = submission
            .job
            .context("Spansh response omitted the job identifier")?;
        ensure!(
            !job.is_empty()
                && job.len() <= 80
                && job.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "Spansh returned an invalid job identifier"
        );
        let mut wait = self.poll;
        loop {
            tokio::time::sleep(wait).await;
            let result: Job = self
                .json(
                    self.client
                        .get(format!("{}/api/results/{job}", self.origin)),
                )
                .await?;
            match result.status.as_str() {
                "queued" | "running" => {
                    wait = (wait * 2).min(Duration::from_secs(4));
                }
                "ok" => {
                    let raw = result
                        .result
                        .context("Spansh completed without trade results")?;
                    let mut legs: Vec<TradeLeg> = serde_json::from_value(raw)
                        .context("Spansh trade-result format changed")?;
                    ensure!(
                        legs.len() <= args.max_legs as usize,
                        "Spansh returned more legs than requested"
                    );
                    validate_route(&legs, &args)?;
                    for leg in &mut legs {
                        leg.source.set_age();
                        leg.destination.set_age();
                    }
                    return Ok(json!({"provider": "Spansh", "query": args, "legs": legs,
                        "result_url": format!("{}/trade/results/{job}", self.origin),
                        "note": "An empty itinerary means no matching trades were found. Profit estimates come from reported market prices; this is not a hyperspace route.", "warning": WARNING}));
                }
                _ => bail!(
                    "Spansh trade calculation failed: {}",
                    result.error.as_deref().unwrap_or("unknown provider status")
                ),
            }
        }
    }
}

#[derive(Deserialize)]
struct Job {
    status: String,
    job: Option<String>,
    error: Option<String>,
    result: Option<Value>,
}

#[derive(Deserialize)]
struct StationResults {
    count: u64,
    reference: SystemReference,
    results: Vec<Station>,
}
#[derive(Deserialize)]
struct SystemReference {
    name: String,
}

#[derive(Deserialize)]
struct Station {
    name: String,
    system_name: String,
    market_id: u64,
    distance: f64,
    distance_to_arrival: Option<f64>,
    has_large_pad: Option<bool>,
    is_planetary: Option<bool>,
    #[serde(rename = "type")]
    kind: String,
    carrier_docking_access: Option<String>,
    market_updated_at: Option<String>,
    market: Vec<MarketRow>,
    #[serde(default)]
    prohibited_commodities: Vec<CommodityName>,
}

#[derive(Deserialize)]
struct CommodityName {
    name: String,
}
#[derive(Deserialize)]
struct MarketRow {
    commodity: String,
    buy_price: u64,
    sell_price: u64,
    supply: u64,
    demand: u64,
}

impl Station {
    fn offer(self, args: &MarketSearch, commodity: &str, now: DateTime<Utc>) -> Option<Value> {
        if !self.distance.is_finite() || self.distance > args.radius_ly || self.distance < 0.0 {
            return None;
        }
        if args.large_pad && self.has_large_pad != Some(true) {
            return None;
        }
        if !args.include_planetary && self.is_planetary != Some(false) {
            return None;
        }
        let carrier = self.kind.to_ascii_lowercase().contains("carrier");
        if carrier
            && (!args.include_carriers || self.carrier_docking_access.as_deref() != Some("All"))
        {
            return None;
        }
        if self
            .prohibited_commodities
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(commodity))
        {
            return None;
        }
        let arrival = self.distance_to_arrival?;
        if !arrival.is_finite() || arrival < 0.0 || arrival > args.max_arrival_ls.into() {
            return None;
        }
        let updated = DateTime::parse_from_rfc3339(self.market_updated_at.as_deref()?)
            .ok()?
            .with_timezone(&Utc);
        let seconds = now.signed_duration_since(updated).num_seconds();
        if seconds < -300 || seconds > i64::from(args.max_age_hours) * 3600 {
            return None;
        }
        let row = self
            .market
            .iter()
            .find(|m| m.commodity.eq_ignore_ascii_case(commodity))?;
        let (price, available) = match args.action {
            TradeAction::Buy => (row.buy_price, row.supply),
            TradeAction::Sell => (row.sell_price, row.demand),
        };
        if price == 0 || available < args.amount.into() {
            return None;
        }
        Some(
            json!({"system": self.system_name, "station": self.name, "market_id": self.market_id,
            "station_type": self.kind, "distance_ly": self.distance, "arrival_ls": arrival,
            "price_per_tonne": price, "supply": row.supply, "demand": row.demand,
            "value_for_requested_amount": price.checked_mul(args.amount.into())?,
            "market_updated_at": updated.to_rfc3339(), "market_age_hours": seconds.max(0) as f64 / 3600.0,
            "station_url": format!("{}/station/{}", ORIGIN, self.market_id)}),
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct RouteStation {
    system: String,
    station: String,
    market_id: u64,
    distance_to_arrival: f64,
    market_updated_at: i64,
    #[serde(default)]
    market_age_hours: Option<f64>,
}
impl RouteStation {
    fn set_age(&mut self) {
        self.market_age_hours =
            Some((Utc::now().timestamp() - self.market_updated_at).max(0) as f64 / 3600.0);
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Quote {
    buy_price: u64,
    sell_price: u64,
    supply: u64,
    demand: u64,
}
#[derive(Debug, Deserialize, Serialize)]
struct TradeCommodity {
    name: String,
    amount: u32,
    profit: i64,
    total_profit: i64,
    source_commodity: Quote,
    destination_commodity: Quote,
}
#[derive(Debug, Deserialize, Serialize)]
struct TradeLeg {
    source: RouteStation,
    destination: RouteStation,
    distance: f64,
    commodities: Vec<TradeCommodity>,
    total_profit: i64,
    cumulative_profit: i64,
}

fn validate_route(legs: &[TradeLeg], args: &RouteArgs) -> Result<()> {
    let mut budget = i128::from(args.budget_credits);
    let mut cumulative = 0i128;
    for (i, leg) in legs.iter().enumerate() {
        let expected = if i == 0 {
            (&args.system, &args.station)
        } else {
            (
                &legs[i - 1].destination.system,
                &legs[i - 1].destination.station,
            )
        };
        ensure!(
            leg.source.system.eq_ignore_ascii_case(expected.0.trim())
                && leg.source.station.eq_ignore_ascii_case(expected.1.trim()),
            "Spansh returned a different starting market or discontinuous itinerary"
        );
        ensure!(
            leg.distance.is_finite()
                && leg.distance >= 0.0
                && leg.distance <= args.max_leg_ly + 0.01,
            "Spansh returned a trade leg outside the requested distance"
        );
        for station in [&leg.source, &leg.destination] {
            let age = Utc::now().timestamp() - station.market_updated_at;
            ensure!(
                age >= -300 && age <= i64::from(args.max_age_hours) * 3600,
                "Spansh returned stale or invalid market timestamps; request a fresh plan"
            );
            ensure!(
                station.distance_to_arrival.is_finite()
                    && station.distance_to_arrival >= 0.0
                    && station.distance_to_arrival <= f64::from(args.max_arrival_ls),
                "Spansh returned a station beyond the arrival-distance limit"
            );
        }
        let mut cargo = 0u64;
        let mut cost = 0i128;
        let mut profit = 0i128;
        ensure!(
            !leg.commodities.is_empty(),
            "Spansh returned an empty trade leg"
        );
        for c in &leg.commodities {
            ensure!(
                c.amount > 0
                    && c.source_commodity.buy_price > 0
                    && c.destination_commodity.sell_price > 0,
                "Spansh returned invalid trade prices or quantities"
            );
            cargo += u64::from(c.amount);
            ensure!(
                u64::from(c.amount) <= c.source_commodity.supply
                    && u64::from(c.amount) <= c.destination_commodity.demand,
                "Spansh returned insufficient supply or demand"
            );
            let per_tonne = i128::from(c.destination_commodity.sell_price)
                - i128::from(c.source_commodity.buy_price);
            let total = per_tonne * i128::from(c.amount);
            ensure!(
                per_tonne > 0
                    && per_tonne == i128::from(c.profit)
                    && total == i128::from(c.total_profit),
                "Spansh profit estimates do not match the reported prices"
            );
            cost += i128::from(c.source_commodity.buy_price) * i128::from(c.amount);
            profit += total;
        }
        ensure!(
            cargo <= args.cargo_tonnes.into() && cost <= budget,
            "Spansh returned a trade beyond the cargo space or purchasing budget"
        );
        cumulative += profit;
        ensure!(
            profit == i128::from(leg.total_profit)
                && cumulative == i128::from(leg.cumulative_profit),
            "Spansh returned inconsistent itinerary totals"
        );
        budget += profit;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::{Form, State},
        http::StatusCode,
        routing::{get, post},
    };
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    fn search(action: &str) -> MarketSearch {
        serde_json::from_value(
            json!({"system":"Sol","commodity":"Tea","action":action,"amount":10}),
        )
        .unwrap()
    }
    fn route_args() -> RouteArgs {
        serde_json::from_value(
            json!({"system":"Sol","station":"Galileo","cargo_tonnes":10,"budget_credits":10000}),
        )
        .unwrap()
    }
    fn station() -> Value {
        json!({"name":"Example Port","system_name":"Example System","market_id":9007199254740993u64,
            "distance":12.0,"distance_to_arrival":400.0,"has_large_pad":true,"is_planetary":false,
            "type":"Coriolis","market_updated_at":Utc::now().to_rfc3339(),
            "market":[{"commodity":"Tea","buy_price":100,"sell_price":250,"supply":100,"demand":100}]})
    }
    fn leg() -> Value {
        let port = |system: &str, station: &str| {
            json!({"system":system,"station":station,
            "market_id":9007199254740993u64,"distance_to_arrival":400.0,"market_updated_at":Utc::now().timestamp()})
        };
        json!({"source":port("Sol","Galileo"),"destination":port("Example System","Example Port"),
            "distance":12.0,"commodities":[{"name":"Tea","amount":10,"profit":150,"total_profit":1500,
                "source_commodity":{"buy_price":100,"sell_price":90,"supply":100,"demand":0},
                "destination_commodity":{"buy_price":0,"sell_price":250,"supply":0,"demand":100}}],
            "total_profit":1500,"cumulative_profit":1500})
    }
    async fn client(app: Router) -> Spansh {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut client = Spansh::at(&origin).unwrap();
        client.poll = Duration::from_millis(1);
        client
    }

    #[test]
    fn validates_inputs_before_network_access() {
        let mut args = search("buy");
        args.radius_ly = f64::NAN;
        assert!(args.validate().is_err());
        args = search("buy");
        args.amount = 0;
        assert!(args.validate().is_err());
        let mut args = route_args();
        args.budget_credits = 0;
        assert!(args.validate().is_err());
        args = route_args();
        args.max_legs = 100;
        assert!(args.validate().is_err());
        assert!(serde_json::from_value::<RouteArgs>(json!({"system":"Sol","station":"Galileo","cargo_tonnes":10,"budget_credits":1000,"token":"must not be accepted"})).is_err());
    }

    #[test]
    fn market_filters_match_commander_buy_sell_perspective() {
        let types = vec!["Coriolis".into(), "Drake-Class Carrier".into()];
        let buy = search("buy").body("Tea", &types, Utc::now());
        assert_eq!(buy["filters"]["market"][0]["supply"]["value"][0], 10);
        assert_eq!(buy["sort"][0]["market_buy_price"][0]["direction"], "asc");
        assert!(
            buy["filters"]["market_updated_at"]["value"][0]
                .as_str()
                .unwrap()
                .ends_with('Z')
        );
        assert_eq!(buy["filters"]["type"]["value"], json!(["Coriolis"]));
        let sell = search("sell").body("Tea", &types, Utc::now());
        assert_eq!(sell["filters"]["market"][0]["demand"]["value"][0], 10);
        assert_eq!(sell["sort"][0]["market_sell_price"][0]["direction"], "desc");
        let form = route_args().form();
        assert_eq!(form["allow_prohibited"], 0);
        assert_eq!(form["allow_restricted_access"], 0);
        assert_eq!(form["requires_large_pad"], 1);
    }

    #[test]
    fn refuses_stale_illegal_or_inaccessible_offers() {
        let args = search("buy");
        let parse = |s: Value| serde_json::from_value::<Station>(s).unwrap();
        let offer = parse(station()).offer(&args, "Tea", Utc::now()).unwrap();
        assert_eq!(offer["price_per_tonne"], 100);
        assert_eq!(offer["value_for_requested_amount"], 1000);
        assert_eq!(offer["market_id"].as_u64(), Some(9007199254740993));
        let mut s = station();
        s["market_updated_at"] = json!((Utc::now() - chrono::Duration::days(2)).to_rfc3339());
        assert!(parse(s).offer(&args, "Tea", Utc::now()).is_none());
        let mut s = station();
        s["prohibited_commodities"] = json!([{"name":"Tea"}]);
        assert!(parse(s).offer(&args, "Tea", Utc::now()).is_none());
        let mut s = station();
        s["type"] = json!("Drake-Class Carrier");
        s["carrier_docking_access"] = json!("Friends");
        let mut with_carriers = args.clone();
        with_carriers.include_carriers = true;
        assert!(parse(s).offer(&with_carriers, "Tea", Utc::now()).is_none());
        let mut s = station();
        s["has_large_pad"] = json!(false);
        assert!(parse(s).offer(&args, "Tea", Utc::now()).is_none());
    }

    #[test]
    fn checks_route_budget_continuity_and_profit_arithmetic() {
        let parse = |v| serde_json::from_value::<TradeLeg>(v).unwrap();
        assert!(validate_route(&[parse(leg())], &route_args()).is_ok());
        let mut v = leg();
        v["commodities"][0]["total_profit"] = json!(99999);
        assert!(validate_route(&[parse(v)], &route_args()).is_err());
        let mut v = leg();
        v["source"]["station"] = json!("Wrong station");
        assert!(validate_route(&[parse(v)], &route_args()).is_err());
        let mut args = route_args();
        args.budget_credits = 999;
        assert!(validate_route(&[parse(leg())], &args).is_err());
        let mut v = leg();
        v["commodities"][0]["amount"] = json!(11);
        assert!(validate_route(&[parse(v)], &route_args()).is_err());
        let mut v = leg();
        v["destination"]["market_updated_at"] = json!(0);
        assert!(validate_route(&[parse(v)], &route_args()).is_err());
    }

    #[tokio::test]
    async fn station_search_uses_typed_filters_and_cached_reference_data() {
        let reads = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/api/stations/field_values/market",
                get(|State(reads): State<Arc<AtomicUsize>>| async move {
                    reads.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({"values":["Tea","Uranium"]}))
                }),
            )
            .route(
                "/api/stations/field_values/type",
                get(|| async { axum::Json(json!({"values":["Coriolis","Drake-Class Carrier"]})) }),
            )
            .route(
                "/api/stations/search",
                post(|axum::Json(body): axum::Json<Value>| async move {
                    assert_eq!(body["reference_system"], "Sol");
                    assert_eq!(body["filters"]["type"]["value"], json!(["Coriolis"]));
                    axum::Json(json!({"reference":{"name":"Sol"},"count":1,"results":[station()]}))
                }),
            )
            .with_state(reads.clone());
        let client = client(app).await;
        let found = client
            .commodities(CommodityArgs {
                query: "URAN".into(),
            })
            .await
            .unwrap();
        assert_eq!(found["commodities"], json!(["Uranium"]));
        let result = client.markets(search("buy")).await.unwrap();
        assert_eq!(result["markets"][0]["price_per_tonne"], 100);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!(
            client
                .markets(MarketSearch {
                    commodity: "not a commodity".into(),
                    ..search("buy")
                })
                .await
                .unwrap_err()
                .to_string()
                .contains("unknown commodity")
        );
    }

    #[tokio::test]
    async fn submits_route_once_then_polls_to_completion() {
        let counts = Arc::new((AtomicUsize::new(0), AtomicUsize::new(0)));
        let app = Router::new()
            .route(
                "/api/trade/route",
                post(
                    |State(counts): State<Arc<(AtomicUsize, AtomicUsize)>>,
                     Form(form): Form<HashMap<String, String>>| async move {
                        counts.0.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(form["max_cargo"], "10");
                        assert_eq!(form["allow_prohibited"], "0");
                        axum::Json(json!({"status":"queued","job":"example-job"}))
                    },
                ),
            )
            .route(
                "/api/results/example-job",
                get(
                    |State(counts): State<Arc<(AtomicUsize, AtomicUsize)>>| async move {
                        if counts.1.fetch_add(1, Ordering::SeqCst) == 0 {
                            axum::Json(json!({"status":"queued"}))
                        } else {
                            axum::Json(json!({"status":"ok","result":[leg()]}))
                        }
                    },
                ),
            )
            .with_state(counts.clone());
        let result = client(app).await.route(route_args()).await.unwrap();
        assert_eq!(result["legs"][0]["total_profit"], 1500);
        assert_eq!(counts.0.load(Ordering::SeqCst), 1);
        assert_eq!(counts.1.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn distinguishes_empty_results_rate_limits_and_invalid_responses() {
        let app = Router::new()
            .route(
                "/api/trade/route",
                post(|| async { axum::Json(json!({"status":"queued","job":"empty"})) }),
            )
            .route(
                "/api/results/empty",
                get(|| async { axum::Json(json!({"status":"ok","result":[]})) }),
            );
        assert_eq!(
            client(app).await.route(route_args()).await.unwrap()["legs"],
            json!([])
        );
        let app = Router::new().route(
            "/api/trade/route",
            post(|| async { StatusCode::TOO_MANY_REQUESTS }),
        );
        assert!(
            client(app)
                .await
                .route(route_args())
                .await
                .unwrap_err()
                .to_string()
                .contains("rate limit")
        );
        let app = Router::new().route("/api/trade/route", post(|| async { "not json" }));
        assert!(
            client(app)
                .await
                .route(route_args())
                .await
                .unwrap_err()
                .to_string()
                .contains("format changed")
        );
        let app = Router::new().route(
            "/api/trade/route",
            post(|| async { axum::Json(json!({"status":"queued","job":"../other"})) }),
        );
        assert!(
            client(app)
                .await
                .route(route_args())
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid job")
        );
    }

    #[tokio::test]
    async fn queued_job_has_a_bounded_timeout() {
        let app = Router::new()
            .route(
                "/api/trade/route",
                post(|| async { axum::Json(json!({"status":"queued","job":"slow"})) }),
            )
            .route(
                "/api/results/slow",
                get(|| async { axum::Json(json!({"status":"queued"})) }),
            );
        let mut client = client(app).await;
        client.deadline = Duration::from_millis(30);
        assert!(
            client
                .route(route_args())
                .await
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }
}

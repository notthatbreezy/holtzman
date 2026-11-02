//! General station facilities and affiliation search using Spansh's station index.
use std::collections::{BTreeMap, HashSet};

use super::*;

const FIELDS: [&str; 7] = [
    "services",
    "allegiance",
    "material_trader",
    "technology_broker",
    "government",
    "primary_economy",
    "type",
];
const PAGE_SIZE: usize = 20;
const MAX_PAGES: usize = 5;
const STATION_WARNING: &str = "Community station reports may be stale. Verify docking access, permits, station state and service availability in-game. Allegiance is the station's reported affiliation, not system allegiance or Powerplay control. Search never plots a route or sends game input.";

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StationOptions {
    /// Optional case-insensitive substring of a filter name or value, e.g. factors, trader, Empire or repair.
    #[serde(default)]
    pub query: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StationSearch {
    /// Explicit reference star system, sent to Spansh. Results rank by ly distance, not jump count.
    pub system: String,
    /// Station allegiance, e.g. Empire, Federation, Alliance. Not system or Powerplay affiliation.
    pub allegiance: Option<String>,
    /// ALL required services. Accepts Interstellar Factors or Interstellar Factors Contact,
    /// Material Trader, Repair, Shipyard, etc. Use station_search_options for names.
    #[serde(default)]
    pub services: Vec<String>,
    /// Raw, Manufactured or Encoded. Also requires the Material Trader service.
    pub material_trader: Option<String>,
    /// Optional broker subtype; use station_search_options. Also requires Technology Broker.
    pub technology_broker: Option<String>,
    pub government: Option<String>,
    /// Station primary economy, e.g. High Tech, Industrial or Refinery.
    pub economy: Option<String>,
    /// Optional exact station type from station_search_options.
    pub station_type: Option<String>,
    /// Radius in light-years, default 50, maximum 500. Not the ship's jump range.
    #[serde(default = "radius")]
    pub radius_ly: f64,
    /// Maximum station distance from arrival in light-seconds, default 10000.
    #[serde(default = "arrival")]
    pub max_arrival_ls: u32,
    /// Maximum age of the station report in days, default 30; null disables the age filter.
    #[serde(default = "station_age")]
    pub max_age_days: Option<u32>,
    /// Require a large landing pad (default true); false also admits smaller/unknown pads.
    #[serde(default = "yes")]
    pub large_pad: bool,
    #[serde(default)]
    pub include_planetary: bool,
    /// Include carriers only when their reported docking access is All. Default false.
    #[serde(default)]
    pub include_carriers: bool,
    /// Number of nearest eligible stations, default 10, maximum 20.
    #[serde(default = "limit")]
    pub limit: usize,
}

fn station_age() -> Option<u32> {
    Some(30)
}

fn normalized(value: &str) -> String {
    value.trim().to_lowercase().replace(['_', '-'], " ")
}

#[derive(Clone)]
pub(super) struct ReferenceCache {
    at: Instant,
    values: BTreeMap<String, Vec<String>>,
}

impl ReferenceCache {
    fn canonical(&self, field: &str, value: &str) -> Result<String> {
        let mut key = normalized(value);
        if field == "services" && key == "interstellar factors" {
            key = "interstellar factors contact".into();
        }
        self.values[field].iter().find(|v| normalized(v) == key).cloned()
            .with_context(|| format!("unknown {field} value {value:?}; use station_search_options to find supported names"))
    }
}

impl StationSearch {
    fn validate(&self) -> Result<()> {
        name(&self.system, "system")?;
        distance_constraints(self.radius_ly, self.max_arrival_ls)?;
        ensure!((1..=20).contains(&self.limit), "limit must be 1..20");
        ensure!(
            self.services.len() <= 8,
            "at most 8 required services are allowed"
        );
        if let Some(days) = self.max_age_days {
            ensure!(
                (1..=3650).contains(&days),
                "max_age_days must be 1..3650 or null"
            );
        }
        for value in self.services.iter().chain(
            [
                &self.allegiance,
                &self.material_trader,
                &self.technology_broker,
                &self.government,
                &self.economy,
                &self.station_type,
            ]
            .into_iter()
            .flatten(),
        ) {
            name(value, "station filter")?;
        }
        Ok(())
    }

    fn resolve(&mut self, refs: &ReferenceCache) -> Result<()> {
        self.system = self.system.trim().into();
        for (field, value) in [
            ("allegiance", &mut self.allegiance),
            ("material_trader", &mut self.material_trader),
            ("technology_broker", &mut self.technology_broker),
            ("government", &mut self.government),
            ("primary_economy", &mut self.economy),
            ("type", &mut self.station_type),
        ] {
            if let Some(v) = value {
                *v = refs.canonical(field, v)?;
            }
        }
        self.services = self
            .services
            .iter()
            .map(|s| refs.canonical("services", s))
            .collect::<Result<_>>()?;
        if self.material_trader.is_some() {
            self.services
                .push(refs.canonical("services", "Material Trader")?);
        }
        if self.technology_broker.is_some() {
            self.services
                .push(refs.canonical("services", "Technology Broker")?);
        }
        self.services.sort();
        self.services.dedup();
        if let Some(kind) = &self.station_type {
            ensure!(
                self.include_carriers || !is_carrier(kind),
                "set include_carriers=true to search a carrier station type"
            );
        }
        Ok(())
    }

    fn body(&self, refs: &ReferenceCache, now: DateTime<Utc>, page: usize) -> Value {
        let mut filters = json!({
            "distance": {"min": 0, "max": self.radius_ly},
            "distance_to_arrival": {"comparison":"<=>", "value":[0, self.max_arrival_ls]},
        });
        for (field, value) in [
            ("allegiance", &self.allegiance),
            ("material_trader", &self.material_trader),
            ("technology_broker", &self.technology_broker),
            ("government", &self.government),
            ("primary_economy", &self.economy),
        ] {
            if let Some(value) = value {
                filters[field] = json!({"value":[value]});
            }
        }
        if !self.services.is_empty() {
            filters["services"] = json!(
                self.services
                    .iter()
                    .map(|s| json!({"name":s}))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(kind) = &self.station_type {
            filters["type"] = json!({"value":[kind]});
        } else if !self.include_carriers {
            filters["type"] = json!({"value":refs.values["type"].iter().filter(|s| !is_carrier(s)).collect::<Vec<_>>()});
        }
        if self.large_pad {
            filters["has_large_pad"] = json!({"value":true});
        }
        if !self.include_planetary {
            filters["is_planetary"] = json!({"value":false});
        }
        if let Some(days) = self.max_age_days {
            filters["updated_at"] = json!({"comparison":"<=>", "value":[
                (now - chrono::Duration::days(days.into())).to_rfc3339_opts(SecondsFormat::Secs, true),
                now.to_rfc3339_opts(SecondsFormat::Secs, true)
            ]});
        }
        json!({"reference_system":self.system, "filters":filters,
            "sort":[{"distance":{"direction":"asc"}}, {"distance_to_arrival":{"direction":"asc"}}],
            "size":PAGE_SIZE, "page":page})
    }
}

fn is_carrier(kind: &str) -> bool {
    kind.to_ascii_lowercase().contains("carrier")
}

impl Spansh {
    async fn station_reference_data(&self) -> Result<ReferenceCache> {
        let mut cache = self.station_references.lock().await;
        if let Some(refs) = &*cache
            && refs.at.elapsed() < Duration::from_secs(3600)
        {
            return Ok(refs.clone());
        }
        let mut values = BTreeMap::new();
        for field in FIELDS {
            let response: Value = self
                .json(
                    self.client
                        .get(format!("{}/api/stations/field_values/{field}", self.origin)),
                )
                .await?;
            let raw = if field == "services" {
                &response["values"]["name"]
            } else {
                &response["values"]
            };
            let names: Vec<String> = serde_json::from_value(raw.clone())
                .context("Spansh station reference-data format changed")?;
            ensure!(
                !names.is_empty(),
                "Spansh returned empty {field} reference data"
            );
            values.insert(field.into(), names);
        }
        let refs = ReferenceCache {
            at: Instant::now(),
            values,
        };
        *cache = Some(refs.clone());
        Ok(refs)
    }

    pub async fn station_options(&self, args: StationOptions) -> Result<Value> {
        ensure!(
            args.query.len() <= 128 && !args.query.chars().any(char::is_control),
            "query must be at most 128 bytes without control characters"
        );
        let _permit = self
            .requests
            .try_acquire()
            .context("Spansh is busy with other planning requests; try again shortly")?;
        let refs = tokio::time::timeout(self.deadline, self.station_reference_data())
            .await
            .context("Spansh station options timed out")??;
        let query = normalized(&args.query);
        let values: BTreeMap<_, Vec<_>> = refs
            .values
            .into_iter()
            .map(|(field, names)| {
                let argument = match field.as_str() {
                    "primary_economy" => "economy",
                    "type" => "station_type",
                    other => other,
                }
                .to_string();
                let all = normalized(&argument).contains(&query);
                (
                    argument,
                    names
                        .into_iter()
                        .filter(|s| all || normalized(s).contains(&query))
                        .collect(),
                )
            })
            .collect();
        Ok(json!({"provider":"Spansh", "filters":values,
            "note":"Pass these names to find_stations. All requested services are required. Allegiance means station affiliation. Interstellar Factors is accepted as an alias for Interstellar Factors Contact."}))
    }

    pub async fn stations(&self, args: StationSearch) -> Result<Value> {
        args.validate()?;
        let _permit = self
            .requests
            .try_acquire()
            .context("Spansh is busy with other planning requests; try again shortly")?;
        tokio::time::timeout(self.deadline, self.stations_inner(args))
            .await
            .context("Spansh station search timed out; try again later")?
    }

    async fn stations_inner(&self, mut args: StationSearch) -> Result<Value> {
        let refs = self.station_reference_data().await?;
        args.resolve(&refs)?;
        let now = Utc::now();
        let mut matches = Vec::new();
        let mut seen = HashSet::new();
        let mut checked = 0usize;
        let mut count = 0;
        let mut exhausted = false;
        let mut previous_distance = 0.0;
        for page in 0..MAX_PAGES {
            let response: SearchResults = self
                .json(
                    self.client
                        .post(format!("{}/api/stations/search", self.origin))
                        .json(&args.body(&refs, now, page)),
                )
                .await?;
            ensure!(
                response.reference.name.eq_ignore_ascii_case(&args.system),
                "Spansh resolved a different reference system; check its exact name"
            );
            ensure!(
                response.results.len() <= PAGE_SIZE,
                "Spansh returned an oversized station page"
            );
            if page == 0 {
                count = response.count;
            }
            checked += response.results.len();
            exhausted = response.results.len() < PAGE_SIZE || checked as u64 >= response.count;
            for station in response.results {
                if station.distance.is_finite() && station.distance >= 0.0 {
                    ensure!(
                        station.distance + 0.000001 >= previous_distance,
                        "Spansh returned stations out of distance order; cannot rank nearest matches"
                    );
                    previous_distance = station.distance;
                }
                if let Some(result) = station.matched(&args, now)
                    && seen.insert(station.market_id)
                {
                    matches.push(result);
                }
            }
            if matches.len() >= args.limit || exhausted {
                break;
            }
        }
        matches.sort_by(|a, b| {
            a["distance_ly"]
                .as_f64()
                .unwrap()
                .total_cmp(&b["distance_ly"].as_f64().unwrap())
                .then_with(|| {
                    a["arrival_ls"]
                        .as_f64()
                        .unwrap()
                        .total_cmp(&b["arrival_ls"].as_f64().unwrap())
                })
        });
        let complete = exhausted || matches.len() >= args.limit;
        matches.truncate(args.limit);
        Ok(
            json!({"provider":"Spansh", "query":args, "provider_matches":count,
            "candidates_checked":checked, "search_complete":complete, "stations":matches,
            "note":"Nearest eligible stations by straight-line light-years within the stated filters, not hyperspace jumps. All requested services are checked locally. Up to 100 provider candidates are examined; search_complete=false means the scan limit was reached before enough verified matches were found. Empty results do not relax your constraints.",
            "warning":STATION_WARNING}),
        )
    }
}

#[derive(Deserialize)]
struct SearchResults {
    count: u64,
    reference: SystemReference,
    results: Vec<StationRecord>,
}

#[derive(Deserialize)]
struct StationRecord {
    name: String,
    system_name: String,
    market_id: u64,
    distance: f64,
    distance_to_arrival: Option<f64>,
    has_large_pad: Option<bool>,
    is_planetary: Option<bool>,
    #[serde(rename = "type")]
    kind: String,
    allegiance: Option<String>,
    controlling_minor_faction: Option<String>,
    government: Option<String>,
    primary_economy: Option<String>,
    state: Option<String>,
    material_trader: Option<String>,
    technology_broker: Option<String>,
    carrier_docking_access: Option<String>,
    updated_at: Option<String>,
    services: Option<Vec<CommodityName>>,
}

impl StationRecord {
    fn matched(&self, args: &StationSearch, now: DateTime<Utc>) -> Option<Value> {
        if !self.distance.is_finite() || self.distance < 0.0 || self.distance > args.radius_ly {
            return None;
        }
        let arrival = self.distance_to_arrival?;
        if !arrival.is_finite() || arrival < 0.0 || arrival > args.max_arrival_ls.into() {
            return None;
        }
        if args.large_pad && self.has_large_pad != Some(true) {
            return None;
        }
        if !args.include_planetary && self.is_planetary != Some(false) {
            return None;
        }
        if is_carrier(&self.kind)
            && (!args.include_carriers || self.carrier_docking_access.as_deref() != Some("All"))
        {
            return None;
        }
        for (wanted, actual) in [
            (&args.allegiance, &self.allegiance),
            (&args.material_trader, &self.material_trader),
            (&args.technology_broker, &self.technology_broker),
            (&args.government, &self.government),
            (&args.economy, &self.primary_economy),
            (&args.station_type, &Some(self.kind.clone())),
        ] {
            if let Some(wanted) = wanted
                && !actual
                    .as_ref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(wanted))
            {
                return None;
            }
        }
        let services = self
            .services
            .as_ref()
            .map(|values| values.iter().map(|s| &s.name).collect::<Vec<_>>());
        if !args.services.iter().all(|wanted| {
            services
                .as_ref()
                .is_some_and(|all| all.iter().any(|s| s.eq_ignore_ascii_case(wanted)))
        }) {
            return None;
        }
        let timestamp = self
            .updated_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
        let seconds = timestamp.map(|t| now.signed_duration_since(t).num_seconds());
        if let Some(days) = args.max_age_days
            && !seconds.is_some_and(|s| s >= -300 && s <= i64::from(days) * 86400)
        {
            return None;
        }
        Some(
            json!({"system":self.system_name, "station":self.name, "market_id":self.market_id,
            "distance_ly":self.distance, "arrival_ls":arrival, "station_type":self.kind,
            "allegiance":self.allegiance, "controlling_faction":self.controlling_minor_faction,
            "government":self.government, "economy":self.primary_economy, "station_state":self.state,
            "has_large_pad":self.has_large_pad, "is_planetary":self.is_planetary, "services":services,
            "material_trader":self.material_trader, "technology_broker":self.technology_broker,
            "carrier_docking_access":self.carrier_docking_access, "updated_at":self.updated_at,
            "report_age_days":seconds.filter(|s| *s >= -300).map(|s| s.max(0) as f64 / 86400.0),
            "access_verified":false, "station_url":format!("{}/station/{}",ORIGIN,self.market_id)}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::{Path, State},
        http::StatusCode,
        routing::{get, post},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn options() -> ReferenceCache {
        ReferenceCache {
            at: Instant::now(),
            values: BTreeMap::from([
                (
                    "services".into(),
                    vec![
                        "Interstellar Factors Contact".into(),
                        "Material Trader".into(),
                        "Technology Broker".into(),
                        "Repair".into(),
                    ],
                ),
                (
                    "allegiance".into(),
                    vec!["Empire".into(), "Federation".into(), "Alliance".into()],
                ),
                (
                    "material_trader".into(),
                    vec!["Raw".into(), "Encoded".into(), "Manufactured".into()],
                ),
                (
                    "technology_broker".into(),
                    vec!["Human".into(), "Guardian".into()],
                ),
                ("government".into(), vec!["Corporate".into()]),
                (
                    "primary_economy".into(),
                    vec!["High Tech".into(), "Industrial".into()],
                ),
                (
                    "type".into(),
                    vec![
                        "Coriolis Starport".into(),
                        "Outpost".into(),
                        "Drake-Class Carrier".into(),
                    ],
                ),
            ]),
        }
    }
    fn args(value: Value) -> StationSearch {
        serde_json::from_value(value).unwrap()
    }
    fn record(distance: f64) -> Value {
        json!({"name":"Example Port", "system_name":"Example", "market_id":9007199254740993u64,
            "distance":distance, "distance_to_arrival":500.0, "has_large_pad":true, "is_planetary":false,
            "type":"Coriolis Starport", "allegiance":"Empire", "material_trader":"Raw",
            "government":"Corporate", "primary_economy":"High Tech", "controlling_minor_faction":"Example Faction",
            "services":[{"name":"Interstellar Factors Contact"},{"name":"Material Trader"}],
            "updated_at":Utc::now().to_rfc3339()})
    }
    fn matched(value: Value, args: &StationSearch) -> Option<Value> {
        serde_json::from_value::<StationRecord>(value)
            .unwrap()
            .matched(args, Utc::now())
    }
    async fn client(app: Router) -> Spansh {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Spansh::at(&address).unwrap();
        *client.station_references.lock().await = Some(options());
        client
    }

    #[test]
    fn validates_bounds_and_names_before_network() {
        for invalid in [
            json!({"system":""}),
            json!({"system":"Sol","radius_ly":0}),
            json!({"system":"Sol","radius_ly":501}),
            json!({"system":"Sol","max_arrival_ls":0}),
            json!({"system":"Sol","max_age_days":0}),
            json!({"system":"Sol","max_age_days":3651}),
            json!({"system":"Sol","limit":21}),
            json!({"system":"Sol","services":["\n"]}),
            json!({"system":"Sol","services":vec!["Repair";9]}),
        ] {
            assert!(args(invalid).validate().is_err());
        }
        assert!(
            args(json!({"system":"Sol","max_age_days":null}))
                .validate()
                .is_ok()
        );
        assert!(
            serde_json::from_value::<StationSearch>(json!({"system":"Sol","unknown":true}))
                .is_err()
        );
    }

    #[test]
    fn resolves_aliases_and_builds_actual_station_filters_without_market_requirement() {
        let mut query = args(
            json!({"system":" Sol ","allegiance":"empire","services":["interstellar_factors"],"material_trader":"raw"}),
        );
        query.resolve(&options()).unwrap();
        let body = query.body(&options(), Utc::now(), 0);
        assert_eq!(body["reference_system"], "Sol");
        assert_eq!(body["filters"]["allegiance"]["value"], json!(["Empire"]));
        assert_eq!(
            body["filters"]["services"],
            json!([{"name":"Interstellar Factors Contact"},{"name":"Material Trader"}])
        );
        assert_eq!(body["filters"]["material_trader"]["value"], json!(["Raw"]));
        assert_eq!(
            body["filters"]["type"]["value"],
            json!(["Coriolis Starport", "Outpost"])
        );
        assert_eq!(body["sort"][0], json!({"distance":{"direction":"asc"}}));
        assert!(body["filters"].get("market").is_none());
        assert!(body["filters"].get("has_market").is_none());
        assert!(body["filters"].get("updated_at").is_some());
        assert!(matched(record(10.0), &query).is_some());
        assert!(
            args(json!({"system":"Sol","allegiance":"not an allegiance"}))
                .resolve(&options())
                .is_err()
        );
    }

    #[test]
    fn checks_all_services_affiliation_and_subtypes_locally() {
        let mut query = args(
            json!({"system":"Sol","allegiance":"Empire","services":["Interstellar Factors","Material Trader"],"material_trader":"Raw"}),
        );
        query.resolve(&options()).unwrap();
        let found = matched(record(10.0), &query).unwrap();
        assert_eq!(found["market_id"], 9007199254740993u64);
        assert_eq!(found["controlling_faction"], "Example Faction");
        for (field, value) in [
            ("services", json!([{"name":"Material Trader"}])),
            ("allegiance", json!("Federation")),
            ("material_trader", json!("Encoded")),
            ("services", Value::Null),
        ] {
            let mut station = record(10.0);
            station[field] = value;
            assert!(
                matched(station, &query).is_none(),
                "failed to reject {field}"
            );
        }
    }

    #[test]
    fn rejects_unsuitable_docking_distances_and_reports_and_labels_unknown_access() {
        let query = args(json!({"system":"Sol"}));
        for (field, value) in [
            ("has_large_pad", json!(false)),
            ("has_large_pad", Value::Null),
            ("is_planetary", json!(true)),
            ("type", json!("Drake-Class Carrier")),
            ("distance_to_arrival", json!(10001.0)),
            ("distance", json!(-1)),
            ("distance", json!(51)),
            ("distance_to_arrival", Value::Null),
            ("updated_at", Value::Null),
            (
                "updated_at",
                json!((Utc::now() - chrono::Duration::days(31)).to_rfc3339()),
            ),
            (
                "updated_at",
                json!((Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
            ),
        ] {
            let mut station = record(10.0);
            station[field] = value;
            assert!(
                matched(station, &query).is_none(),
                "failed to reject {field}"
            );
        }
        let mut station = record(10.0);
        station["type"] = json!("Drake-Class Carrier");
        let query = args(json!({"system":"Sol","include_carriers":true}));
        assert!(matched(station.clone(), &query).is_none());
        station["carrier_docking_access"] = json!("All");
        assert_eq!(matched(station, &query).unwrap()["access_verified"], false);
        let mut station = record(10.0);
        station["updated_at"] = Value::Null;
        let result = matched(station, &args(json!({"system":"Sol","max_age_days":null}))).unwrap();
        assert!(result["report_age_days"].is_null());
    }

    #[tokio::test]
    async fn caches_real_reference_shapes_and_filters_options() {
        let reads = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/api/stations/field_values/{field}",
                get(
                    |Path(field): Path<String>, State(reads): State<Arc<AtomicUsize>>| async move {
                        reads.fetch_add(1, Ordering::SeqCst);
                        let values = options().values.remove(&field).unwrap();
                        axum::Json(if field == "services" {
                            json!({"values":{"name":values}})
                        } else {
                            json!({"values":values})
                        })
                    },
                ),
            )
            .with_state(reads.clone());
        let client = client(app).await;
        *client.station_references.lock().await = None;
        let result = client
            .station_options(StationOptions {
                query: "factors".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            result["filters"]["services"],
            json!(["Interstellar Factors Contact"])
        );
        assert_eq!(result["filters"]["allegiance"], json!([]));
        client
            .station_options(StationOptions::default())
            .await
            .unwrap();
        let result = client
            .station_options(StationOptions {
                query: "material trader".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            result["filters"]["material_trader"],
            json!(["Raw", "Encoded", "Manufactured"])
        );
        assert!(result["filters"].get("economy").is_some());
        assert!(result["filters"].get("station_type").is_some());
        assert_eq!(reads.load(Ordering::SeqCst), FIELDS.len());
    }

    #[tokio::test]
    async fn paginates_past_unverified_candidates_and_returns_nearest_verified_results() {
        let calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new().route("/api/stations/search", post(
            |State(calls):State<Arc<AtomicUsize>>, axum::Json(body):axum::Json<Value>| async move {
                calls.fetch_add(1,Ordering::SeqCst);
                assert_eq!(body["filters"]["allegiance"]["value"], json!(["Empire"]));
                assert_eq!(body["filters"]["services"], json!([{"name":"Interstellar Factors Contact"},{"name":"Material Trader"}]));
                let results = if body["page"] == 0 {
                    (1..=20).map(|n| { let mut r = record(n as f64); r["services"] = json!([{"name":"Material Trader"}]); r }).collect::<Vec<_>>()
                } else { vec![record(25.0)] };
                axum::Json(json!({"reference":{"name":"Sol"},"count":21,"results":results}))
            }
        )).with_state(calls.clone());
        let result = client(app).await.stations(args(json!({"system":"Sol","allegiance":"Empire","services":["Interstellar Factors","Material Trader"],"limit":1}))).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(result["stations"][0]["distance_ly"], 25.0);
        assert_eq!(result["stations"].as_array().unwrap().len(), 1);
        assert_eq!(result["candidates_checked"], 21);
        assert_eq!(result["search_complete"], true);
    }

    #[tokio::test]
    async fn caps_scanning_and_reports_partial_search_instead_of_claiming_no_stations_exist() {
        let app = Router::new().route(
            "/api/stations/search",
            post(|axum::Json(body): axum::Json<Value>| async move {
                let page = body["page"].as_u64().unwrap();
                let results: Vec<_> = (1..=20)
                    .map(|n| {
                        let mut r = record((page * 20 + n) as f64);
                        r["allegiance"] = json!("Federation");
                        r
                    })
                    .collect();
                axum::Json(json!({"reference":{"name":"Sol"},"count":1000,"results":results}))
            }),
        );
        let result = client(app)
            .await
            .stations(args(
                json!({"system":"Sol","allegiance":"Empire","radius_ly":500}),
            ))
            .await
            .unwrap();
        assert_eq!(result["search_complete"], false);
        assert_eq!(result["candidates_checked"], 100);
        assert_eq!(result["stations"], json!([]));
    }

    #[tokio::test]
    async fn rejects_reference_mismatch_bad_order_and_unknown_filter() {
        for (reference, results) in [
            ("Elsewhere", vec![]),
            ("Sol", vec![record(20.0), record(10.0)]),
        ] {
            let app = Router::new().route("/api/stations/search", post(move || {
                let results = results.clone();
                async move { axum::Json(json!({"reference":{"name":reference},"count":results.len(),"results":results})) }
            }));
            assert!(
                client(app)
                    .await
                    .stations(args(json!({"system":"Sol"})))
                    .await
                    .is_err()
            );
        }
        let client = client(Router::new()).await;
        assert!(
            client
                .stations(args(json!({"system":"Sol","material_trader":"Wrong"})))
                .await
                .unwrap_err()
                .to_string()
                .contains("unknown material_trader")
        );
    }

    #[tokio::test]
    async fn distinguishes_empty_results_from_provider_failure() {
        let app = Router::new().route(
            "/api/stations/search",
            post(|| async {
                axum::Json(json!({"reference":{"name":"Sol"},"count":0,"results":[]}))
            }),
        );
        let result = client(app)
            .await
            .stations(args(json!({"system":"Sol"})))
            .await
            .unwrap();
        assert_eq!(result["stations"], json!([]));
        assert_eq!(result["search_complete"], true);
        let app = Router::new().route(
            "/api/stations/search",
            post(|| async { StatusCode::TOO_MANY_REQUESTS }),
        );
        assert!(
            client(app)
                .await
                .stations(args(json!({"system":"Sol"})))
                .await
                .unwrap_err()
                .to_string()
                .contains("rate limit")
        );
        let app = Router::new().route("/api/stations/search", post(|| async { "invalid JSON" }));
        assert!(
            client(app)
                .await
                .stations(args(json!({"system":"Sol"})))
                .await
                .unwrap_err()
                .to_string()
                .contains("format changed")
        );
    }
}

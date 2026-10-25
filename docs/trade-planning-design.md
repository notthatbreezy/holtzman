# Trade planning: integration findings and proposed first version

Investigated October 23, 2026. The historical research and initial proposal
are retained below. On October 25 the commander requested a simple MCP
integration, implemented in Holtzman 0.3.0; see
[the current tools](trade-planning.md) for actual behavior and limits.
No game input was sent during the integration work.

## Earlier decision: try native web planning first (superseded October 25)

Defer a dedicated trade-planning MCP integration. First use Codex/ChatGPT web
or browser access for trade research, with Holtzman supplying local ship/game
state and handling only separately requested, supervised route plotting.
This can establish whether a new server feature is needed at all.

Reconsider the integration below if repeated use shows a need for structured
results, consistent filters, bounded API jobs, or caching. No trading code,
new MCP tool, account integration or live-state export was implemented.

To resume, confirm the commander's current location, available cargo, budget
and trading objective (best-profit itinerary versus sourcing a named commodity
for a fixed destination). Confirm consent for any actual game-state details
sent to an external planner. Then try one planning request through the native
web/browser path and evaluate the result before choosing implementation work.

## Provider findings

### Spansh: feasible for the first integration

The public [trade planner](https://spansh.co.uk/trade) uses:

- `POST https://spansh.co.uk/api/trade/route`, with form-encoded parameters.
- `GET https://spansh.co.uk/api/results/{job}`, to poll a queued calculation.

These paths and parameters were inspected in Spansh's own deployed client
JavaScript, then verified with two synthetic requests unrelated to the
commander's actual location, cargo or credits. No login or API key was needed
for those requests. This is an observed website interface; a documented,
stable public API contract or published rate-limit agreement was not found.

The first synthetic request completed with an empty result under restrictive
settings. A broader Lave Station example completed and returned two trade
legs, including commodities, quantities, source/destination buy/sell prices,
supply/demand, per-tonne and total profit, station arrival distance, and market
update timestamps. These were integration examples, not current trading advice.

Observed request parameters:

| Parameter | Meaning |
| --- | --- |
| `system`, `station` | Starting market |
| `max_cargo` | Cargo space to use |
| `starting_capital` | Budget available for purchasing commodities |
| `max_hops` | Number of trade legs to consider |
| `max_hop_distance` | Maximum separation between trade-leg systems, not ship jump range |
| `max_system_distance` | Maximum station distance from arrival |
| `max_price_age` | Maximum market age in seconds |
| `requires_large_pad` | Large-pad requirement |
| `allow_prohibited` | Allow prohibited goods |
| `allow_planetary` | Include planetary markets |
| `allow_player_owned` | Include player-owned markets |
| `allow_restricted_access` | Include restricted-access markets |
| `unique`, `permit` | Additional route/permit flags exposed by the site |

The client posts booleans as `0`/`1` and receives `status = "queued"` plus a
job identifier. Result polling returned `status = "ok"`, `state = "completed"`
and a `result` array. Keep system addresses and market identifiers as integers;
do not round large identifiers through floating-point conversion.

### Inara: useful website, unsuitable market API for this version

Inara has a [public API](https://inara.cz/elite/inara-api/) and
[event documentation](https://inara.cz/elite/inara-api-docs/), but it does not
provide general commodity-market search output. The
[API discussion](https://inara.cz/elite/board-thread/7049/480676/?page=40)
includes a trade-tool author confirming that limitation after the maintainer
questioned the proposed market events. The maintainer also directs developers
toward EDDN for general galaxy and market information in the
[API discussion](https://inara.cz/elite/board-thread/7049/492244/).

Do not assume that an Inara API key unlocks its website's market search, or
invent `getCommodity`/`getStationMarket` API events. Direct links to Inara
can still help a commander cross-check a suggestion.

### EDDN: later option for owning the market index

Inara's maintainer identifies EDDN as a major shared data source. Consuming a
feed and maintaining a market index would be a separate backend undertaking;
it is not necessary for the first Spansh-backed planner. Its schema, operation
and data licensing would need their own investigation before implementation.

## Original proposal: first MCP tool if needed

`plan_trade_route` should return advice only: source/destination stations,
commodities, quantities, purchase cost, estimated profit, market ages and
constraints used. It must not press controls, plot a route automatically,
buy/sell goods, or begin travel.

Use existing journal readers for current location and available cargo space
only when the user has opted into using those details for external planning.
Use an explicit trading budget rather than silently treating all credits as
spendable capital. Accept an explicit start station and cargo/budget overrides
so a caller can plan without exporting live commander state.

Proposed defaults: require a large pad for a Type-9; exclude prohibited goods,
permit/restricted destinations, player-owned and planetary markets initially;
apply a bounded market age and station-arrival distance. Make clear that a
trade leg may require multiple hyperspace jumps. Recheck availability and
price age at presentation time; do not describe estimated profit as guaranteed.

Hide the provider's asynchronous jobs behind a bounded timeout and capped
polling backoff. Distinguish an empty result, invalid starting market, provider
error, rate limit, timeout and malformed response. Do not resubmit a calculation
on every poll. Cache identical requests briefly.

## Implementation and validation outline

1. Add a provider module with typed request/result structures and a shared
   HTTPS client, fixed Spansh origin, timeouts and bounded polling.
2. Add a read-only planning MCP tool, with explicit outbound-data disclosure
   and user consent before sending live game-state defaults. Never send the
   Holtzman bearer token, commander identity, raw config or journal files.
3. Present compact trade legs and freshness information, with enough raw
   quantities/prices to audit profit calculations. Keep route plotting as a
   separate, explicitly requested supervised action.
4. Test parsing, empty/error results, timeout/backoff, numeric identifiers,
   request constraints and mock HTTP interactions. Use sanitized synthetic
   fixtures; no game input or live commander data in automated tests.
5. Run the repository build, formatting, strict lint and test checks. Perform
   one opt-in live planning request after the outbound-data consent is clear.

Commodity sourcing to a fixed destination (for example supplying a community
goal) is a useful follow-up. The site also exposes commodity and station-search
interfaces, but those have not been exercised here; do not promise fixed-
destination or commodity-filter behavior from the generic route endpoint.

## Investigation data boundary

A proposed live-state probe was rejected by automatic approval because the
initial authorization to investigate APIs did not specifically authorize
exporting actual location/cargo planning details. That request was not sent.
The two completed probes used synthetic settings only. Permission to use live
state for requested trade plans remains to be resolved before enabling it.

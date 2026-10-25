# Spansh trade planning

Holtzman 0.3.0 adds three read-only MCP tools for conversations such as:

> I'm around Sol in a large ship. Where could I buy 100 tonnes of uranium?
> Which nearby stations will buy it, and how old are those prices?
> Starting at Lave Station, plan two trade legs for 100 empty tonnes and a
> one-million-credit purchasing budget.

The agent supplies the search arguments, discusses results, and can adjust
radius, price age, station distance and landing-pad requirements. A system
alone is enough for commodity searches; the itinerary optimizer also needs
the starting **station**. An itinerary is a list of trades, not a hyperspace
route or a promise of profit.

## Tools

### `trade_commodities`

Find exact commodity names by case-insensitive substring, for example
`{"query":"uran"}`. Returns at most 50 names and the total matching count.
This searches tradable commodity names, not engineering-material inventory.

### `find_commodity_market`

```json
{
  "system": "Sol",
  "commodity": "Uranium",
  "action": "buy",
  "amount": 100,
  "radius_ly": 50,
  "max_arrival_ls": 10000,
  "max_age_hours": 24,
  "large_pad": true,
  "include_planetary": false,
  "include_carriers": false,
  "limit": 10
}
```

`buy` means **the commander buys from the station**: lowest buy price first,
with at least `amount` tonnes of reported supply. `sell` means **the commander
sells to the station**: highest sell price first, with at least `amount` tonnes
of reported demand. Distance breaks price ties. Searches use up to 20
provider candidates and return at most `limit` eligible offers; omitted carriers
or unknown-age candidates can mean fewer results than requested.

Each offer includes system, station, market ID, price per tonne, value for the
requested amount, supply, demand, arrival distance, reference-system distance,
market timestamp, age in hours and a station link. Prohibited commodities,
stale/unknown-age reports, mismatching quantities and unsuitable pad/distance
results are omitted. When carriers are included, only those reporting public
docking access are eligible. Permit access and any other local restrictions
still need checking; a reported market is not permission to dock or trade.

Only `system`, `commodity` and `action` are required. Defaults are shown above
except `amount`, which defaults to 1. Radius may be up to 500 ly, market age up
to 720 hours, arrival distance up to 1,000,000 ls, amount up to 10,000 tonnes,
and result limit up to 20. No results is a valid outcome; widening a search is
a conversation decision, not an automatic relaxation of the user's limits.

### `plan_trade_route`

```json
{
  "system": "Lave",
  "station": "Lave Station",
  "cargo_tonnes": 100,
  "budget_credits": 1000000,
  "max_legs": 2,
  "max_leg_ly": 50,
  "max_arrival_ls": 10000,
  "max_age_hours": 24,
  "large_pad": true,
  "include_planetary": false,
  "include_carriers": false
}
```

System, station, cargo space and budget are required. Cargo space is the
**empty space allocated to new purchases**, not total capacity when already
loaded. Budget is explicitly available purchasing capital; the agent should
ask rather than assume the entire account balance is spendable.

Other fields default as shown. `max_legs` may be 1..6. A trade leg can require
several hyperspace jumps: `max_leg_ly` is not the ship's jump range. Cargo space
is limited to 10,000 tonnes and budget to one trillion credits; distance/age
bounds match commodity search. Prohibited goods, restricted access and permit
destinations are disabled in the provider request.

The provider queues a calculation; Holtzman submits once and polls with
capped backoff for at most 45 seconds. It returns trade legs with source and
destination stations, commodities, tonnes, buy/sell quotes, supply/demand,
per-tonne profit, leg/cumulative profit, distances and market ages. It checks
the starting station, itinerary continuity, reported prices/profit arithmetic,
cargo/budget bounds, data age and arrival distance. Budget available for later
legs includes earlier estimated profit.

A completed empty itinerary means the provider found no matching trades.
Timeout, rate limit, HTTP failure and changed response schema are distinct
errors. The optimizer chooses commodities; it does not implement a fixed-
commodity delivery mission. For that, use `find_commodity_market` to compare
sources near the desired destination and buyers near a source.

## External data and game controls

The tools contact only Spansh's fixed HTTPS origin. Searches send only explicit
tool arguments (the named reference/start location and planning constraints).
They do not read the live journal to fill in or export location, cargo or
credit defaults. An agent can discuss local `status`/`cargo` results with the
commander separately and use relevant values when the commander requests that
planning query. No commander identity, Holtzman authentication, raw config or
journal files are forwarded. The HTTP client follows no redirects.

Planning does not focus the game, press keys, move the mouse, plot a course,
or buy/sell cargo. `plot_route` remains a separate supervised action explicitly
requested by the commander. No game configuration or HOTAS bindings change.

## Provider limits

This uses the public website interface observed at [Spansh](https://spansh.co.uk/trade),
not a guaranteed versioned/documented API contract. No login or API key was
required by the synthetic integration probes. Commodity names and station
types are cached for one hour; at most two planning requests run concurrently.
HTTP requests have bounded timeouts and responses have a size limit.

Reported supply and demand can change while travelling, and prices can be
depleted or stale even within the selected age limit. Fleet carriers can move
or alter access. Check actual prices, stock, access and legality in-game before
acting on a suggestion. An integration failure never falls back to game input.

Inara's public API does not expose general market searches; the
[research note](trade-planning-design.md) records that finding and the Spansh
endpoint details. A separate market index/feed is not part of this version.

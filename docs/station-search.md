# Nearest station search

`find_stations` answers requests such as “closest Empire station”, “nearest
Interstellar Factors”, and “where is a Raw material trader?”. It searches the
station index behind [Spansh's station search](https://spansh.co.uk/stations).
No Inara account or API key is needed. Results are advice; the tools never
plot routes, move the ship, or send game input.

## Examples

Find nearby Empire-affiliated stations, nearest first:

```json
{"system":"Sol","allegiance":"Empire","limit":3}
```

Find Interstellar Factors (the shorter name is accepted as an alias):

```json
{"system":"Sol","services":["Interstellar Factors"],"limit":3}
```

Find a Raw material trader; `material_trader` also requires Material Trader:

```json
{"system":"Sol","material_trader":"Raw","limit":3}
```

Manufactured and Encoded traders work the same way. To find any trader:

```json
{"system":"Sol","services":["Material Trader"]}
```

Combine affiliation and **all** required services:

```json
{
  "system":"Sol",
  "allegiance":"Federation",
  "services":["Interstellar Factors", "Repair"],
  "radius_ly":100,
  "large_pad":false,
  "include_planetary":true
}
```

The optional filters `technology_broker`, `government`, `economy` (station
primary economy), and `station_type` accept names from `station_search_options`.
A technology-broker subtype also requires the Technology Broker service.

`station_search_options` with `{}` lists canonical names grouped by filter.
`{"query":"trader"}` or `{"query":"factors"}` narrows the list by filter name
or supported value. Filter values
are case-insensitive; underscores/hyphens can replace spaces. A misspelt or
unsupported value returns an error rather than silently broadening the query.

## Defaults and bounds

Only `system` is required. It is the explicit reference system sent to Spansh;
there is no automatic journal/location export. A client can use current
instrument readings when the Captain asks for a nearby search.

| Argument | Default | Bounds / meaning |
|---|---|---|
| `radius_ly` | 50 | Greater than 0, at most 500 ly |
| `max_arrival_ls` | 10000 | 1..1,000,000 light-seconds |
| `large_pad` | true | False also allows smaller or unknown pads |
| `include_planetary` | false | Include reported planetary stations |
| `include_carriers` | false | Included carriers must report public (`All`) docking |
| `max_age_days` | 30 | 1..3650 days, or `null` to accept unknown/older station reports |
| `limit` | 10 | 1..20 nearest eligible stations |
| `services` | [] | At most eight explicitly requested services; all must match |

The report-age filter uses **station** `updated_at`, not commodity market age.
Requested affiliation, services/subtypes, pad, distance, planetary/carrier and
report-age constraints are checked again locally against each result. Unknown
required values are omitted. Constraints are never relaxed automatically.

Results rank by straight-line distance in light-years, with station arrival
distance as a tie-breaker. This is not travel time, jump count, or a plotted
hyperspace route. Results include station/system, market ID and station link,
arrival distance, allegiance and controlling faction, facilities, trader/broker
subtypes, station state, and report timestamp/age.

Station **allegiance** is the station's reported affiliation, not the system's
allegiance, a faction's presence elsewhere in the system, or Powerplay control.
Missing permit/docking restrictions remain **unverified** (`access_verified`
is false). A reported facility does not guarantee it is currently available
to the commander; check station state, access and in-game contacts before travel.

## Provider and scan limits

This uses the public website interface verified on 2026-11-01, not a guaranteed
versioned API. Verified requests used synthetic searches around Sol for station
allegiance, Interstellar Factors and Raw material traders. No commander identity,
bearer token or raw journal/configuration is sent. HTTPS requests use the existing
fixed Spansh origin, response-size limits, no redirects, bounded timeouts, and
shared maximum of two concurrent planning operations. Reference lists cache for
one hour; station results are fetched for each search.

The search scans at most five distance-sorted pages of 20 candidates to compensate
for omitted/unverified results. Duplicate market IDs are removed. If the scan
limit is reached before enough matching stations are found, `search_complete`
is false. That means the scan is incomplete, not that no matching station exists.
`provider_matches` and `candidates_checked` make this distinction visible.
Unexpected reference systems, oversized pages or unordered distance results
fail rather than returning a misleading nearest-station ranking. Empty results,
rate limits, malformed responses and timeouts are distinct outcomes.

After updating the server, reconnect/refresh the HOLT MCP connection in ST to
discover `find_stations` and `station_search_options`.

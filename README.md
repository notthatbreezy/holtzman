# holtzman

*The Holtzman effect folds space. This folds the tedium out of the galaxy map.*

An [MCP](https://modelcontextprotocol.io) server that turns an AI assistant
into a ship computer for **Elite Dangerous**. It runs on your gaming PC, reads
the game's live journal, and can drive a few tedious UI chores, starting with
plotting a route, by pressing your own key bindings.

```
 laptop / phone                      gaming PC
┌──────────────┐   HTTP + bearer   ┌──────────────────────────────┐
│ Claude (MCP  │ ────────────────▶ │ holtzman serve               │
│   client)    │    LAN :7777      │  ├─ reads Journal.*.log,     │
└──────────────┘                   │  │  Status/Market/Cargo/     │
                                   │  │  NavRoute.json            │
                                   │  └─ SendInput scan codes ──▶ │ Elite Dangerous
                                   └──────────────────────────────┘
```

## Tools

| Tool | What it does |
|---|---|
| `status` | Location, docked station, ship, cargo capacity, credits, fuel, status flags, open panel, plotted destination |
| `market` | Last-opened commodity market (buy/sell/stock/demand), optional name filter |
| `cargo` | Hold contents |
| `nav_route` | Current plotted route |
| `recent_events` | Latest journal events (noise filtered), optionally by type |
| `plot_route` | Galaxy map → search → plot → close, then **verifies** the destination in `NavRoute.json` |
| `screenshot` | Picture of the game window (never the desktop) plus open panel and status flags; crop by named region or rect |
| `press_binding` | Press one allow-listed binding (for stepping through menus / tuning recipes); `observe: true` returns a screenshot of the result and how much of the screen changed |
| `nav_target_start` / `_move` / `_confirm` / `_cancel` | Lock or unlock a station in the current system, or toggle supercruise assist to it, via the left panel's Navigation list (supervised, step by step) |
| `cursor_position` | Mouse position relative to the game window |
| `record_click_target` | Save the hovered position as a named click target in `holtzman.toml` |
| `trade_commodities` | Look up Spansh commodity names |
| `find_commodity_market` | Find nearby stations buying or selling a commodity, with prices and market ages |
| `plan_trade_route` | Get a short Spansh trade itinerary for a specified station, cargo space and budget |

holtzman also serves one MCP prompt, `ship_computer`: the personality and
rules of engagement for HOLT, the ship's droid
([prompts/ship-computer.md](prompts/ship-computer.md)). Harnesses can load it
as their system prompt. To rename or rewrite your droid, point `ship_prompt`
in `holtzman.toml` at your own Markdown file (relative to the config); it is
re-read on every request.

Read tools are side-effect free. Action tools take the keyboard for a few
seconds, so the server's instructions tell the model to announce them first.

## Setup (gaming PC)

1. Install Rust (<https://rustup.rs>), then:
   ```powershell
   cargo install --git https://github.com/notthatbreezy/holtzman
   mkdir $HOME\holtzman; cd $HOME\holtzman
   holtzman init          # writes holtzman.toml with a random token
   ```
2. Edit `holtzman.toml`:
   - add this PC's LAN IP (and name) to `allowed_hosts`
   - set `journal_dir` / `bindings_dir` only if they aren't in the default places
3. **Keyboard bindings.** holtzman can only press keyboard keys. If your
   HOTAS owns `GalaxyMapOpen`, `UI_Up`, `UI_Select`, etc., add keyboard keys as
   the *Secondary* binding in Options → Controls. **The UI Up/Down/Left/Right
   keys must not share keys with galaxy-map camera panning** (WASD by default);
   arrow keys work well. Check with:
   ```powershell
   holtzman bindings
   ```
4. Allow the port through Windows Firewall (Private networks only):
   ```powershell
   New-NetFirewallRule -DisplayName holtzman -Direction Inbound -Protocol TCP -LocalPort 7777 -Profile Private -Action Allow
   ```
5. Sanity-check, then serve:
   ```powershell
   holtzman status        # should print your commander and location
   holtzman serve
   ```

## Connect a client (laptop)

Until holtzman grows its own stdio bridge, use
[`mcp-remote`](https://www.npmjs.com/package/mcp-remote) to connect Claude
Desktop to the LAN server. In `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "holtzman": {
      "command": "npx",
      "args": [
        "-y", "mcp-remote", "http://skippy:7777/mcp",
        "--allow-http",
        "--header", "Authorization:${HOLTZMAN_AUTH}"
      ],
      "env": { "HOLTZMAN_AUTH": "Bearer <token from holtzman.toml>" }
    }
  }
}
```

## Calibrating the Plot Route button

The recipe uses a recorded mouse click for Plot Route. A supervised Odyssey
test succeeded with this approach; the keyboard hold prompt did not produce
a route in that test. Record the button's position once (stored as fractions of the game window,
so it survives resolution changes as long as the UI layout is the same):

1. Open the galaxy map, search for any system, and let its info panel open.
2. Hover the mouse over **Plot Route**.
3. Ask the assistant to run `record_click_target` with name
   `plot_route_button` (or run `holtzman cursor` and add it by hand):
   ```toml
   [click_targets.plot_route_button]
   x = 0.82
   y = 0.31
   ```

Upgrading from 0.1? `holtzman reset-recipe` swaps in the new default recipe
and keeps your token and targets.

For a complete recipe verified in-game, including search-result selection,
see [Validated Odyssey route recipe](docs/validated-odyssey-recipe.md).
`reset-recipe` restores the built-in default, not this calibrated recipe.

## Screenshots

The journal says which panel is open, not which tab or row is highlighted.
`screenshot` fills that gap: it asks the game window to render itself into
an off-screen bitmap, so other windows never appear in the image and the
game doesn't need focus. Use borderless or windowed mode; exclusive
fullscreen can come back black.

Images are downscaled so the longest side is 1568 px (JPEG by default).
Crop with `region` (`full`, `center`, `left`, `right`, `top`, `bottom`) or a
`rect` in window fractions to read small HUD text. Set defaults and add your
own named regions in `holtzman.toml`:

```toml
[screenshot]
max_edge = 1024        # smaller for local vision models
format = "jpeg"        # or "png"
jpeg_quality = 85

[screen_regions.station_menu]
x = 0.43
y = 0.6
w = 0.14
h = 0.3
```

Check capture from the command line with `holtzman screenshot out.png`.

## Locking a station in the current system

holtzman can't read the Navigation list, so the assistant reads it from
screenshots while the server presses keys and runs the checks:

1. `nav_target_start(station, action)` refuses if a panel is open, the ship
   is in danger, or the lock is already as asked (Status.json
   `Destination`). Otherwise it opens the left panel and returns the list.
2. `nav_target_move(session, rows, tabs, expect)` moves the highlight and
   returns the list again. `expect` names what the assistant sees
   highlighted afterwards; confirm is refused until it is the station.
3. `nav_target_confirm(session)` presses the lock or supercruise assist
   button, checks the new `Destination` (lock and unlock), closes the panel,
   and for supercruise assist returns a HUD screenshot to check.

The keys and timings live in `[nav_target]` in `holtzman.toml`: the open
and close recipes, the lock and assist button recipes, the row and tab
bindings, and `list_region` (where the list is, for the screenshots). The
defaults are uncalibrated guesses; tune them in-game.

## Tuning `plot_route`

The recipe lives in `holtzman.toml` as a list of steps:

```toml
[[plot_route.steps]]
do = "binding"            # press whatever key the game binds to an action
name = "GalaxyMapOpen"

[[plot_route.steps]]
do = "wait_for_gui"       # poll Status.json until the galaxy map is open
focus = 6
timeout_ms = 5000

[[plot_route.steps]]
do = "paste_target"       # clipboard + Ctrl+V

[[plot_route.steps]]
do = "key"                # raw key by Elite name
key = "Key_Enter"

[[plot_route.steps]]
do = "click"              # left-click a recorded [click_targets] position
target = "plot_route_button"

[[plot_route.steps]]
do = "wait"
ms = 800
```

The default recipe is a starting point for the Odyssey galaxy map; menus
shift between game updates. To tune: `holtzman plot "Sol" --dry-run` logs the
keys, `holtzman plot "Sol"` runs it for real (3 s to alt-tab), and the
`press_binding` tool lets an assistant walk the menu one key at a time while
you watch.

Requirements: the game window must exist, no panel open, and holtzman must
run at the same privilege level as the game (Windows blocks input injection
into elevated windows from non-elevated processes).

## Trade planning with Spansh

Ask your agent to find nearby sellers or buyers, compare opportunities, or
plan a short trade itinerary. The planning tools send explicit search
parameters to Spansh and return advice, without controlling the game. They do
not automatically read/export your commander profile, spend credits, buy/sell
cargo, or plot a route. Your Holtzman token is never sent to Spansh.

No Spansh account or API key was needed in testing. The integration uses the
interface behind Spansh's public website, rather than a guaranteed versioned
API, so provider changes may require updates. Prices are community reports;
every result includes freshness information and constraints still matter.

See [Trade-planning tools and conversation examples](docs/trade-planning.md)
for arguments, defaults, outbound data, and limitations.

## Fair play

Frontier allows macros and voice-command tools; unattended automated play is
botting. holtzman is designed to keep the commander in the loop. Every action
is a single, short, supervised UI chore. Please keep it that way.

## Roadmap

- `holtzman bridge`: built-in stdio↔HTTP proxy (drop the `npx` dependency)
- Spansh follow-ups: neutron plotting and broader station search
- Journal watch → MCP notifications (docked, market opened, interdicted)
- More recipes: target next system, request docking

See [Trade-planning research and deferred integration design](docs/trade-planning-design.md)
for tested Spansh interfaces, Inara API limits, and the original design history.

## Development

```sh
cargo test       # unit + end-to-end HTTP tests against fixture journals
cargo clippy --all-targets
```

Off Windows, keypresses are logged instead of sent, so everything except the
actual injection is testable anywhere.

## License

MIT

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
| `press_binding` | Press one allow-listed binding (for stepping through menus / tuning recipes) |

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
   the *Secondary* binding in Options → Controls. Check with:
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
do = "binding"
name = "UI_Select"
hold_ms = 1200            # hold-to-confirm

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

## Fair play

Frontier allows macros and voice-command tools; unattended automated play is
botting. holtzman is designed to keep the commander in the loop. Every action
is a single, short, supervised UI chore. Please keep it that way.

## Roadmap

- `holtzman bridge`: built-in stdio↔HTTP proxy (drop the `npx` dependency)
- Spansh tools: trade routes, neutron plotting, station search
- Journal watch → MCP notifications (docked, market opened, interdicted)
- More recipes: target next system, request docking

## Development

```sh
cargo test       # unit + end-to-end HTTP tests against fixture journals
cargo clippy --all-targets
```

Off Windows, keypresses are logged instead of sent, so everything except the
actual injection is testable anywhere.

## License

MIT

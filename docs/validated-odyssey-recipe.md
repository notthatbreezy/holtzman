# Validated Odyssey route recipe

This recipe completed a supervised end-to-end test on Windows with Elite
Dangerous Odyssey on October 23, 2026, using Holtzman 0.2.0 at `e84da1d` and
Codex as the native HTTP MCP client. It created a fresh two-jump route from
AD Leonis to Ross 248, verified the destination in `NavRoute.json`, and closed
the galaxy map. The commander remained docked throughout.

A second supervised run later that session plotted the return trip from
Ross 248 to AD Leonis with 776 tonnes aboard: three jumps, a fresh matching
`NavRoute.json`, and the map closed. The commander confirmed the result.

This records one calibrated setup tested in both directions, not a guarantee for every UI
layout or destination. No Rust changes were needed. The built-in default
recipe has not been changed to match it.

## Bindings and calibration

The tested keyboard bindings were:

| Binding | Key |
| --- | --- |
| `GalaxyMapOpen` | Shift+M |
| `UI_Up` / `UI_Down` | Up / Down Arrow |
| `UI_Left` / `UI_Right` | Left / Right Arrow |
| `UI_Select` | Space |

Keep UI navigation separate from galaxy-map camera panning (WASD). Preserve
existing joystick/HOTAS bindings; use secondary keyboard bindings if needed.

Record `plot_route_button` for your own layout: open a system's information
panel, hover Plot Route, and call `record_click_target` with
`{"name":"plot_route_button"}`. Keep the game focused and the cursor still
while recording. `holtzman cursor` only reports coordinates; it does not save
a target. The tested client area was 3440x1440; do not copy another setup's
coordinates without calibration.

## Applying the recipe

Back up `holtzman.toml` first. Replace only its `[plot_route]` section and
`[[plot_route.steps]]` entries with the following. Keep authentication,
allowed hosts, paths, keyboard timing, the pressable allow-list, and recorded
click targets intact. Never publish the full config: it contains the token.

The tested keyboard `tap_ms` was 80. The mouse click has its own explicit
1200 ms hold, so keyboard timing does not need to change. Restart the server
after editing its recipe; the running server keeps its config in memory.
`reset-recipe` restores the built-in default and overwrites these recipe edits.

```toml
[plot_route]
verify_timeout_ms = 8000

[[plot_route.steps]]
do = "binding"
name = "GalaxyMapOpen"

[[plot_route.steps]]
do = "wait_for_gui"
focus = 6
timeout_ms = 5000

[[plot_route.steps]]
do = "wait"
ms = 1500

[[plot_route.steps]]
do = "binding"
name = "UI_Up"

[[plot_route.steps]]
do = "wait"
ms = 200

[[plot_route.steps]]
do = "binding"
name = "UI_Select"

[[plot_route.steps]]
do = "wait"
ms = 400

[[plot_route.steps]]
do = "paste_target"

[[plot_route.steps]]
do = "wait"
ms = 300

[[plot_route.steps]]
do = "key"
key = "Key_Enter"

[[plot_route.steps]]
do = "wait"
ms = 200

[[plot_route.steps]]
do = "binding"
name = "UI_Select"

[[plot_route.steps]]
do = "wait"
ms = 200

[[plot_route.steps]]
do = "binding"
name = "UI_Down"

[[plot_route.steps]]
do = "wait"
ms = 200

[[plot_route.steps]]
do = "binding"
name = "UI_Select"

[[plot_route.steps]]
do = "wait"
ms = 3000

[[plot_route.steps]]
do = "click"
target = "plot_route_button"
hold_ms = 1200

[[plot_route.steps]]
do = "wait"
ms = 1000

[[plot_route.steps]]
do = "binding"
name = "GalaxyMapOpen"
```

## What the test established

Pasting worked, but Enter alone left one matching result without moving the
camera. Select then returned to text editing. Down Arrow highlighted the
matching result, and Select activated it and opened the system panel. The
recipe above retains that exact successful sequence; a shorter sequence has
not been validated.

The three-second camera wait was sufficient in the successful run. A full
keyboard attempt using a 1200 ms Select hold for Plot Route did not create a
route. Changing only that final plotting action to the recorded 1200 ms mouse
click succeeded. Earlier failures also lacked the working search-result
selection, so increasing the mouse hold alone was not proven to fix them.

## Supervised verification

Before each live test, confirm the commander's current state and start from
the cockpit with no panel open (`GuiFocus = 0`). Announce the action and keep
the commander watching. Run one `plot_route` call with the exact system name;
do not automatically retry failed live actions.

For a clean validation, confirm there is no existing route first. Success
requires a newly updated `NavRoute.json` ending at the requested destination,
not just a cursor move or a matching search field. Also confirm that the map
closed (`GuiFocus = 0`).

If a test fails, stop and locate the failed stage with the commander: search
text, highlighted result, system panel, plotting action, or route confirmation.
A temporary recipe ending before the plotting action will still time out its
route check; that is expected and does not by itself identify an input failure.
Restore the complete recipe after such diagnostics.

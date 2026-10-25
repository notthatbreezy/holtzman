# Supervised docking-request calibration

On October 23, 2026, the commander approached Homer Platform in AD Leonis
and explicitly authorized one-key-at-a-time calibration through native
Holtzman MCP controls. This note records the observations; a dedicated docking
recipe/tool has not been implemented.

## Observed sequence

| Action | Observed state afterward |
| --- | --- |
| `FocusLeftPanel` (1) | Left panel open on Navigation |
| `CycleNextPanel` (E) | Transactions tab |
| `CycleNextPanel` (E) | Contacts tab, Homer Platform highlighted |
| `UI_Select` (Space) | Station remained highlighted; no visible change reported |
| `UI_Right` (Right Arrow) | Request Docking highlighted |
| `UI_Select` (Space) | Docking granted |

Fresh journal events at `2026-10-24T02:57:25Z` recorded both
`DockingRequested` and `DockingGranted` for Homer Platform, market ID
`4210924803`, with landing pad 10. Earlier events for a different station were
read before requesting and were not mistaken for this result.

No flight, throttle, landing-gear or weapon controls were sent. Bindings and
the input allow-list were read without modification.

## Preconditions and limits

The initial tab was confirmed by the commander, and the commander confirmed
the station highlight and then Request Docking before activation. Tabs and
contact selection can persist between uses. Do not blindly replay two E
presses when the initial tab is unknown, or assume that the desired station
will always occupy the highlighted row. The apparently ineffective first
Select is retained above as an observation, not a necessary proven step.

The current journal interface can identify the station actually contacted
after submission through fresh `DockingRequested`, `DockingGranted` or
`DockingDenied` events. Compare station identity, preferably market ID as well
as name, with the requested station and distinguish denial from timeout.

The navigation destination in `Status.json` is useful context but does not
prove which Contacts row is highlighted. Before submission, verifying that row
currently requires commander confirmation or a visual observation; neither a
skill nor a hardcoded key sequence resolves that missing state by itself.

A future `request_docking` tool should take an expected station, verify known
preconditions, carry out only the bounded panel chore, and confirm a fresh
matching journal response. Stop on ambiguity or mismatch and do not retry
automatically. Station approach and flight remain separately controlled by
the commander.

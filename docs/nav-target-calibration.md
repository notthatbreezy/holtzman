# Supervised nav_target calibration

On October 25, 2026, in AD Leonis (normal space near Homer Platform, 3440x1440
window), the commander authorized step-by-step calibration of the
`nav_target_*` tools through a test server running this build. Each call was
announced and approved before it ran.

## Results

| Step | Result |
| --- | --- |
| `nav_target_start` (lock Homer Platform) | Left panel opened on the Navigation tab; Status.json `GuiFocus` 2 (ExternalPanel). Highlight on the top row, Homer Platform. |
| `nav_target_move` rows 1 | Highlight moved to Bear Point; ~3% of the window changed. `expect` Bear Point correctly did not arm confirm. |
| `nav_target_move` rows -1 | Highlight back on Homer Platform; confirm armed. |
| `nav_target_confirm` (lock) | Default `lock_steps` (Select, wait, Select) locked Homer Platform. Status.json `Destination` matched, panel closed, `GuiFocus` 0. HUD showed the target marker. **Validated.** |
| `nav_target_start` (unlock) | Opened as before. The locked station appears in the list as `< HOMER PLATFORM >`. |
| `nav_target_confirm` (unlock) | **Failed, correctly reported.** The tool reported failure: Status.json still listed Homer Platform as `Destination`. The commander wasn't watching at the time, but the Navigation list opened later still showed `< HOMER PLATFORM >`, so the unlock did not happen and Status.json was right. The default `lock_steps` don't unlock an already-locked station; the button sequence for a locked row needs calibrating. |

## Observations

- The Navigation list shows "Filters active" with a Set Filters button;
  fleet carriers were not listed in this system.
- The default `list_region` (left half) cuts off the right side of the list
  and includes empty cockpit. Roughly x 0.20, y 0.20, w 0.45, h 0.65 frames
  it at this resolution; not yet written to config.
- With the target first in the list, it is unclear whether the highlight
  starts on the top row or on the locked target.

## Docking request (same panel, Contacts tab)

Later the same session, on request, docking at Homer Platform was requested
with `press_binding` + `observe`: `FocusLeftPanel` (opened on Navigation),
`CycleNextPanel` x2 (Contacts, Homer Platform highlighted), `UI_Right`
(Request Docking highlighted), `UI_Select`. `DockingGranted`, pad 9, one
second later. Closing with `FocusLeftPanel` while focus was on the right-hand
button **did not** close the panel; it returned focus to the list. `UI_Back`
closed it (`GuiFocus` 0). After a grant, the same button reads Cancel
Docking, so a stray Select would cancel it. `nav_target`'s default
`close_steps` (`FocusLeftPanel`) worked after a lock, but `UI_Back` may be
the more robust close.

## Still to calibrate

- Unlock, with the commander watching: which button sequence a locked row
  needs.
- Supercruise assist, in supercruise: is the button below (`UI_Down`) or
  beside (`UI_Right`) lock/unlock?
- Scrolling past the visible rows in a busier system.
- Where the highlight starts when the target isn't the first row.

# Request docking

Use when the Captain asks to request docking or clearance at a named or currently
targeted station. The order authorizes the menu steps needed to request clearance;
do not ask for confirmation again on the Request Docking button.

This procedure requests clearance. Do not claim the ship has landed, and do not
infer a docking grant from a keypress alone.

1. Read current instruments and inspect a cockpit screenshot. Verify the intended
   station, normal space, not already docked, and range within 7.5 km. Read target
   distance from the screen; galactic coordinates are not station range.
2. Discover real controls with `list_bindings` when available. Exact Elite action
   names include `FocusLeftPanel`, `UI_Up`, `UI_Down`, `UI_Left`, `UI_Right`,
   `UI_Select`, `UI_Back`, `CycleNextPanel` and `CyclePreviousPanel`. Human labels
   such as `UI panel left` and `PanelLeft` are not action names. An older server
   without discovery may still expose `FocusLeftPanel`; do not invent a different
   spelling or conclude it is unavailable without checking the actual tool result.
3. If the left panel is closed, press `FocusLeftPanel` with `observe: true`.
   Inspect the returned picture. Open Contacts using the observed tabs and the
   actual panel-cycle controls. Never guess a fixed number of presses.
4. Select the intended station in Contacts, checking the highlighted name. Move
   to its Request Docking option using one observed menu action at a time.
5. Select Request Docking. A requested docking task already covers this selection;
   it does not need another permission question.
6. Verify an observed docking grant, assigned pad and timer, or a fresh
   `DockingGranted` journal event. If refused, report the refusal and reason. If
   nothing confirms the outcome, say clearance is unconfirmed.

Stop for an unknown target, out-of-range/supercruise state, unexpected menu,
unavailable actual keyboard mapping, or an uncertain input outcome. Do not retry a
keypress blindly. A rejected unknown action name is a discovery problem, not proof
that the Captain withheld permission or that all cockpit controls are prohibited.

When preserving a successful variation as a playbook, record the actual binding
names, observations and success evidence. Keep untested variations marked draft.

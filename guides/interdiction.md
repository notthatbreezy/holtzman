# Interdiction

Another ship (NPC or player) pulls you out of supercruise. Haulers with
valuable cargo are prime targets.

## During the interdiction

- A tether minigame: steer toward the escape vector on the HUD and keep it
  ahead to escape.
- If escape is unlikely, **submit** by cutting throttle to zero. Submitting
  gives a shorter FSD cooldown than losing the minigame.
- Status.json flags: `BeingInterdicted` during, `IsInDanger` while hostile
  ships are near. Journal events: `Interdicted`, `EscapeInterdiction`.

## After dropping out

- Boost away, put power to systems and engines, and charge the FSD.
- Larger ships nearby and stations cause **mass lock** (the `FsdMassLocked`
  flag), which stops the FSD from charging. Fly clear, then jump to another
  system (high wake) or back to supercruise (low wake).
- Pirates often demand cargo. Dropping some can end the encounter; dying
  loses all cargo and costs the rebuy.

## For the ship computer

- When `BeingInterdicted` or `IsInDanger` appears, switch to short, urgent
  callouts. Never open panels, take screenshots of menus, or press controls
  until it clears.
- Remind the Captain of cargo value versus rebuy when deciding whether to
  fight, run, or submit.

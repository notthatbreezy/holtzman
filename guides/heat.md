# Heat

Heat damages modules and then the hull. It builds from scooping, firing,
silent running, charging the FSD, and flying close to stars.

## What to know

- The `OverHeating` Status.json flag means the ship is running hot. Status
  doesn't expose the exact temperature, so the HUD is the only gauge.
- Above about 100% heat, modules start taking damage; much higher and the
  hull does too **(verify thresholds)**.
- Fuel scooping is the usual culprit on expeditions. Scoop from farther
  out and throttle down if heat climbs. Leave the star's corona once the
  tank is full.
- Arriving very close to a star, especially in close binaries, can cause an
  emergency drop from supercruise and a heat spike. Turn away and fly out.
- Heat sinks dump heat instantly; worth fitting for exploration and for
  scooping-heavy routes.

## For the ship computer

- While `ScoopingFuel` is set, watch for `OverHeating` and call it out
  immediately.
- If `OverHeating` persists, suggest throttling down and pulling away from
  the star, or a heat sink if fitted.

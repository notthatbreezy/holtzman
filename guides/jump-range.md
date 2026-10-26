# Jump range

How far one hyperspace jump can go. It decides how many jumps a trip
takes, which often matters more than distance.

## What changes it

- **Mass.** Cargo and fuel weigh down the ship. A full hold can cut range
  dramatically. Example from this ship's own logs: a Panther Clipper Mk II
  managed 8–12 ly jumps empty but about 7.5 ly with 1,168 t of uranium, so a
  3-jump outbound trip took about 15 jumps back.
- **FSD and engineering.** A larger or engineered Frame Shift Drive
  ("increased range") and a Guardian FSD Booster add range.
- **Supercharging.** Flying through a neutron star's jet boosts the next
  jump to 4x range; a white dwarf's to 1.5x. Each supercharge damages the
  FSD a little; white dwarfs hit harder and their cones are more dangerous.
  Keep an AFMU (auto field-maintenance unit) to repair the FSD.
- **FSD injection ("jumponium").** Synthesis from collected materials
  boosts one jump by 25%, 50% or 100% depending on grade.

## What holtzman can see

- The `Loadout` journal event's `MaxJumpRange` (in `status` as
  `max_jump_range_ly`) is the game's figure for the current loadout. Treat
  it as the unladen best case.
- `FSDJump` events record each `JumpDist`; recent jumps show the real laden
  range.

## For the ship computer

- When planning, distinguish distance in light-years from the number of
  jumps at the current mass. A closer source can cost more jumps once
  loaded.
- Recommend sources and destinations within one laden jump when possible.

# Fuel

Running dry far from help is the most common way to end an expedition.
Treat fuel as a standing watch, not a thing to check when asked.

## Scoopable stars

Only main-sequence stars of classes **K, G, B, F, O, A, M** can be scooped
("KGBFOAM"). Not scoopable: brown dwarfs (L, T, Y), T Tauri, white dwarfs
(D…), neutron stars (N), black holes, Wolf-Rayet (W), and carbon stars (C,
S, MS). The star class is shown in the galaxy map and in the `FSDTarget`
journal event (`StarClass`) for the next jump.

## Reading the gauge

- Status.json `Fuel.FuelMain` is the main tank in tonnes; `FuelReservoir`
  is the small tank feeding the power plant.
- The `LowFuel` flag is set below 25% of the main tank.
- A jump's fuel cost grows with ship mass and jump length. A full hold
  costs more fuel per jump, as well as range (see `jump-range`).

## Rules of thumb

- Keep enough fuel for at least two more jumps when the next star can't be
  scooped, and top up at every scoopable star on long trips.
- Plot with the galaxy map's scoopable-stars option on long routes
  **(verify the exact option name in your game version)**.
- No fuel scoop fitted means every trip has a hard limit; know it before
  leaving the bubble.
- Scooping heats the ship; see `heat`.

## Out of fuel

If the main tank empties away from a station, the ship can't jump. Ask for
help from the **Fuel Rats** (fuelrats.com), a player group that delivers
fuel. If the emergency oxygen timer starts, their standing advice is to exit
to the main menu at once and contact them.

## For the ship computer

- Warn when `LowFuel` appears, and before any jump to a non-scoopable star
  with less than two jumps of fuel left.
- During long routes, mention scoopable stars ahead when fuel is under half.

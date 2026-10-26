# Playbook: exploring unexplored space for money

How to find systems nobody has scanned, decide quickly what's worth your
time, and bring the data home. Read `exploration` first for the basics.

## Where the money is

Value comes from body type, mapping, and being first.

- **Earth-like worlds, water worlds, ammonia worlds**, and anything
  **terraformable** (especially terraformable high-metal-content worlds
  and water worlds) are the money bodies. Most others are near worthless.
- **Mapping** with probes multiplies a body's value several times.
- **First discovered** and **first mapped** each add a large bonus. The
  tag goes to the first commander to *sell* the data, not the first to
  scan it. Sitting on data risks someone else selling first.
- Rough order of value for a first-discovered, first-mapped body
  **(verify; values change between updates)**: Earth-like world (several
  million credits) > terraformable water world ≈ ammonia world > water
  world ≈ terraformable high-metal-content world (around a million or
  more) > everything else (thousands).
- **Exobiology** (Odyssey) is often worth more than scans: some species
  pay well over ten million credits per three-sample set, and first-logged
  species pay a large multiple **(verify)**. Bio signals appear in the FSS
  (`FSSBodySignals`) and on mapping (`SAASignalsFound`).

## Finding untouched systems

The bubble (around Sol) and the highways (Sol to Colonia, to Sagittarius
A*, to Beagle Point, the popular neutron routes) are thoroughly scanned.
Aim for places traffic skips:

- **Distance off the lanes.** A thousand or more light-years from the
  bubble, away from the routes to famous destinations.
- **Up or down.** A few hundred to a couple thousand light-years above or
  below the galactic plane sees far less traffic, while stars are still
  plentiful near the plane.
- **Ordinary procedural systems.** Names like `Sector XY-Z d12-34` are
  rarely visited unless they lie on a route. In those names the letter
  after the dash is the mass code: `a` systems are small and often poor;
  `c`–`e` are larger and more likely to have big systems **(rule of thumb)**.
- **Star classes.** Earth-like and water worlds turn up most often around
  F, G and K stars; A and B stars can host big systems too.

### Telling whether a system is unexplored

- **In game:** after the honk, bodies already discovered show the
  discoverer's name in the system map; undiscovered ones don't. Each `Scan`
  event's `WasDiscovered` says it for certain.
- **Before jumping:** if Spansh or EDSM know nothing about a system, it's
  likely unexplored (they only know what tool-using commanders reported,
  so this is a strong hint, not proof).

## Working fast

1. Honk on arrival. Few bodies plus only small, cold stars: move on.
2. In the FSS, zoom only on likely money bodies. Ignore icy and rocky
   clutter.
3. Map only Earth-like, water, ammonia and terraformable bodies, plus
   bodies with bio signals if you're doing exobiology.
4. Check fuel and heat, then jump.

Speed matters more than completeness: a system you leave at 60% scanned
can still pay most of its value if you mapped the right body.

## Bringing it home

- Sell at any station with Universal Cartographics (and Vista Genomics for
  biology). A long expedition can come back via a frontier station or a
  fleet carrier with these services.
- Sell in batches when the unsold total gets uncomfortable; death loses it
  all (see `rebuy`).

## Near-bubble money

For quick credits close to home, Spansh's "Road to Riches" plotter routes
through known high-value bodies. They're already discovered, so no first-
discovery bonus, but mapping still pays.

## For the ship computer

- On each new system: report body count, any `WasDiscovered: false`
  money bodies, and bio signals; recommend map or skip.
- Track an estimate of unsold data value and say when it's time to sell.
- Gaps worth proposing as upgrades: `system_bodies` (summarize scans with
  estimated values), `system_known` (ask Spansh/EDSM whether a system has
  been reported), and an exploration route planner that avoids the
  highways.

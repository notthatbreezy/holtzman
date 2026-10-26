# Mission profile: exploration

Scan new systems and bodies, then sell the data at a Universal
Cartographics office. The money is in what you scan, where you scan it,
and getting it home alive. For finding untouched space and maximizing
income, see `exploration-for-profit`.

## Prep

- **Jump range first.** Engineered FSD, Guardian FSD Booster, light
  modules. Good explorers include the Diamondback Explorer (cheap), Asp
  Explorer, Krait Phantom, Mandalay, and Anaconda. Bulk haulers make poor
  explorers.
- **Fuel scoop**, as large as fits (see `fuel`).
- **AFMU** to repair modules, including the FSD after supercharging.
- **Detailed Surface Scanner** for mapping bodies.
- Optional: SRV hangar and a genetic sampler for exobiology (Odyssey),
  heat sinks, a spare fuel tank.
- Rebuy covered, with margin (see `rebuy`).

## Scanning, in order

1. **Honk** (discovery scan) on arrival: reveals the body count
   (`FSSDiscoveryScan` event: `BodyCount`, `Progress`).
2. **FSS** (Full Spectrum System scanner): resolve bodies. Each completes a
   `Scan` event with `PlanetClass`, `TerraformState`, `WasDiscovered` and
   `WasMapped`. `WasDiscovered: false` means nobody has sold data on it yet.
3. **Map** high-value bodies with probes (DSS). Using no more probes than
   the target earns an efficiency bonus (`SAAScanComplete`).
4. Leave. Don't fully scan low-value systems: time is the real cost.

## Neutron highway

Spansh's neutron plotter routes through neutron stars for 4x jumps (see
`jump-range`). Approach the jet, let it carry you through, and don't sit in
the cone. The FSD takes damage each time; repair it with the AFMU.

## Watches

- Fuel before every jump to a non-scoopable star (see `fuel`).
- `OverHeating` while scooping (see `heat`).
- Value of unsold data versus risk; suggest selling runs home.
- Landing: high gravity planets wreck ships and SRVs; check gravity first.

## Tools

- `status` for fuel, flags and jump range; `recent_events` with
  `only: ["Scan", "FSSDiscoveryScan", "SAAScanComplete", "FSSBodySignals"]`
  to read scans; `plot_route` for the next leg.
- No exploration-specific tools exist yet. When the Captain keeps asking
  "what's worth mapping here?", propose a `system_bodies` upgrade that
  summarizes this system's scans with estimated values.

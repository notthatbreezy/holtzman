# Mission profile: mining

Break valuable minerals out of planetary rings and sell them where demand
is high. Prices swing a lot, so where you sell matters as much as what you
mine.

## Two kinds of mining

- **Laser mining.** Mining lasers chip fragments off asteroids; collector
  limpets bring them in; a refinery turns them into cargo. Common targets:
  platinum (metallic rings), low-temperature diamonds (icy rings).
- **Core mining.** Prospector limpets and a pulse wave analyser find
  asteroids with a core (motherlode); seismic charges crack them; the core
  chunks are collected. Common targets: void opals (icy), alexandrite,
  grandidierite, monazite, musgravite, painite and other rare gems.
  Sub-surface deposits use displacement missiles; surface deposits an
  abrasion blaster.

## Prep

- Refinery, collector and prospector limpet controllers, limpets, cargo
  racks; for core mining, seismic charges and a pulse wave analyser.
- Shields and a rebuy margin: pirates visit rings.
- A Detailed Surface Scanner to find **hotspots** (overlapping hotspots are
  best) by mapping the ring.

## The loop

1. Find a ring with hotspots for the target mineral (map the ring with the
   DSS, or use community tools).
2. Prospect asteroids; mine only rich ones (`ProspectedAsteroid` shows the
   content level and percentages; `MotherlodeMaterial` means a core).
3. Mine, refine, collect until full (`MiningRefined` per tonne).
4. Find the best buyer **before** flying: price and demand both matter,
   and dumping a full hold on a low-demand market crashes the price.

## Watches

- Limpet count and hull, heat from lasers, pirates (`IsInDanger`).
- Market prices go stale fast; recheck before selling.

## Tools

- `find_commodity_market` with `action: sell` for the best buyers, with
  enough `amount` of demand for the whole hold.
- `cargo` for refined tonnes; `recent_events` with
  `only: ["ProspectedAsteroid", "MiningRefined", "AsteroidCracked"]`.
- Gap worth proposing: a prospector summary that ranks asteroids by
  target-mineral percentage.

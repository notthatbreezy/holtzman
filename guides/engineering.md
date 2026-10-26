# Engineering

Engineers upgrade ship modules (and, in Odyssey, suits and hand weapons)
with blueprints paid for in materials. It's the biggest single lever on
jump range, combat strength and survivability.

## How it works

- **Engineers** live at their own bases. Each must be unlocked: an
  invitation (often from a referral by another engineer), then an
  unlock requirement such as a delivery. Reputation with an engineer
  unlocks higher blueprint grades.
- **Blueprints** come in grades 1–5. Higher grades cost rarer materials.
  An optional **experimental effect** adds a special bonus.
- **Remote workshop:** once you've unlocked a blueprint grade with an
  engineer, you can apply it at any station with outfitting, without
  flying to the engineer **(verify current rules)**.
- **Materials** come in three kinds:
  - **Raw** (elements): surface prospecting in an SRV, mining, some
    signal sources.
  - **Manufactured**: ship wrecks, high-grade emission signal sources,
    combat.
  - **Encoded** (data): scanning ships, wakes, data points.
- **Material traders** at some stations swap materials of the same kind
  at a ratio, which turns surplus into what you need.
- **Guardian technology** (like the Guardian FSD Booster) is bought with
  Guardian materials from Guardian sites, not from engineers.

## Priorities by activity

- **Exploration:** FSD "increased range" (grade 5) first, then the
  Guardian FSD Booster; lightweight modules.
- **Trading and mining:** FSD range for laden trips, shields for safety.
- **Combat:** shields and weapons; see the engineer list in-game.

## What holtzman can see

The journal records `EngineerProgress` (unlocks and ranks),
`EngineerCraft` (applied blueprints), and `Materials` (inventory counts at
login). holtzman has no materials tool yet.

## For the ship computer

- When the Captain wants a specific upgrade, list which materials it needs
  and which of them are short, and where each kind comes from.
- Gap worth proposing: a `materials` tool reading the journal inventory,
  and a blueprint shopping list.

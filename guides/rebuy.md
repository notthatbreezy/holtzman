# Rebuy and what death costs

Destroyed ships are replaced for the insurance excess, the **rebuy**, about
5% of the ship's total value including modules.

## What you lose on death

- The rebuy credits.
- All cargo (cargo is not insured).
- Unsold exploration data (Universal Cartographics) and unsold exobiology
  samples (Vista Genomics).
- Unredeemed bounties and combat bonds.

You respawn at the last station you docked at, which may be very far away
after an expedition.

## Rules

- Never fly a ship without enough credits to cover its rebuy. If the
  rebuy can't be paid, you get a much worse ship or a loan.
- The value at risk is rebuy plus cargo plus unsold data. On a long
  expedition, unsold data can be worth far more than the rebuy.

## What holtzman can see

The `Loadout` event's `Rebuy` field (in `status` as `ship.rebuy`) and the
credit balance (`credits`).

## For the ship computer

- Warn if credits drop below the current rebuy.
- On expeditions, track roughly how much unsold data is aboard and suggest
  selling when the risk grows large.

# Mission profile: trading and community goals

Buy where a commodity is cheap and plentiful, sell where it's wanted. For
community goals (CGs), deliver a specific commodity to one station for a
reward paid when the goal ends.

## Prep

- Cargo racks in every slot you can spare; a shield is still worth having
  near busy systems.
- Credits for cargo **and** the rebuy (see `rebuy`). Cargo isn't insured.
- Know your pad size: large ships need large pads (no outposts).

## The loop

1. Pick the source with the shortest **laden** round trip, not the
   cheapest price (see `jump-range`). A 5% cheaper source 15 jumps away
   loses to a slightly dearer one a single jump away.
2. Check the source market in-game before committing: stock, price, and
   market age. Community data can be hours old and other commanders drain
   stock fast during CGs.
3. Buy, plot, fly, sell. Repeat while the source holds out.

## Community goals

- Sign up with the goal before delivering (the `CommunityGoalJoin` event
  confirms it). Contributions count in Open, Private Group and Solo.
- Rewards depend on the tier the whole community reaches and on your
  percentile band among contributors (top 10%, 25%, 50%, 75%…). Claim them
  at the CG station after it ends (`CommunityGoalReward`).
- The CG station usually pays far above market for the goal commodity, so
  profit per run is dominated by trip time.
- The `CommunityGoal` journal event shows progress, tier, percentile band,
  and expiry; check it between runs.

## Watches

- `IsInDanger` / `BeingInterdicted` while loaded (see `interdiction`).
- Fuel per laden jump is higher than empty.
- Source stock: warn when a run would leave the source short for the next.

## Tools

- `find_commodity_market` (Spansh) for sources (`action: buy`) and buyers
  (`action: sell`), `plan_trade_route` for open-ended profit routes.
- `market` for the last market opened in-game; `cargo` for the hold;
  `plot_route` for the next leg on the Captain's order.

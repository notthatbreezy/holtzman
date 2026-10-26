# Recorded fleet inventory

The read-only MCP tool `fleet` answers owned-ship and location questions and supplies
recorded equipment for role comparisons, including pirate hunting. Default `{}`
returns all last-known owned ships with compact combat-relevant equipment; pass
`{"ship_id":4,"include_modules":true}` to inspect one ship's full recorded module
list and engineering. ShipID is the game's numeric identifier, not the painted ident.
The model makes the suitability assessment; the server does not invent combat ratings.

## Reconciliation

The server reads all available `Journal.*.log` files in its configured journal
directory. Relevant events are replayed by parsed UTC timestamp, with deterministic
file/line ordering for ties. The most recently observed commander and Live/Legacy
galaxy scope is selected; numeric IDs from different commanders are never merged.

- `LoadGame`, `Loadout`, `ShipyardNew` and `ShipyardSwap` identify owned/current hulls.
- `ShipyardSell`, `SellShipOnRebuy` and sale fields on buy/swap remove ownership.
  `ShipyardBuy` does not identify the new hull until `ShipyardNew`/`Loadout` supplies
  its ID. Stored old ships retain their recorded fits.
- Complete `StoredShips` snapshots reconcile missing older sales: ships absent from
  both stored lists are removed, except for the separately known current ship.
  Partial/malformed snapshots never delete records.
- Locations have independent timestamps and provenance. A remote ship may have a
  known system and market ID but no station name. A station name is filled only
  from a previously observed matching market ID, never guessed.
- Transfers report an unconfirmed destination and no confirmed current location.
  Elapsed transfer time never proves arrival. Later inventory/active-ship evidence
  can establish arrival; an `InTransit` snapshot remains pending.
- Latest full `Loadout` supplies recorded modules. Later module/engineering changes
  mark it possibly outdated until another Loadout arrives. A death marks unresolved
  ownership/equipment uncertainty until further evidence. Missing fits stay unknown.

This is an on-demand archive scan, not a background account synchronizer. The model
receives the reduced fleet; it does not need to reconstruct ownership from raw logs.
No game controls, transfer requests, purchases or external account calls occur.

## Coverage and limitations

The result always reports incomplete-account-history status, available event range,
latest complete stored-ship snapshot, skipped lines and unreadable files. Deleted
journals, another PC's missing logs, old records without commander identity, missing
loadouts and later unobserved carrier movements can leave gaps. A recorded timestamp
does not certify present truth. Visit the shipyard to produce a fresh stored-ship
inventory; inspect/switch to a ship to obtain a fresh full loadout where needed.
Do not switch ships automatically just to answer a question.

The field `possibly_outdated: false` means no later modifying event was observed;
it does not guarantee a current fit. The separate age and timestamp still matter.
Snapshots from another galaxy scope or unresolved identity are not silently merged.

## Deployment and checks

Update/rebuild the server on SKIPPY from `main`, then Stop/Start HOLT so its new MCP
session discovers `fleet`. Restarting an old binary alone cannot add the tool.
From SKIPPY's checkout:

```powershell
git pull --ff-only
powershell -ExecutionPolicy Bypass -File scripts\update-holtzman-server.ps1
```

Rust regression tests cover historical sale ordering, trade-ins/rebuy sales,
complete/partial snapshots, commander/galaxy isolation, transfers, loadout changes,
module detail filtering, torn lines and reused IDs. HTTP integration tests verify
the advertised read-only tool and an actual call against fixture journals.
`python -m client.check_fleet` is an opt-in, small paid GPT-6 Luna check using the
Captain's exact question, simulated ships and the desktop/ST prompt paths. It never
connects to or controls the real game. Outputs are retained alongside this change.

The initial six GPT-6 Luna low trials (three desktop, three ST) all called `fleet`
and selected the recorded Vulture instead of querying cargo. These were simulated
inventory results, not the Captain's actual fleet. One answer described the Panther
fit as unrecorded despite the fixture's empty equipment list; successful tool use
does not establish perfect interpretation. Full outputs are in
[the model check](evaluations/2026-10-26-fleet/model-check.json).

Event fields follow Frontier's [Player Journal manual](https://hosting.zaonce.net/community/journal/v32/Journal_Manual-v32.pdf),
especially Loadout, ShipyardBuy/New/Sell/Swap/Transfer, StoredShips and SellShipOnRebuy.

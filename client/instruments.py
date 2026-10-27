"""Grounded ship context and observed trends; no language-model arithmetic."""
import hashlib
import json
from datetime import datetime, timezone

from client.telemetry import reading_age

EVENTS = ["FSDJump", "Docked", "Undocked", "Touchdown", "Liftoff", "SupercruiseEntry",
          "SupercruiseExit", "Location", "Loadout", "RefuelAll", "RefuelPartial"]


def stamp(value):
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        return parsed.timestamp() if parsed.tzinfo else None
    except (AttributeError, ValueError, TypeError):
        return None


def snapshot(facts, now=None):
    now = now or datetime.now(timezone.utc)
    status = facts.get("status", {})
    flags = status.get("flags", [])
    capacity = facts.get("ship", {}).get("cargo_capacity")
    occupied = status.get("cargo_tons")
    credits = facts.get("credits")
    rebuy = facts.get("ship", {}).get("rebuy")
    free = max(0, capacity - occupied) if isinstance(capacity, (int, float)) and isinstance(occupied, (int, float)) else None
    budget = max(0, int(credits - rebuy)) if isinstance(credits, (int, float)) and isinstance(rebuy, (int, float)) else None
    return {"available": True, "retrieved_at_utc": now.isoformat(),
            "source_age": reading_age(facts, now), "location": facts.get("location", {}),
            "ship": facts.get("ship", {}), "credits": facts.get("credits"),
            "cargo_tonnes": status.get("cargo_tons"),
            "cargo_free_tonnes": free,
            "trade_planning_budget_credits": budget,
            "trade_planning_budget_note": "Read-only planning default: current credits minus one ship rebuy reserve. An explicit Captain budget overrides this; no credits are spent by planning.",
            "fuel_main_tonnes": (status.get("fuel") or {}).get("FuelMain"),
            "fuel_reservoir_tonnes": (status.get("fuel") or {}).get("FuelReservoir"),
            "fuel_percentage": None, "fuel_percentage_note": "Tank capacity is not supplied; do not convert tonnes to percent.",
            "docked": "Docked" in flags, "landed": "Landed" in flags,
            "mass_locked": "FsdMassLocked" in flags, "supercruise": "Supercruise" in flags,
            "flags": flags, "gui_focus": status.get("gui_focus"),
            "controls": facts.get("controls"),
            "destination": status.get("destination"), "plotted_route_to": facts.get("plotted_route_to"),
            "loadout_note": "Installed modules and free optional slots are not provided by this snapshot."}


class InstrumentHistory:
    def __init__(self, db):
        self.db = db
        db.executescript("""
            CREATE TABLE IF NOT EXISTS instrument_samples(
                observed REAL PRIMARY KEY, system TEXT, state TEXT, place TEXT, ship TEXT, fuel REAL);
            CREATE TABLE IF NOT EXISTS instrument_events(
                id TEXT PRIMARY KEY, occurred REAL, data TEXT);
        """)

    def observe(self, facts, now=None):
        now = now or datetime.now(timezone.utc)
        status = facts.get("status", {})
        flags = status.get("flags", [])
        state = ("docked" if "Docked" in flags else "landed" if "Landed" in flags
                 else "supercruise" if "Supercruise" in flags else "flight")
        place = facts.get("location", {}).get("near") if state in ("docked", "landed") else None
        ship = facts.get("ship", {})
        identity = str(ship.get("ident")) + "/" + str(ship.get("type"))
        self.db.execute("INSERT OR IGNORE INTO instrument_samples VALUES(?,?,?,?,?,?)", (
            now.timestamp(), facts.get("location", {}).get("system"), state, place, identity,
            (status.get("fuel") or {}).get("FuelMain")))
        self.db.execute("DELETE FROM instrument_samples WHERE observed<?", (now.timestamp() - 21600,))
        self.db.commit()

    def events(self, events, now=None):
        now = now or datetime.now(timezone.utc)
        for event in events:
            occurred = stamp(event.get("timestamp"))
            if event.get("event") not in EVENTS or occurred is None or occurred > now.timestamp() + 5:
                continue
            # Keep only facts relevant to context, not arbitrary journal fields.
            selected = {k: event[k] for k in ("timestamp", "event", "StarSystem", "StationName",
                        "Body", "FuelUsed", "FuelLevel", "JumpDist", "Ship", "ShipID") if k in event}
            raw = json.dumps(selected, sort_keys=True)
            self.db.execute("INSERT OR IGNORE INTO instrument_events VALUES(?,?,?)",
                            (hashlib.sha256(raw.encode()).hexdigest(), occurred, raw))
        self.db.execute("DELETE FROM instrument_events WHERE occurred<?", (now.timestamp() - 604800,))
        self.db.commit()

    def context(self, facts, now=None):
        now = now or datetime.now(timezone.utc)
        current = snapshot(facts, now)
        rows = self.db.execute("SELECT observed,system,state,place,ship,fuel FROM instrument_samples WHERE observed>=? AND observed<=? ORDER BY observed",
                               (now.timestamp() - 21600, now.timestamp())).fetchall()
        events = [json.loads(raw) for (raw,) in self.db.execute(
            "SELECT data FROM instrument_events WHERE occurred<=? ORDER BY occurred DESC LIMIT 200", (now.timestamp() + 5,)).fetchall()[::-1]]
        jumps = [e for e in events if e["event"] == "FSDJump"][-8:]
        trend = {"recent_jumps": jumps, "duration_note": "Observed durations are lower bounds; gaps over 15 seconds reset continuous observation. Journal times are separate confirmed event times."}
        if rows:
            last = rows[-1]
            for label, indexes in [("system", (1,)), ("state", (1, 2, 3))]:
                start, next_time = last[0], last[0]
                for row in reversed(rows[:-1]):
                    if next_time - row[0] > 15 or any(row[i] != last[i] for i in indexes):
                        break
                    start, next_time = row[0], row[0]
                trend[label + "_observed_seconds"] = max(0, round(now.timestamp() - start))
            trend["observed_state"] = last[2]
            candidates = []
            for row in reversed(rows):
                if row[0] < now.timestamp() - 300 or row[1:5] != last[1:5] or not isinstance(row[5], (int, float)):
                    break
                candidates.append(row)
            candidates.reverse()
            if len(candidates) >= 2:
                elapsed = candidates[-1][0] - candidates[0][0]
                delta = candidates[-1][5] - candidates[0][5]
                discontinuity = any(b[0] - a[0] > 15 or b[5] > a[5] + .001 for a, b in zip(candidates, candidates[1:]))
                intervening = any((stamp(e["timestamp"]) or 0) >= candidates[0][0]
                                  and e["event"] in ("FSDJump", "RefuelAll", "RefuelPartial", "Loadout") for e in events)
                trend["fuel_window_seconds"] = round(elapsed)
                trend["fuel_main_net_change_tonnes"] = round(delta, 4)
                age = current["source_age"]["age_seconds"]
                if elapsed >= 60 and not discontinuity and not intervening and age is not None and 0 <= age <= 30:
                    trend["fuel_main_net_decrease_tonnes_per_minute"] = round(max(0, -delta) * 60 / elapsed, 4)
                    trend["fuel_rate_note"] = "Observed net MAIN-tank decrease, not engine burn or an endurance prediction. Reservoir transfers affect it."
                else:
                    trend["fuel_rate_note"] = "No meaningful steady rate: short/gapped/stale window, refuelling, jump or ship/loadout change."
        # Anchor only to the latest matching transition, never a prior visit.
        system_events = [e for e in events if e["event"] in ("FSDJump", "Location")]
        if system_events and system_events[-1].get("StarSystem") == current["location"].get("system"):
            e = system_events[-1]
            if e["event"] == "FSDJump":
                trend["current_system_arrival_journal_utc"] = e["timestamp"]
                trend["current_system_seconds_since_journal_arrival"] = round(now.timestamp() - stamp(e["timestamp"]))
        transitions = [e for e in events if e["event"] in ("Docked", "Undocked", "Touchdown", "Liftoff", "FSDJump", "Location")]
        if transitions:
            e = transitions[-1]
            if (current["docked"] and e["event"] == "Docked" or current["landed"] and e["event"] == "Touchdown"):
                trend["current_state_journal_utc"] = e["timestamp"]
                trend["current_state_seconds_since_journal_event"] = round(now.timestamp() - stamp(e["timestamp"]))
        current["history"] = trend
        return current

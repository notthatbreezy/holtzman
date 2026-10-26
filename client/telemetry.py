"""Quiet, read-only watches. Never turn an alert into a game action."""
from datetime import datetime, timezone


def reading_age(facts, now=None):
    now = now or datetime.now(timezone.utc)
    raw = facts.get("status", {}).get("timestamp")
    try:
        stamp = datetime.fromisoformat(raw.replace("Z", "+00:00"))
        if stamp.tzinfo is None:
            return {"timestamp": raw, "age_seconds": None, "note": "Timestamp has no timezone"}
        age = round((now - stamp).total_seconds())
        return {"timestamp": raw, "age_seconds": age,
                "note": "Time of the last recorded status change, not a game heartbeat. Old timestamps alone do not prove the game is closed."}
    except (ValueError, AttributeError, TypeError):
        return {"timestamp": raw, "age_seconds": None, "note": "No usable timestamp"}


class Watches:
    def __init__(self):
        self.hull_band = None
        self.overheating = False
        self.low_fuel = False

    def update(self, facts, now=None):
        now = now or datetime.now(timezone.utc)
        try:
            stamp = datetime.fromisoformat(facts["status"]["timestamp"].replace("Z", "+00:00"))
            if abs((now - stamp).total_seconds()) > 30:
                return []
        except (KeyError, ValueError):
            return []
        flags = facts["status"].get("flags", [])
        if "Docked" in flags:
            self.hull_band = None
            self.overheating = self.low_fuel = False
            return []
        hull = facts.get("ship", {}).get("hull_health")
        alerts = []
        if isinstance(hull, (int, float)):
            band = 2 if hull <= .15 else 1 if hull <= .30 else 0
            if band and (self.hull_band is None or band > self.hull_band):
                alerts.append({"text": f"Captain! Hull integrity is at {round(hull * 100)} percent. "
                               + ("Hull critical. Clear the danger now." if band == 2 else "Break off and move clear of the danger."),
                               "delivery": "critical" if band == 2 else "urgent"})
            self.hull_band = band
        hot = "OverHeating" in flags
        low = "LowFuel" in flags
        if hot and not self.overheating:
            alerts.append({"text": "Captain, the ship is overheating. Reduce heat exposure.", "delivery": "urgent"})
        if low and not self.low_fuel:
            alerts.append({"text": "Captain, the low fuel warning is active. Check your fuel before continuing.", "delivery": "caution"})
        self.overheating, self.low_fuel = hot, low
        return alerts

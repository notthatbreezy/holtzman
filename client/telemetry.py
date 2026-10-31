"""Quiet, read-only watches. Never turn an alert into a game action."""
from datetime import datetime, timezone
import time

FLAG_BITS = {0: "Docked", 1: "Landed", 3: "ShieldsUp", 19: "LowFuel", 20: "OverHeating",
             22: "IsInDanger", 23: "BeingInterdicted", 24: "InMainShip"}


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
        self.flags = set()
        self.has_shields = False
        self.cooldowns = {}
        self.missions = {}
        self.route = []
        self.route_key = None
        self.fuel_used = None

    def resync(self, status):
        raw = status.get("Flags", 0)
        self.flags = {name for bit, name in FLAG_BITS.items() if isinstance(raw, int) and raw & (1 << bit)}
        self.overheating, self.low_fuel = "OverHeating" in self.flags, "LowFuel" in self.flags

    def resync_facts(self, facts):
        self.flags = set(facts.get("status", {}).get("flags", []))
        modules = facts.get("ship", {}).get("modules", [])
        if "has_shield_generator" in facts.get("ship", {}):
            self.has_shields = bool(facts["ship"]["has_shield_generator"])
        elif modules:
            self.has_shields = any("shieldgenerator" in str(m).casefold() for m in modules)

    def alert(self, key, text, delivery, cooldown=0):
        now = time.monotonic()
        if now - self.cooldowns.get(key, -float("inf")) < cooldown:
            return []
        self.cooldowns[key] = now
        return [{"key": key, "text": text, "delivery": delivery, "cooldown": cooldown}]

    def event(self, kind, data):
        if kind != "flag":
            return []
        flag, on = data.get("flag"), data.get("on")
        before = flag in self.flags
        if on:
            self.flags.add(flag)
        else:
            self.flags.discard(flag)
        if before == on or "InMainShip" not in self.flags or "Docked" in self.flags:
            return []
        if flag == "BeingInterdicted" and on:
            return self.alert("interdiction", "Interdiction, Captain!", "urgent", 30)
        if flag == "ShieldsUp" and not on and self.has_shields:
            return self.alert("shields", "Shields are down.", "urgent", 30)
        if flag == "OverHeating" and on:
            return self.alert("heat", "Ship overheating, Captain.", "urgent", 20)
        if flag == "LowFuel" and on:
            return self.alert("fuel", "Low fuel, Captain.", "caution")
        return []

    def journal(self, event):
        kind = event.get("event")
        if kind in {"LoadGame", "Loadout", "ShipyardSwap", "ShipyardBuy"}:
            self.hull_band = None
        if kind == "Loadout":
            self.has_shields = any("shieldgenerator" in str(m.get("Item", "")).casefold() for m in event.get("Modules", []))
        if kind == "MissionAccepted":
            self.missions[event.get("MissionID")] = event
        elif kind in {"MissionCompleted", "MissionAbandoned", "MissionFailed"}:
            self.missions.pop(event.get("MissionID"), None)
        if kind == "HullDamage" and event.get("PlayerPilot", True) and not event.get("Fighter", False):
            health = event.get("Health")
            if not isinstance(health, (float, int)) or not 0 <= health <= 1:
                return []
            band = 2 if health <= .2 else 1 if health <= .4 else 0
            previous, self.hull_band = self.hull_band, band
            if band and (previous is None or band > previous):
                return self.alert("hull" + str(band), f"Hull at {round(health * 100)} percent.", "critical" if band == 2 else "urgent")
        if "InMainShip" not in self.flags or "Docked" in self.flags:
            return []
        if kind == "UnderAttack":
            return self.alert("attack", "We're taking fire.", "caution", 20)
        if kind == "DockingGranted":
            return self.alert("docking", f"Docking granted, pad {event.get('LandingPad', 'unknown')}.", "normal")
        if kind == "DockingDenied":
            reasons = {"NoSpace": "No landing pads available.", "TooLarge": "Ship too large for available pads.", "Hostile": "Docking denied: hostile status.", "Distance": "Too far away for docking."}
            return self.alert("docking", reasons.get(event.get("Reason"), "Docking denied, Captain."), "normal")
        if kind == "FSDJump" and isinstance(event.get("FuelUsed"), (float, int)):
            self.fuel_used = event["FuelUsed"]
        return []

    def route_alerts(self, route, fuel):
        systems = route.get("Route", [])[1:]
        identity = tuple(s.get("StarSystem") for s in systems)
        if not systems or identity == self.route_key:
            return []
        self.route_key = identity
        scoopable = lambda s: str(s.get("StarClass", ""))[:1].upper() in "KGBFOAM"
        # An unknown star class cannot establish that there are no scoopable stars.
        if len(systems) >= 3 and all(s.get("StarClass") and not scoopable(s) for s in systems[:3]):
            return self.alert("route-stars-" + str(identity), "No scoopable stars for three jumps.", "caution")
        if self.fuel_used and isinstance(fuel, (float, int)):
            jumps = int(fuel / self.fuel_used)
            if jumps < len(systems) and all(s.get("StarClass") and not scoopable(s) for s in systems[:jumps+1]):
                return self.alert("route-fuel-" + str(identity), f"Fuel for about {jumps} jumps; none scoopable.", "caution")
        return []

    def mission_alerts(self, now=None):
        now = now or datetime.now(timezone.utc)
        alerts = []
        for identifier, mission in self.missions.items():
            try:
                expiry = datetime.fromisoformat(mission["Expiry"].replace("Z", "+00:00"))
                if 0 < (expiry - now).total_seconds() <= 3600:
                    alerts += self.alert("mission-" + str(identifier), "Mission expires within one hour, Captain.", "caution", 86400)
            except (KeyError, ValueError, TypeError):
                pass
        return alerts

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
        # Loadout.HullHealth is stale during combat; only HullDamage drives hull alerts.
        hull = None
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

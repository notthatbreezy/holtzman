"""Anchored command grammar. Matching never grants authority to a model."""
import math
import re
import random
from dataclasses import dataclass
from rapidfuzz.fuzz import ratio
from client.policy import normalized

ONE_SHOTS = frozenset({"heat_sink", "chaff", "shield_cell"})
SHIP_BINDINGS = frozenset({"DeployHardpointToggle", "LandingGearToggle", "ToggleCargoScoop",
    "ShipSpotLightToggle", "ToggleButtonUpInput", "NightVisionToggle", "ToggleFlightAssist",
    "DeployHeatSink", "FireChaffLauncher", "UseShieldCell"})
PHRASES = {
    "heat_sink": ["heat sink", "heatsink", "heat think", "drop a heat sink", "pop a heat sink", "fire a heat sink"],
    "chaff": ["chaff", "fire chaff", "launch chaff"],
    "shield_cell": ["shield cell", "use a shield cell"],
    "hardpoints": ["hardpoints", "hard points", "weapons"],
    "landing_gear": ["gear", "landing gear"], "cargo_scoop": ["scoop", "cargo scoop"],
    "lights": ["lights"], "silent_running": ["silent running"],
    "night_vision": ["night vision"], "flight_assist": ["flight assist", "fa"],
    "cancel": ["stop", "cancel", "belay that"],
}
STATES = {
    "hardpoints": {"out": "on", "deploy": "on", "in": "off", "retract": "off", "stow": "off"},
    "landing_gear": {"down": "on", "deploy": "on", "up": "off", "retract": "off"},
    "cargo_scoop": {"out": "on", "open": "on", "deploy": "on", "in": "off", "close": "off", "retract": "off"},
    **{name: {"on": "on", "off": "off"} for name in ["lights", "silent_running", "night_vision", "flight_assist"]},
}

@dataclass(frozen=True)
class Match:
    actions: tuple
    addressed: bool

def phrase_matches(words, phrase):
    expected = phrase.split()
    return len(words) == len(expected) and all(a == b or (len(b) > 2 and ratio(a, b) >= 85) for a, b in zip(words, expected))

def parse(text):
    if text.rstrip().endswith("?"):
        return None
    text = normalized(text)
    if re.search(r"\b(don't|dont|do not|never|not)\b", text) or re.match(
            r"^(should|did|what|what's|is|are|have|has|can|could|would|why|how|does|they said)\b", text):
        return None
    address = re.match(r"^(?:(?:hey|okay) )?holt(?: |$)", text)
    if address:
        text = text[address.end():]
    text = re.sub(r" (please|now|captain|thanks)$", "", text)
    actions = []
    for fragment in re.split(r"\s+(?:and|then)\s+", text):
        words = fragment.split()
        candidates = []
        if fragment == "go dark":
            candidates.append({"action": "silent_running", "state": "on"})
        for name, phrases in PHRASES.items():
            if name in STATES:
                if words and words[-1] in STATES[name] and any(phrase_matches(words[:-1], p) for p in phrases):
                    candidates.append({"action": name, "state": STATES[name][words[-1]]})
                # Common imperative order, with an exact state/verb as well.
                if words and words[0] in {"deploy", "retract", "stow"} & STATES[name].keys() and any(phrase_matches(words[1:], p) for p in phrases):
                    candidates.append({"action": name, "state": STATES[name][words[0]]})
            elif any(phrase_matches(words, p) for p in phrases):
                candidates.append({"action": name})
        unique = {tuple(sorted(c.items())) for c in candidates}
        if len(unique) != 1:
            return None
        command = dict(next(iter(unique)))
        # Conflicting duplicate toggles are ambiguous; identical repeats execute once.
        existing = next((a for a in actions if a["action"] == command["action"]), None)
        if existing and existing != command:
            return None
        if not existing:
            actions.append(command)
        if len(actions) > 10:
            return None
    if not actions or any(a["action"] == "cancel" for a in actions) and len(actions) != 1:
        return None
    actions.sort(key=lambda a: a["action"] not in ONE_SHOTS)
    return Match(tuple(actions), bool(address))

def confident(metadata):
    """Missing/non-finite metrics fail closed. Every segment must pass."""
    if not isinstance(metadata, dict):
        return False
    segments = metadata.get("segments") or metadata.get("transcription") or [metadata]
    if not isinstance(segments, list) or not segments:
        return False
    try:
        return all(math.isfinite(float(s["no_speech_prob"])) and math.isfinite(float(s["avg_logprob"]))
                   and 0 <= float(s["no_speech_prob"]) <= .5 and float(s["avg_logprob"]) >= -.8 for s in segments)
    except (KeyError, TypeError, ValueError):
        return False

def gate(match, *, held, metadata, spoken, open_mic="toggles_only"):
    if not any(a["action"] in ONE_SHOTS for a in match.actions):
        return None
    if spoken and not confident(metadata):
        return "I couldn't confirm that command, Captain. No input sent."
    if spoken and not held and not match.addressed and open_mic != "all":
        return "Hold push-to-talk or address HOLT for consumables, Captain."
    return None

def dictation(text):
    found = re.match(r"^\s*(?:(?:hey |okay )?holt[,.]?\s+)?(?:captain['’]s log|log entry)[\s,.:…]+(.+)$", text, re.I | re.S)
    return found[1] if found else None

def acknowledgement(result):
    action, outcome = result.get("action", "command"), result.get("outcome", "not_confirmed")
    if outcome == "already":
        return "Already set, Captain."
    if outcome == "done":
        return action.replace("_", " ").capitalize() + (" on." if result.get("after") else " off.")
    if outcome == "pressed_unverified":
        name = {"heat_sink":"Heat sink", "chaff":"Chaff", "shield_cell":"Shield cell"}.get(action,"Control")
        return random.choice([name + " control pressed.", name + " input sent, Captain.", name + " key pressed; unverified."])
    if outcome == "busy":
        return "Cockpit controls busy, Captain. Say stop to cancel."
    if outcome == "unbound":
        return "Keyboard secondary binding needed, Captain. " + result.get("reason", "")
    if outcome == "refused":
        return "Can't do that, Captain: " + result.get("reason", "state check failed") + "."
    return "Not confirmed, Captain. I won't repeat it."

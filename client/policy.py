"""Explicit, bounded user requests authorize controls; model output cannot grant permission."""
import re
import time

READ = frozenset({"status", "cargo", "market", "nav_route", "recent_events", "read_guide",
                  "trade_commodities", "find_commodity_market", "plan_trade_route", "screenshot"})
CONTROL = frozenset({"plot_route", "nav_target_start", "nav_target_move", "nav_target_confirm",
                     "nav_target_cancel", "press_binding"})
MENU_KEYS = frozenset({"FocusLeftPanel", "UI_Up", "UI_Down", "UI_Left", "UI_Right", "UI_Select",
                      "UI_Back", "UI_NextPanel", "UI_PrevPanel", "CycleNextPanel", "CyclePreviousPanel"})


def normalized(text):
    return " ".join(re.findall(r"[\w'-]+", text.casefold()))


class Permit:
    def __init__(self, text, spoken=False, enabled=False):
        self.text = normalized(text)
        self.enabled = enabled
        self.started = time.monotonic()
        self.calls = 0
        self.failed = False
        self.observed = False
        self.nav_session = None
        self.station = None
        self.plotted = False
        uncertain = re.search(r"not sure|don't remember|do not remember|maybe|which number|can't remember", self.text)
        discussion = re.search(r"\b(explain|describe|hypothetical|example|don't|do not|without|never|how|what happens|would happen)\b", self.text)
        addressed = not spoken or bool(re.match(r"^(?:hey |okay |ok )?holt\b", self.text))
        self.route = bool(re.search(r"\b(plot|chart|set|lay)\b.*\b(course|route)\b", self.text))
        self.nav = bool(re.search(r"\b(target|lock|unlock|supercruise assist)\b", self.text))
        self.docking = bool(re.search(r"\b(request|ask for) docking\b", self.text))
        self.authorized = bool(enabled and addressed and not uncertain and not discussion and (self.route or self.nav or self.docking))

    def check(self, name, arguments):
        if name in READ:
            return
        if name not in CONTROL or not self.authorized or self.failed:
            raise ValueError("No explicit, unambiguous command authorizes this control")
        if self.calls >= 8 or time.monotonic() - self.started > 120:
            raise ValueError("The bounded action permit has expired")
        if name == "plot_route":
            target = normalized(arguments.get("system", ""))
            requested = re.search(r"\b(?:course|route) (?:to|for) (.+)$", self.text)
            exact = re.sub(r"(?: please| captain| thanks)+$", "", requested[1]) if requested else ""
            if not self.route or self.plotted or not target or target != exact:
                raise ValueError("Destination must appear explicitly in the Captain's request")
            self.plotted = True
        elif name == "nav_target_start":
            target = normalized(arguments.get("station", ""))
            if not self.nav or not target or f" {target} " not in f" {self.text} ":
                raise ValueError("Station must appear explicitly in the Captain's request")
            if arguments.get("action") == "supercruise_assist" and "supercruise assist" not in self.text:
                raise ValueError("Assist requires an explicit order")
            self.station = target
        elif name.startswith("nav_target_"):
            if not self.nav or not self.nav_session or arguments.get("session") != self.nav_session:
                raise ValueError("No matching navigation session")
        elif name == "press_binding":
            if not self.docking or not self.observed or arguments.get("binding") not in MENU_KEYS:
                raise ValueError("Docking input requires a current cockpit observation and an allowed menu key")
            if arguments.get("hold_ms", 0) and arguments["hold_ms"] > 1000:
                raise ValueError("Menu holds are limited to one second")
        self.calls += 1

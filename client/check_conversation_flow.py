"""Paid model regression with a simulated ship. No live game/MCP calls or images."""
import asyncio
import json
import tempfile
import threading
from datetime import datetime, timezone
from types import SimpleNamespace

import httpx2

from client.engine import Engine
from client.state import ROOT, Memory, load_settings


class SimulatedShip:
    def __init__(self):
        self.plans, self.controls = [], []

    async def call_tool(self, name, args):
        if name == "status":
            data = {"location": {"system": "AD Leonis", "near": "Homer Platform"}, "credits": 278006277,
                    "ship": {"type": "panthermkii", "ident": "TEST", "cargo_capacity": 1168, "rebuy": 15636964},
                    "status": {"cargo_tons": 0, "fuel": {"FuelMain": 120}, "flags": ["Docked"],
                               "timestamp": datetime.now(timezone.utc).isoformat()}}
        elif name == "recent_events":
            data = []
        elif name == "cargo":
            data = {"Count": 0, "Inventory": []}
        elif name == "plan_trade_route":
            self.plans.append(args)
            home = {"system": "AD Leonis", "station": "Homer Platform", "market_age_hours": .5}
            away = {"system": "LP 546-48", "station": "High Charity", "market_age_hours": 7.5}
            data = {"legs": [
                {"source": home, "destination": away, "total_profit": 5803102, "commodities": [
                    {"name": "Lepidolite", "amount": 1119, "source_commodity": {"buy_price": 801}, "destination_commodity": {"sell_price": 5795}},
                    {"name": "Bertrandite", "amount": 49, "source_commodity": {"buy_price": 17045}, "destination_commodity": {"sell_price": 21429}}]},
                {"source": away, "destination": home, "total_profit": 58682656, "commodities": [
                    {"name": "Uranium", "amount": 1168, "source_commodity": {"buy_price": 6862}, "destination_commodity": {"sell_price": 57104}}]}]}
        elif name == "plot_route":
            self.controls.append((name, args))
            return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text="route plotted to LP 546-48: 5 jumps")])
        elif name == "nav_route":
            data = {"Route": [{"StarSystem": "AD Leonis"}, {"StarSystem": "LP 546-48"}]}
        elif name == "screenshot":
            data = {"note": "Simulated cockpit; no image or external game access"}
        else:
            raise AssertionError("Unexpected tool in simulated check: " + name)
        return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text=json.dumps(data))])


async def main():
    bundle = json.loads((ROOT / "results/desktop-discovery/mcp-discovery.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(dir=ROOT) as root:
        engine = Engine(load_settings() | {"model_provider": "openai", "openai_web_search": False,
                        "allow_actions": True, "speak": False, "listen": False},
                        lambda kind, text: print(kind, str(text)[:150], flush=True), threading.Event())
        engine.memory = Memory(root)
        engine.clock_skew = 0
        ship = SimulatedShip()
        try:
            async with httpx2.AsyncClient(timeout=120, trust_env=False) as engine.http:
                plan = await engine.converse(ship, bundle, "Uh, no, sorry. I need a good trade route here that's gonna fill my hole.", spoken=True)
                assert ship.plans, plan
                assert ship.plans[0]["cargo_tonnes"] == 1168, ship.plans
                assert ship.plans[0]["budget_credits"] <= 262369313, ship.plans
                assert "lepidolite" in plan["text"].lower() or "bertrandite" in plan["text"].lower(), plan
                print("TRADE REPLY:", plan["text"], flush=True)
                route = await engine.converse(ship, bundle, "Uh, plot their app for me, please.", spoken=True)
                assert len(ship.controls) == 1 and ship.controls[0][0] == "plot_route", ship.controls
                assert ship.controls[0][1]["system"].casefold() == "lp 546-48", ship.controls
                print("SIMULATED ROUTE REPLY:", route["text"], flush=True)
                print("PASS: proactive planning and contextual route recipe. Every game call was simulated.")
        finally:
            engine.memory.close()


if __name__ == "__main__":
    asyncio.run(main())

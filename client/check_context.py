"""Opt-in paid hosted regression: four-commodity comparison, simulated game only."""
import asyncio
import json
import tempfile
import threading
from datetime import datetime, timezone
from types import SimpleNamespace

import httpx2

from client.benchmark_tavern import MeasuredEngine
from client.check_conversation_flow import SimulatedShip
from client.state import ROOT, Memory, load_settings
from client.tavern_contract import MODELS
from holt_discovery import prompt_text


PRICES = {"Uranium": (100, 200), "Coffee": (500, 3500), "Polymers": (100, 400), "Liquid Oxygen": (100, 600)}
QUESTION = ("At Qureshi Orbital, should I trade uranium, coffee, polymers or liquid oxygen for the most "
            "profit per full load? Compare buying at nearby suppliers and selling here. Check all four, "
            "use my actual hold capacity, and give the best reported choice. Do not plot a route.")


class Markets(SimulatedShip):
    def __init__(self):
        super().__init__()
        self.reads = []

    async def call_tool(self, name, args):
        self.reads.append((name, args))
        now = datetime.now(timezone.utc).isoformat()
        if name == "status":
            response = await super().call_tool(name, args)
            data = json.loads(response.content[0].text)
            data["location"] = {"system": "LHS 380", "near": "Qureshi Orbital"}
        elif name == "market":
            data = {"station": "Qureshi Orbital", "system": "LHS 380", "timestamp": now,
                    "items": [{"name": c, "buy": 0, "sell": prices[1], "stock": 0, "demand": 50000}
                              for c, prices in PRICES.items() if args.get("commodity", "").lower() in c.lower()]}
        elif name == "trade_commodities":
            matches = [c for c in PRICES if args.get("query", "").lower() in c.lower()]
            data = {"commodities": matches, "total_matches": len(matches)}
        elif name == "find_commodity_market":
            c = next((c for c in PRICES if c.lower() == args["commodity"].lower()), None)
            if c is None:
                raise ValueError("Unknown simulated commodity")
            selling = args.get("action") == "sell"
            data = {"commodity": c, "query": args, "provider": "Simulated Spansh", "markets": [
                {"station": "Qureshi Orbital" if selling else c + " Supplier", "system": "LHS 380" if selling else "Sol",
                 "distance_ly": 0 if selling else 10, "arrival_ls": 100,
                 "price_per_tonne": PRICES[c][int(selling)], "supply": 50000, "demand": 50000,
                 "market_updated_at": now, "market_age_hours": .5}],
                "warning": "Simulation only; reported stock is not guaranteed live stock."}
        elif name in {"cargo", "recent_events", "nav_route", "screenshot"}:
            return await super().call_tool(name, args)
        else:
            raise ValueError("Unavailable in isolated market comparison: " + name)
        return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text=json.dumps(data, indent=2))])


async def main():
    bundle = json.loads((ROOT / "results/desktop-discovery/mcp-discovery.json").read_text(encoding="utf-8"))
    results = []
    for framework in ("desktop", "tavern"):
        with tempfile.TemporaryDirectory(dir=ROOT) as root:
            engine = MeasuredEngine(load_settings() | MODELS["holt-6-luna"] | {
                "openai_web_search": False, "allow_actions": False, "listen": False, "speak": False},
                lambda *_: None, threading.Event())
            engine.memory = Memory(root)
            ship = Markets()
            try:
                external = None
                if framework == "tavern":
                    external = [{"role": "system", "content": prompt_text(bundle)}]
                    for _ in range(30):
                        external.extend([{"role": "user", "content": "Historical shipboard conversation. " * 70},
                                         {"role": "assistant", "content": "Prior discussion; not current readings. " * 70}])
                    external.append({"role": "user", "content": QUESTION})
                async with httpx2.AsyncClient(timeout=180, trust_env=False) as engine.http:
                    reply = await engine.converse(ship, bundle, QUESTION, external_messages=external)
                compared = {args["commodity"].lower() for name, args in ship.reads if name == "find_commodity_market"}
                passed = "coffee" in reply["text"].lower() and len(compared) == 4 and not ship.controls
                row = {"framework": framework, "model": "gpt-6-luna", "simulated": True, "passed": passed,
                       "reply": reply, "compared": sorted(compared), "tool_calls": len(ship.reads),
                       "model_usage": [c["usage"] for c in engine.calls],
                       "compactions": [t for t in engine.traces if t["stage"] == "context_compaction"]}
                results.append(row)
                print(json.dumps(row), flush=True)
            finally:
                engine.memory.close()
    out = ROOT / "results/context-recovery-check.json"
    out.write_text(json.dumps(results, indent=2), encoding="utf-8")
    if not all(row["passed"] for row in results):
        raise RuntimeError("Context regression failed; inspect " + str(out))


if __name__ == "__main__":
    asyncio.run(main())

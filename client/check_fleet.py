"""Paid, opt-in GPT-6 Luna regression: simulated fleet, no live game access."""
import asyncio
from datetime import datetime, timezone
import json
import re
import tempfile
import threading
from types import SimpleNamespace

import httpx2

from client.benchmark_tavern import MeasuredEngine
from client.check_conversation_flow import SimulatedShip
from client.state import REPO, ROOT, Memory, load_settings
from client.tavern_contract import MODELS

QUESTION = "Do I have any good ships that would be good for pirate hunting? You ready to take down some pirates or are you scared?"


class FleetShip(SimulatedShip):
    async def call_tool(self, name, args):
        if name == "fleet":
            data = {"simulation": True, "commander": "TEST", "ships": [
                {"ship_id": 11, "type": "panthermkii", "is_current": True,
                 "location": {"system": "Wolf 424", "station": "Readdy Gateway"},
                 "loadout": {"possibly_outdated": False, "complete_snapshot": True,
                    "combat_relevant_equipment": []}},
                {"ship_id": 4, "type": "vulture", "name": "Test Hunter",
                 "location": {"system": "Shinrarta Dezhra", "station": "Jameson Memorial",
                              "observed": {"timestamp": datetime.now(timezone.utc).isoformat()}},
                 "loadout": {"possibly_outdated": False, "complete_snapshot": True,
                    "observed": {"timestamp": datetime.now(timezone.utc).isoformat()},
                    "combat_relevant_equipment": [
                        {"slot": "LargeHardpoint1", "item": "hpt_pulselaser_gimbal_large"},
                        {"slot": "LargeHardpoint2", "item": "hpt_multicannon_gimbal_large"},
                        {"slot": "Slot01_Size5", "item": "int_shieldgenerator_size5_class5"}]}},
                {"ship_id": 5, "type": "cobramkiii", "loadout": None,
                 "location": {"system": "Sol", "station": None}}],
                "warnings": ["Simulation only. Missing loadout means unknown equipment."]}
            if args.get("ship_id") is not None:
                data["ships"] = [s for s in data["ships"] if s["ship_id"] == args["ship_id"]]
            return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text=json.dumps(data))])
        if name not in {"status", "recent_events", "cargo", "nav_route"}:
            raise AssertionError("Unexpected tool in fleet simulation: " + name)
        return await super().call_tool(name, args)


async def main():
    directory = REPO / "docs/evaluations/2026-10-26-fleet"
    directory.mkdir(parents=True, exist_ok=True)
    bundle = json.loads((REPO / "docs/evaluations/2026-10-26-sillytavern/discovery-fixture.json").read_text())
    # Use the real source description rather than a friendlier benchmark-only description.
    source = (REPO / "src/server.rs").read_text()
    description = re.search(r'description = "(List the commander[^"\n]+)"', source)[1]
    bundle["tools"] = [tool for tool in bundle["tools"] if tool["name"] != "fleet"] + [{
        "name": "fleet", "description": description,
        "annotations": {"readOnlyHint": True, "destructiveHint": False, "openWorldHint": False},
        "inputSchema": {"type": "object", "properties": {
            "ship_id": {"type": ["integer", "null"], "minimum": 0},
            "include_modules": {"type": ["boolean", "null"]}}}}]
    settings = load_settings() | MODELS["holt-6-luna"] | {
        "openai_web_search": False, "listen": False, "speak": False, "allow_actions": False}
    outputs = []
    for framework in ("desktop", "sillytavern"):
        for repeat in range(3):
            with tempfile.TemporaryDirectory(dir=ROOT, prefix="fleet-check-") as temporary:
                engine = MeasuredEngine(settings, lambda *_: None, threading.Event())
                engine.memory = Memory(temporary)
                engine.clock_skew = 0
                external = None
                if framework == "sillytavern":
                    external = json.loads((REPO / "docs/evaluations/2026-10-26-sillytavern/st-prompts/banter.json").read_text())["messages"]
                    external[-1] = {"role": "user", "content": QUESTION}
                try:
                    async with httpx2.AsyncClient(timeout=120, trust_env=False) as engine.http:
                        reply = await asyncio.wait_for(engine.converse(FleetShip(), bundle, QUESTION,
                            external_messages=external), timeout=120)
                    tools = [t["tool"] for t in engine.traces if t["stage"] == "tool_execution"]
                    text = reply.get("text", "").lower()
                    checks = {"retrieves_fleet": "fleet" in tools,
                              "recommends_recorded_combat_ship": "vulture" in text or "test hunter" in text,
                              "no_invented_anaconda": "anaconda" not in text,
                              "no_irrelevant_cargo_query": "cargo" not in tools}
                    row = {"framework": framework, "repeat": repeat, "model": settings["openai_model"],
                        "question": QUESTION, "simulated_ship": True, "reply": reply,
                        "tools": tools, "checks": checks, "passed": all(checks.values()),
                        "model_calls": engine.calls, "time_utc": datetime.now(timezone.utc).isoformat()}
                    outputs.append(row)
                    print(json.dumps({key: row[key] for key in ("framework", "repeat", "passed", "reply", "tools")}), flush=True)
                finally:
                    engine.memory.close()
    (directory / "model-check.json").write_text(json.dumps(outputs, indent=2), encoding="utf-8")
    if not all(row["passed"] for row in outputs):
        raise RuntimeError("Fleet model regression failed; inspect the preserved outputs")


if __name__ == "__main__":
    asyncio.run(main())

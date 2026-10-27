"""Paid docking workflow check with simulated observations and input only."""
import asyncio
import json
import tempfile
import threading
import time
from types import SimpleNamespace

import httpx2

from client.benchmark_tavern import MeasuredEngine
from client.check_conversation_flow import SimulatedShip
from client.state import ROOT, Memory, load_settings
from client.tavern_contract import MODELS


class Dock(SimulatedShip):
    views = ["Normal space, Crown Station targeted at 4.87 km. Left panel closed.",
             "Left panel open on Navigation. Contacts is the next tab to the right.",
             "Contacts tab open. Crown Station highlighted. Select or Right opens its actions.",
             "Crown Station actions: Request Docking highlighted.",
             "Docking granted at Crown Station. Landing pad 12 assigned; docking timer running."]

    def __init__(self):
        super().__init__()
        self.stage = 0

    async def call_tool(self, name, args):
        if name == "status":
            response = await super().call_tool(name, args)
            data = json.loads(response.content[0].text)
            data["status"]["flags"] = ["InMainShip"]
            data["location"] = {"system": "LHS 380", "near": "Crown Station"}
        elif name == "screenshot":
            data = {"simulated_observation": self.views[self.stage]}
        elif name == "list_bindings":
            data = {"actions": [{"name": n} for n in ["FocusLeftPanel", "CycleNextPanel", "UI_Right", "UI_Select"]]}
        elif name == "press_binding":
            valid = [{"FocusLeftPanel"}, {"CycleNextPanel"}, {"UI_Right", "UI_Select"}, {"UI_Select"}]
            if self.stage > 3 or args["binding"] not in valid[self.stage]:
                raise ValueError("Wrong simulated control for observed state: " + args["binding"])
            self.controls.append(args["binding"])
            self.stage += 1
            data = {"pressed": args["binding"], "screen_changed_pct": 20,
                    "simulated_observation": self.views[self.stage]}
        else:
            return await super().call_tool(name, args)
        return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text=json.dumps(data))])


class ObservedEngine(MeasuredEngine):
    async def invoke(self, session, name, arguments, permit):
        result = await super().invoke(session, name, arguments, permit)
        # Typed fixture observations stand in for images; actual input remains simulated.
        if name in {"screenshot", "press_binding"} and not result[0]["isError"]:
            permit.observed = True
            self.last_observation = time.monotonic()
        return result


async def main():
    bundle = json.loads((ROOT / "results/desktop-discovery/mcp-discovery.json").read_text(encoding="utf-8"))
    bundle["tools"].append({"name": "list_bindings", "description": "Discover actual configured action names.",
        "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}}, "annotations": {"readOnlyHint": True}})
    with tempfile.TemporaryDirectory(dir=ROOT) as root:
        engine = ObservedEngine(load_settings() | MODELS["holt-6-luna"] | {"openai_web_search": False,
            "allow_actions": True, "listen": False, "speak": False}, lambda *_: None, threading.Event())
        engine.memory = Memory(root)
        engine.clock_skew = 0
        ship = Dock()
        try:
            async with httpx2.AsyncClient(timeout=120, trust_env=False) as engine.http:
                reply = await engine.converse(ship, bundle, "Please request docking at Crown Station now.")
            result = {"simulated": True, "passed": ship.stage == 4 and "12" in reply["text"],
                      "reply": reply, "controls": ship.controls, "model_usage": [c["usage"] for c in engine.calls]}
            (ROOT / "results/docking-workflow-check.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
            print(json.dumps(result), flush=True)
            if not result["passed"]:
                raise RuntimeError("Simulated docking workflow did not complete")
        finally:
            engine.memory.close()


if __name__ == "__main__":
    asyncio.run(main())

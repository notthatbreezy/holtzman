"""Offline integration checks: basic facts arrive without model tool calls."""
import asyncio
import json
import tempfile
import threading
from datetime import datetime, timezone
from types import SimpleNamespace

from client.engine import Engine
from client.state import ROOT, Memory, load_settings


class Ship:
    def __init__(self):
        self.reads = 0
        self.failed = False

    async def call_tool(self, name, arguments):
        if self.failed:
            raise RuntimeError("Synthetic unavailable ship link")
        if name == "recent_events":
            data = []
        else:
            assert name == "status", "No game controls permitted"
            self.reads += 1
            data = {"location": {"system": "Sol"}, "status": {
                "timestamp": datetime.now(timezone.utc).isoformat(), "flags": [],
                "fuel": {"FuelMain": 41 - self.reads}}}
        return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text=json.dumps(data))])


class Model:
    def __init__(self):
        self.contexts = []

    async def post(self, url, json):
        context = __import__("json").loads(json["messages"][0]["content"].rsplit("\n", 1)[-1])
        self.contexts.append(context)
        # Never propose status or any other tool. Facts must already be present.
        text = f"Main tank: {context['fuel_main_tonnes']} tonnes." if context["available"] else "The instrument link is unavailable."
        payload = {"choices": [{"finish_reason": "stop", "message": {
            "content": __import__("json").dumps({"text": text, "delivery": "normal"})}}]}
        return SimpleNamespace(raise_for_status=lambda: None, json=lambda: payload)


async def main():
    with tempfile.TemporaryDirectory(dir=ROOT) as root:
        engine = Engine(load_settings() | {"allow_actions": False, "model_provider": "local"}, lambda *_: None, threading.Event())
        engine.memory = Memory(root)
        engine.http = Model()
        async def count(*_):
            return 100
        engine.token_count = count
        names = ("status", "plot_route", "record_click_target", "cursor_position", "future_read_tool")
        bundle = {"initialize": {}, "tools": [{"name": n, "inputSchema": {"type": "object"}} for n in names], "ship_computer": {"messages": []}}
        ship = Ship()
        await engine.converse(ship, bundle, "How much fuel do we have?")
        assert set(names).issubset(engine.schemas), "Advertised tools must stay visible even with game controls disabled"
        await engine.converse(ship, bundle, "And now?")
        assert [c["fuel_main_tonnes"] for c in engine.http.contexts] == [40, 39]
        ship.failed = True
        await engine.converse(ship, bundle, "And now?")
        assert engine.http.contexts[-1]["available"] is False
        assert "fuel_main_tonnes" not in engine.http.contexts[-1]
        assert len(engine.memory.recent()) == 3
        engine.memory.close()
    print("Automatic-context checks pass: current facts on each request without model tool calls; failure does not reuse stale readings. No external calls.")


if __name__ == "__main__":
    asyncio.run(main())

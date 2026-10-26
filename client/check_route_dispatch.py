"""Offline route-dispatch regression: fake MCP responses, zero game/model calls."""
import asyncio
import json
import tempfile
import threading
from types import SimpleNamespace

from client.engine import Engine
from client.state import ROOT, Memory, load_settings


class FakeShip:
    def __init__(self, text, error=False):
        self.text, self.error, self.calls = text, error, []

    async def call_tool(self, name, arguments):
        self.calls.append((name, arguments))
        if name == "status":
            text, error = json.dumps({"status": {"flags": ["FsdMassLocked"]}}), False
        elif name == "screenshot":
            text, error = "Synthetic cockpit observation", False
        elif name == "plot_route":
            text, error = self.text, self.error
        else:
            raise AssertionError("Unexpected tool: " + name)
        return SimpleNamespace(is_error=error, content=[SimpleNamespace(type="text", text=text)])


async def main():
    bundle = {"tools": [{"name": "plot_route", "inputSchema": {
        "type": "object", "properties": {"system": {"type": "string"}}, "required": ["system"]}}]}
    question = "So, little good news bad news here. I was supposed to go to AD Leonis and not Ross 248. Can you plot me a route to AD Leonis please?"
    with tempfile.TemporaryDirectory(dir=ROOT) as root:
        engine = Engine(load_settings() | {"allow_actions": True, "model_provider": "local"}, lambda *_: None, threading.Event())
        engine.memory = Memory(root)
        engine.clock_skew = 0
        for text, error, confirmed in [("route plotted to AD Leonis: 14 jumps", False, True),
                                       ("click target not recorded", True, False),
                                       ("dry run: would plot to AD Leonis (8 steps)", False, False)]:
            ship = FakeShip(text, error)
            reply = await engine.converse(ship, bundle, question, spoken=True)
            assert ("Route plotted to" in reply["text"]) == confirmed, reply
            assert [a for n, a in ship.calls if n == "plot_route"] == [{"system": "ad leonis"}]
            assert all(n != "nav_route" for n, _ in ship.calls)
        engine.clock_skew = 999
        ship = FakeShip("must not execute")
        reply = await engine.converse(ship, bundle, question, spoken=True)
        assert not any(n == "plot_route" for n, _ in ship.calls)
        assert "clocks" in reply["text"]
        engine.memory.close()
    print("Route dispatch checks pass: exact spoken request, one recipe, errors and dry run not called success, clock failure sends no inputs. No game/model calls.")


if __name__ == "__main__":
    asyncio.run(main())

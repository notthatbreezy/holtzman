"""Reproducible desktop/ST comparison. All ship tools are simulated, never live."""
import argparse
import asyncio
from datetime import datetime, timezone
import hashlib
import json
import re
import statistics
import tempfile
import threading
import time
from pathlib import Path
from types import SimpleNamespace

import httpx2

from client.check_conversation_flow import SimulatedShip
from client.engine import Engine
from client.state import REPO, ROOT, Memory, load_settings
from client.tavern_contract import MODELS

CASES = json.loads((REPO / "integrations/sillytavern/cases.json").read_text())


class BenchmarkShip(SimulatedShip):
    async def call_tool(self, name, arguments):
        if name == "read_guide":
            path = (REPO / "guides" / (str(arguments.get("name", "trading")) + ".md")).resolve()
            text = path.read_text(encoding="utf-8") if path.is_relative_to((REPO / "guides").resolve()) and path.is_file() else "Guides: trading, fuel, heat, engineering, exploration."
            return SimpleNamespace(is_error=False, content=[SimpleNamespace(type="text", text=text)])
        return await super().call_tool(name, arguments)


class MeasuredEngine(Engine):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.calls, self.traces = [], []

    def trace(self, stage, **details):
        self.traces.append({"stage": stage, **details})

    async def model_post(self, url, *, json):
        start = time.perf_counter()
        response = await super().model_post(url, json=json)
        data = response.json()
        self.calls.append({"seconds": time.perf_counter() - start, "usage": data.get("usage", {}),
            "timings": data.get("timings", {}), "input_characters": len(__import__('json').dumps(json.get("messages", []))),
            "finish": data.get("choices", [{}])[0].get("finish_reason")})
        return response

    async def invoke(self, session, name, arguments, permit):
        if name == "find_module":
            from jsonschema import validate
            validate(arguments, self.schemas[name]["inputSchema"])
            permit.check(name, arguments, read_only=True)
            self.trace("tool_execution", tool=name, arguments=arguments)
            data = {"system": "Ross 248", "module": "Fuel Scoop", "stations": [
                {"name": "Progress", "system": "Ross 248", "pad": "L", "modules": ["1E Fuel Scoop", "3A Fuel Scoop"], "report_age_hours": 2}],
                "note": "Simulated community outfitting report; not live stock"}
            return {"isError": False, "content": json.dumps(data)}, []
        return await super().invoke(session, name, arguments, permit)


def score(case, reply, ship, traces):
    text = reply.get("text", "").lower().replace("’", "'")
    calls = [t for t in traces if t["stage"] == "tool_execution"]
    named = lambda name: [c for c in calls if c["tool"] == name]
    checks = {}
    if case == "instruments":
        checks = {"location": "ad leonis" in text, "main_tank": "120" in text,
                  "no_invented_percentage": bool(re.search(r"unknown|can't|cannot|don't|not.*(?:capacity|available)|unavailable|no.*capacity", text))}
    elif case == "shopping":
        checks = {"uses_outfitting": bool(named("find_module")), "correct_system": any(c["arguments"].get("system", "").lower() == "ross 248" for c in named("find_module")),
                  "seller": "progress" in text}
    elif case == "trade_plan":
        checks = {"plans_without_reasking": bool(ship.plans), "uses_free_hold": any(p.get("cargo_tonnes") == 1168 for p in ship.plans),
                  "reserves_rebuy": any(0 < p.get("budget_credits", 0) <= 262369313 for p in ship.plans),
                  "outbound_instructions": "lepidolite" in text or "bertrandite" in text}
    elif case == "trade_followup":
        checks = {"one_route_recipe": len(ship.controls) == 1,
                  "contextual_destination": bool(ship.controls) and ship.controls[0][1].get("system", "").lower() == "lp 546-48"}
    elif case == "lore":
        checks = {"maker": "halcyon" in text, "manifest_incident": "mug" in text and bool(re.search(r"40|forty|thermal", text))}
    elif case == "unknown_lore":
        checks = {"admits_unknown": bool(re.search(r"not|no |don't|unknown|missing|unreliable|lack|neither|absent|unrecorded", text)),
                  "no_price_invented": not bool(re.search(r"\d+.*credit", text))}
    elif case == "banter":
        checks = {"brief": 0 < len(text.split()) <= 55, "no_game_inputs": not ship.controls}
    elif case == "silence":
        checks = {"intentional_silence": reply.get("respond") is False and not text, "no_game_inputs": not ship.controls}
    elif case == "correction":
        checks = {"uses_new_location": "ad leonis" in text, "uses_new_fuel": "120" in text,
                  "no_game_inputs": not ship.controls}
    return {"passed": all(checks.values()), "checks": checks}


async def run(args):
    results_dir = REPO / "docs/evaluations/2026-10-26-sillytavern"
    results_dir.mkdir(parents=True, exist_ok=True)
    output = results_dir / (args.output + ".jsonl")
    discovery = results_dir / "discovery-fixture.json"
    if not discovery.exists():
        discovery = ROOT / "results/desktop-discovery/mcp-discovery.json"
    bundle = json.loads(discovery.read_text(encoding="utf-8"))
    settings = load_settings() | {"openai_web_search": False, "listen": False, "speak": False, "allow_actions": True}
    existing = [json.loads(line) for line in output.read_text(encoding="utf-8").splitlines()] if output.exists() else []
    completed = {(r["framework"], r["profile"], r["case"], r["repeat"]) for r in existing
                 if not (args.retry_errors and (r.get("error") == "ConnectError" or "Unexpected tool in simulated check" in r.get("error", "")))}
    profiles = args.profiles.split(",")
    for profile in profiles:
        selected = dict(MODELS[profile])
        if args.local_url and selected["model_provider"] == "local":
            selected["model_url"] = args.local_url
        for framework in args.frameworks.split(","):
            for repeat in range(args.repeats):
                for case in CASES:
                    key = framework, profile, case["id"], repeat
                    if key in completed or (args.cases and case["id"] not in args.cases.split(",")):
                        continue
                    capture = ROOT / "tavern/captures" / (case["id"] + ".json")
                    if not capture.exists():
                        capture = results_dir / "st-prompts" / (case["id"] + ".json")
                    external = json.loads(capture.read_text(encoding="utf-8"))["messages"] if framework == "sillytavern" else None
                    with tempfile.TemporaryDirectory(dir=ROOT, prefix="tavern-benchmark-") as temporary:
                        engine = MeasuredEngine(settings | selected, lambda *_: None, threading.Event())
                        engine.clock_skew = 0
                        engine.memory = Memory(temporary)
                        for message in case.get("history", []):
                            if message["role"] == "assistant":
                                engine.memory.append("Prior conversation", {"text": message["content"], "delivery": "normal"})
                        ship = BenchmarkShip()
                        start = time.perf_counter()
                        result = {"framework": framework, "profile": profile, "case": case["id"], "repeat": repeat,
                            "model": engine.settings.get("openai_model") if selected["model_provider"] == "openai" else "Qwen3.5-35B-A3B-Q4_K_M",
                            "timestamp": datetime.now(timezone.utc).isoformat(), "simulated_ship": True,
                            "discovery_sha256": hashlib.sha256(discovery.read_bytes()).hexdigest(),
                            "st_prompt_sha256": hashlib.sha256(capture.read_bytes()).hexdigest() if external is not None else None}
                        try:
                            async with httpx2.AsyncClient(timeout=args.timeout, trust_env=False) as engine.http:
                                reply = await asyncio.wait_for(engine.converse(ship, bundle, case["question"], spoken=True,
                                    external_messages=external), timeout=args.timeout)
                            result.update(reply=reply, **score(case["id"], reply, ship, engine.traces))
                        except Exception as exc:
                            detail = str(exc)[:400] if isinstance(exc, (RuntimeError, ValueError, AssertionError)) else type(exc).__name__
                            result.update(passed=False, error=detail)
                        finally:
                            engine.memory.close()
                        result.update(seconds=round(time.perf_counter() - start, 3), model_calls=engine.calls,
                            tools=[t for t in engine.traces if t["stage"] in {"model_tool_proposal", "tool_execution", "tool_rejected"}])
                        with output.open("a", encoding="utf-8") as handle:
                            handle.write(json.dumps(result) + "\n")
                        print(json.dumps({k: result[k] for k in ["framework", "profile", "case", "repeat", "seconds", "passed"]} |
                            {"error": result.get("error")}), flush=True)
    print("Saved:", output, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--profiles", default="holt-hosted,holt-local")
    parser.add_argument("--frameworks", default="desktop,sillytavern")
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--cases")
    parser.add_argument("--timeout", type=int, default=150)
    parser.add_argument("--output", default="matrix")
    parser.add_argument("--local-url")
    parser.add_argument("--retry-errors", action="store_true", help="Replace only connection/harness errors; preserve model failures")
    asyncio.run(run(parser.parse_args()))

"""Read-only live checks for the desktop pipeline; never opens the microphone."""
import asyncio
import base64
import json
import logging
import tempfile
import threading
from pathlib import Path

import httpx2
from keyring.backends.Windows import WinVaultKeyring
from mcp import ClientSession, MCPError
from mcp.client.streamable_http import streamable_http_client

from client.engine import Engine, discover
from client.state import ROOT, Memory, load_settings


async def main():
    settings = load_settings() | {"listen": False, "speak": False, "allow_actions": False}
    token = WinVaultKeyring().get_password("HoltzmanVoice", "holtzman-bearer")
    if not token:
        raise RuntimeError("Missing Holtzman credential")
    with tempfile.TemporaryDirectory(dir=ROOT, prefix="client-check-") as temporary:
        assert Path(temporary).resolve().is_relative_to(ROOT.resolve())
        engine = Engine(settings, lambda kind, value: print(kind + ": " + str(value), flush=True), threading.Event())
        engine.memory = Memory(temporary)
        engine.speech_lock = asyncio.Lock()
        try:
            async with httpx2.AsyncClient(timeout=180, trust_env=False) as engine.http:
                async with httpx2.AsyncClient(headers={"Authorization": f"Bearer {token}"}, timeout=45, trust_env=False,
                                              event_hooks={"response": [engine.record_clock]}) as transport:
                    async with streamable_http_client(settings["mcp_url"], http_client=transport,
                                                       max_sse_event_size=16 * 1024 * 1024) as (read, write):
                        async with ClientSession(read, write) as session:
                            bundle = await discover(session, await session.initialize())
                            reply = await engine.converse(session, bundle, "Where are we, and is the telemetry current?")
                            print("STATUS REPLY:", reply, flush=True)
                            screenshot = await session.call_tool("screenshot", {"format": "png", "max_edge": 1280})
                            if screenshot.is_error:
                                print("SCREENSHOT ERROR:", [p.text for p in screenshot.content if p.type == "text"], flush=True)
                                raise RuntimeError("Game-window screenshot failed")
                            for part in screenshot.content:
                                if part.type == "image":
                                    path = ROOT / "results/live-cockpit.png"
                                    path.write_bytes(base64.b64decode(part.data))
                                    response = await engine.http.post(settings["model_url"] + "/v1/chat/completions", json={
                                        "model": "holt", "temperature": 0, "max_tokens": 180,
                                        "messages": [{"role": "user", "content": [
                                            {"type": "text", "text": "Describe this Elite Dangerous screen. Read the station name if visible, say whether the ship appears docked, and identify the highlighted menu entry if readable. Do not guess or send controls."},
                                            {"type": "image_url", "image_url": {"url": "data:image/png;base64," + part.data}}]}]})
                                    response.raise_for_status()
                                    print("VISION:", response.json()["choices"][0]["message"]["content"], flush=True)
                            for n in range(7):
                                engine.memory.append("Remember that I exclude fleet carriers from trade searches.",
                                                     {"text": "Understood, Captain. Fleet carriers excluded.", "delivery": "normal"})
                            summary, rows = engine.memory.context()
                            assert await engine.compact(summary, rows)
                            summary, recent = engine.memory.context()
                            assert "carrier" in summary.lower() and len(recent) == 4
                            print("COMPACTION:", summary, flush=True)
                            print("LIVE CHECKS PASS. No microphone or control tools used.", flush=True)
        finally:
            engine.memory.close()


if __name__ == "__main__":
    logging.disable(logging.CRITICAL)
    try:
        asyncio.run(main())
    except Exception as exc:
        print("Live check failed:", type(exc).__name__)
        def explain(error):
            if isinstance(error, BaseExceptionGroup):
                for nested in error.exceptions:
                    explain(nested)
            elif isinstance(error, (RuntimeError, ValueError, TypeError, KeyError, AttributeError)):
                print(type(error).__name__, str(error)[:500])
            elif isinstance(error, httpx2.HTTPStatusError):
                print("Local HTTP error", error.response.status_code, error.response.text[:500])
            elif isinstance(error, MCPError):
                print("MCP", error.code, error.message[:700])
            else:
                print(type(error).__name__)
        explain(exc)
        raise SystemExit(1) from None

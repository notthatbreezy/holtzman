"""Read-only evidence snapshots around a user-authorized live route test."""
import argparse
import asyncio
import base64
from datetime import datetime, timezone
import json

import httpx2
from keyring.backends.Windows import WinVaultKeyring
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client

from client.state import REPO, ROOT, load_settings


async def main(phase, screenshot=False):
    token = WinVaultKeyring().get_password("HoltzmanVoice", "holtzman-bearer")
    if not token:
        raise RuntimeError("Missing ship credential")
    async with httpx2.AsyncClient(headers={"Authorization": "Bearer " + token}, timeout=60, trust_env=False) as http:
        async with streamable_http_client(load_settings()["mcp_url"], http_client=http, max_sse_event_size=16 * 1024 * 1024) as (read, write):
            async with ClientSession(read, write) as ship:
                await ship.initialize()
                status, route = await ship.call_tool("status", {}), await ship.call_tool("nav_route", {})
                if status.is_error or route.is_error:
                    raise RuntimeError("Live evidence read failed")
                facts = json.loads(next(p.text for p in status.content if p.type == "text"))
                route_text = "\n".join(p.text for p in route.content if p.type == "text")
                result = {"phase": phase, "time_utc": datetime.now(timezone.utc).isoformat(),
                    "location": facts.get("location"), "flags": facts.get("status", {}).get("flags"),
                    "destination": facts.get("status", {}).get("destination"), "nav_route": route_text}
                out = REPO / "docs/evaluations/2026-10-26-sillytavern"
                out.mkdir(parents=True, exist_ok=True)
                (out / ("live-" + phase + ".json")).write_text(json.dumps(result, indent=2), encoding="utf-8")
                print(json.dumps(result), flush=True)
                if screenshot:
                    image = await ship.call_tool("screenshot", {"format": "png", "max_edge": 1280})
                    for part in image.content:
                        if part.type == "image":
                            path = ROOT / "results/tavern-live-cockpit.png"
                            path.write_bytes(base64.b64decode(part.data))
                            print("Cockpit capture:", path)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=["before", "after", "restored"])
    parser.add_argument("--screenshot", action="store_true")
    args = parser.parse_args()
    asyncio.run(main(args.phase, args.screenshot))

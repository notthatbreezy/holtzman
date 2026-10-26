"""Live loopback smoke check. Read-only ship access and small paid model requests."""
import asyncio
from datetime import datetime, timezone
import json
import time

import httpx2

from client.state import REPO


async def main():
    result = {"time_utc": datetime.now(timezone.utc).isoformat()}
    async with httpx2.AsyncClient(base_url="http://127.0.0.1:8082", trust_env=False, timeout=90) as http:
        health = await http.get("/health")
        health.raise_for_status()
        models = (await http.get("/v1/models")).json()["data"]
        assert len(models) == 8 and any(m["id"] == "holt-6-luna" for m in models)
        result["model_catalog"] = True
        blocked = await http.post("/control", headers={"Origin": "https://foreign.example"}, json={"action": "start"})
        assert blocked.status_code == 403
        result["foreign_origin_rejected"] = True
        async with http.stream("GET", "/events?client=smoke-check") as response:
            response.raise_for_status()
            assert response.headers["content-type"].startswith("text/event-stream")
            async for line in response.aiter_lines():
                if line.startswith(": heartbeat") or line.startswith("data:"):
                    result["event_stream"] = True
                    break
        (await http.post("/control", json={"action": "controls", "enabled": False})).raise_for_status()
        (await http.post("/control", json={"action": "start", "listen": False, "speak": False})).raise_for_status()
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            state = (await http.get("/control")).json()
            if state["connected"]:
                break
            await asyncio.sleep(1)
        assert state["connected"], state["state"]
        result["mcp_connected"] = True
        folder = REPO / "docs/evaluations/2026-10-26-sillytavern/st-prompts"
        async def request(case, mode="chat"):
            messages = json.loads((folder / (case + ".json")).read_text(encoding="utf-8"))["messages"]
            response = await http.post("/v1/chat/completions", json={"model": "holt-6-luna", "messages": messages,
                "holt": {"mode": mode, "chat": "http-smoke-check"}})
            response.raise_for_status()
            return response.json()
        answer = await request("lore")
        assert "halcyon" in answer["choices"][0]["message"]["content"].lower(), answer
        result["gpt6_luna_reply"] = answer["choices"][0]["message"]["content"]
        silent = await request("silence")
        assert silent["holt"]["respond"] is False and silent["choices"][0]["message"]["content"] == "\u2063"
        result["silence_without_empty_reply_retry"] = True
        (await http.post("/control", json={"action": "stop"})).raise_for_status()
        state = (await http.get("/control")).json()
        assert not state["running"] and not state["connected"], state["state"]
        result["worker_stop"] = True
        (await http.get("/health")).raise_for_status()
        result["editor_server_survives_worker_stop"] = True
        (await http.post("/control", json={"action": "controls", "enabled": True})).raise_for_status()
    out = REPO / "docs/evaluations/2026-10-26-sillytavern/http-smoke.json"
    out.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result), flush=True)


if __name__ == "__main__":
    asyncio.run(main())

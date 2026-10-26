"""Read-only compatibility check: one verified game-window image, no game inputs."""
import asyncio
import base64
import json

import httpx2
from keyring.backends.Windows import WinVaultKeyring
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client

from client.model_backend import ModelHTTP
from client.state import REPO, ROOT, load_settings


async def main():
    settings = load_settings()
    if not settings.get("allow_hosted_images"):
        raise RuntimeError("Hosted game-window image permission is disabled")
    token = WinVaultKeyring().get_password("HoltzmanVoice", "holtzman-bearer")
    if not token:
        raise RuntimeError("Missing ship credential")
    async with httpx2.AsyncClient(headers={"Authorization": "Bearer " + token}, timeout=60, trust_env=False) as transport:
        async with streamable_http_client(settings["mcp_url"], http_client=transport, max_sse_event_size=16 * 1024 * 1024) as (read, write):
            async with ClientSession(read, write) as ship:
                await ship.initialize()
                result = await ship.call_tool("screenshot", {"format": "png", "max_edge": 1280})
                if result.is_error:
                    raise RuntimeError("Game-window capture unavailable")
                picture = next(p for p in result.content if p.type == "image")
    path = ROOT / "results/tavern-vision.png"
    path.write_bytes(base64.b64decode(picture.data))
    rows = []
    async with httpx2.AsyncClient(timeout=90, trust_env=False) as http:
        for model in ["gpt-5.4-mini", "gpt-5.6-luna", "gpt-6-luna", "gpt-5.6-terra", "gpt-6.1-sol"]:
            backend = ModelHTTP(settings | {"model_provider": "openai", "openai_model": model, "openai_web_search": False}, http)
            response = await backend.post("http://127.0.0.1:8080/v1/chat/completions", json={
                "messages": [{"role": "system", "content": "Describe only what is visibly present in this Elite Dangerous game-window capture. No tool calls, actions or guesses. In at most two sentences, identify the visible screen/menu and transcribe a prominent readable label. If disconnected or at an error/main menu, say so rather than claiming to see a live cockpit."},
                    {"role": "user", "content": [{"type": "image_url", "image_url": {"url": f"data:{picture.mime_type};base64,{picture.data}"}}]}],
                "max_tokens": 400})
            response.raise_for_status()
            data = response.json()
            row = {"model": model, "reply": data["choices"][0]["message"]["content"], "usage": data["usage"], "finish": data["choices"][0]["finish_reason"]}
            rows.append(row)
            print(json.dumps(row), flush=True)
    out = REPO / "docs/evaluations/2026-10-26-sillytavern/vision-compatibility.json"
    out.write_text(json.dumps(rows, indent=2), encoding="utf-8")


if __name__ == "__main__":
    asyncio.run(main())

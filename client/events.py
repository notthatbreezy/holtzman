"""Authenticated SSE transport and thread-safe per-frame PTT state."""
import asyncio
import json
import threading
import time
from urllib.parse import urlsplit, urlunsplit
import httpx2
from client.ship_link import ship_transport

def endpoint(mcp_url, path):
    url = urlsplit(mcp_url)
    return urlunsplit((url.scheme, url.netloc, path, "", ""))

class PushToTalk:
    def __init__(self):
        self.lock = threading.Lock()
        self.connected = self.down = False
        self.active = None
        self.tail_ms, self.max_hold_s = 150, 30
        self.generation = 0
        self.last_seen = 0

    def update(self, data):
        with self.lock:
            self.connected = True
            self.down = bool(data.get("down", False))
            self.active = data.get("input", self.active)
            self.tail_ms = min(2000, max(0, int(data.get("tail_ms", self.tail_ms))))
            self.max_hold_s = min(120, max(1, int(data.get("max_hold_s", self.max_hold_s))))
            self.last_seen = time.monotonic()

    def disconnect(self):
        with self.lock:
            self.connected = self.down = False
            self.generation += 1

    def frame(self):
        with self.lock:
            return {"connected": self.connected and time.monotonic() - self.last_seen < 20,
                    "down": self.down, "tail_ms": self.tail_ms, "max_hold_s": self.max_hold_s,
                    "generation": self.generation, "input": self.active}

async def stream(mcp_url, token, stopped, on_disconnect):
    cursor = None
    backoff = .5
    while not stopped.is_set():
        headers = {"Authorization": f"Bearer {token}"}
        if cursor is not None:
            headers["Last-Event-ID"] = str(cursor)
        try:
            async with httpx2.AsyncClient(transport=ship_transport(mcp_url), trust_env=False,
                                        timeout=httpx2.Timeout(30, read=20), follow_redirects=False) as http:
                async with http.stream("GET", endpoint(mcp_url, "/events"), headers=headers) as response:
                    response.raise_for_status()
                    data, identifier = [], None
                    size = 0
                    async for line in response.aiter_lines():
                        if stopped.is_set():
                            return
                        if not line:
                            if data:
                                event = json.loads("\n".join(data))
                                if not isinstance(event, dict) or not isinstance(event.get("seq"), int):
                                    raise ValueError("Malformed event")
                                cursor = event["seq"]
                                backoff = .5
                                yield event
                            else:
                                yield {"type": "heartbeat"}
                            data, identifier, size = [], None, 0
                        elif line.startswith("data:"):
                            size += len(line)
                            if size > 2 * 1024 * 1024:
                                raise ValueError("Event exceeds bound")
                            data.append(line[5:].lstrip())
                        elif line.startswith("id:"):
                            identifier = line[3:].strip()
        except (httpx2.HTTPError, ValueError, OSError):
            pass
        on_disconnect()
        await asyncio.sleep(backoff)
        backoff = min(10, backoff * 2)

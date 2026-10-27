"""Loopback SillyTavern gateway; the existing HOLT engine remains the tool owner."""
import argparse
import asyncio
import json
import logging
from pathlib import Path
import queue
import subprocess
import threading
import time
import uuid

import httpx2
import uvicorn
from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import JSONResponse, Response, StreamingResponse
from starlette.routing import Route

from client.audio import wav_bytes
from client.engine import Engine
from client.replies import non_speech
from client.services import Services, healthy
from client.state import ROOT, load_settings
from client.tavern_contract import MODELS, ORIGINS, Receipts, normalize_messages, request_key

RUNTIME = ROOT / "sillytavern"
DATA = ROOT / "tavern"


class TavernEngine(Engine):
    def __init__(self, manager, settings):
        super().__init__(settings, manager.report, threading.Event())
        self.manager = manager
        self.memory_root = DATA

    async def connected(self, session, bundle):
        self.manager.connected = True
        self.report("state", "Ready — listening" if self.settings["listen"] else "Ready — text input")
        try:
            while not self.stopped.is_set():
                if self.watcher and self.watcher.done():
                    await self.watcher
                live = time.monotonic() - self.manager.lease < 12 and not self.manager.awaiting_voice
                if live and self.settings["listen"] and not self.busy:
                    self.paused.clear()
                else:
                    self.paused.set()
                try:
                    kind, value = self.inputs.get_nowait()
                except queue.Empty:
                    await asyncio.sleep(.05)
                    continue
                if kind == "audio":
                    if not live or not self.settings["listen"]:
                        continue
                    self.paused.set()
                    self.busy = True
                    self.report("state", "Transcribing…")
                    try:
                        response = await self.http.post(self.settings["transcription_url"] + "/inference",
                            files={"file": ("utterance.wav", wav_bytes(value), "audio/wav")},
                            data={"response_format": "json", "language": "en", "prompt": "Elite Dangerous. HOLT. AD Leonis. Ross 248. Homer Platform."})
                        response.raise_for_status()
                        text = response.json().get("text", "").strip()
                        if text and not non_speech(text):
                            self.manager.awaiting_voice = True
                            self.manager.report("transcript", text)
                    finally:
                        # Keep capture paused while ST constructs and sends this turn.
                        self.busy = False
                    continue
                if kind != "tavern":
                    continue
                body, future = value
                if future.cancelled():
                    continue
                self.busy = True
                self.paused.set()
                self.report("state", "Thinking…")
                original = dict(self.settings)
                try:
                    meta = body.get("holt", {})
                    model = body["model"]
                    selected = MODELS[model]
                    if selected["model_provider"] == "local":
                        await asyncio.to_thread(self.manager.services.start, self.settings | selected, self.stopped)
                    self.settings.update(selected)
                    self.model_http = None
                    self.context_budget = 32768 if selected["model_provider"] == "openai" else self.settings["context_size"]
                    auxiliary = meta.get("mode") != "chat"
                    if auxiliary:
                        self.settings["allow_actions"] = False
                    chat = str(uuid.uuid5(uuid.NAMESPACE_URL, str(meta.get("chat", "tavern"))))
                    self.memory.db.execute("INSERT OR IGNORE INTO sessions(id,created) VALUES(?,?)", (chat, self.memory.now()))
                    self.memory.session = chat
                    self.memory.db.commit()
                    messages = normalize_messages(body["messages"])
                    question = next((m["content"] for m in reversed(messages) if m["role"] == "user"), "")
                    if auxiliary:
                        # Summary/quiet generations never enter the game tool loop or TTS.
                        response = await self.model_post(self.settings["model_url"] + "/v1/chat/completions",
                            json={"model": "holt", "messages": messages, "max_tokens": min(body.get("max_tokens", 700), 1200), "temperature": 0})
                        response.raise_for_status()
                        reply = {"text": response.json()["choices"][0]["message"].get("content", ""), "delivery": "normal"}
                    else:
                        reply = await self.converse(session, bundle, question, external_messages=messages)
                    if not future.cancelled():
                        future.set_result(reply)
                    if not auxiliary and reply.get("respond") is not False:
                        if reply.get("sources"):
                            self.report("sources", reply["sources"])
                        await self.speak(reply["text"], reply.get("delivery", "normal"))
                except asyncio.CancelledError:
                    if not future.done():
                        future.set_exception(RuntimeError("HOLT stopped. A remote recipe may already have executed; no retry was sent."))
                    raise
                except Exception as exc:
                    self.report("error", str(exc)[:700] if isinstance(exc, (RuntimeError, ValueError)) else type(exc).__name__)
                    if not future.done():
                        future.set_exception(RuntimeError(str(exc)[:700]))
                finally:
                    # Preserve a live control toggle made while a request was in progress.
                    controls = self.manager.controls
                    self.settings.clear()
                    self.settings.update(original, allow_actions=controls)
                    self.manager.awaiting_voice = False
                    self.busy = False
                    self.report("state", "Ready — listening" if self.settings["listen"] else "Ready — text input")
        finally:
            self.manager.connected = False


class Manager:
    def __init__(self):
        self.settings = load_settings()
        self.controls = self.settings["allow_actions"]
        self.services = Services(lambda text: self.report("service", text))
        self.engine = self.task = None
        self.events = []
        self.report_lock = threading.Lock()
        self.counter = 0
        self.state = "Stopped"
        self.connected = False
        self.lease = 0
        self.client = None
        self.awaiting_voice = False
        self.receipts = Receipts()
        self.lock = asyncio.Lock()
        self.server = None

    def report(self, kind, value):
        if kind == "image":
            return  # Internal game captures are not duplicated into the browser event log.
        with self.report_lock:
            if kind == "state":
                self.state = str(value)
            self.counter += 1
            self.events.append({"id": self.counter, "kind": kind, "value": value,
                                **({"client": self.client} if kind == "transcript" else {})})
            self.events = self.events[-100:]

    async def start(self, listen=True, speak=True):
        if self.task and not self.task.done():
            return
        self.settings.update(listen=listen, speak=speak, allow_actions=self.controls)
        self.engine = TavernEngine(self, dict(self.settings))
        self.report("state", "Starting HOLT…")

        async def work():
            try:
                # Hosted defaults don't load Qwen. A local profile starts it on demand.
                await asyncio.to_thread(self.services.start, self.engine.settings | {"model_provider": "openai"}, self.engine.stopped)
                await self.engine.run()
            except asyncio.CancelledError:
                pass
            except Exception as exc:
                self.report("error", str(exc)[:700])
            finally:
                await asyncio.to_thread(self.services.stop)
                self.connected = False
                self.report("state", "Stopped")
        self.task = asyncio.create_task(work())

    async def stop(self):
        self.report("state", "Stopping HOLT…")
        self.lease = 0
        self.client = None
        self.awaiting_voice = False
        if self.engine:
            self.engine.cancel()
        if self.task:
            try:
                await asyncio.wait_for(asyncio.shield(self.task), timeout=8)
            except asyncio.TimeoutError:
                self.task.cancel()
                await asyncio.gather(self.task, return_exceptions=True)
            except asyncio.CancelledError:
                pass
        self.engine = self.task = None


manager = Manager()


async def dispatch(request: Request):
    origin = request.headers.get("origin")
    host = request.headers.get("host", "").split(":")[0]
    if host not in {"127.0.0.1", "localhost"} or (origin and origin not in ORIGINS):
        return JSONResponse({"error": "Foreign origin/host rejected"}, status_code=403)
    headers = {"Access-Control-Allow-Origin": origin, "Vary": "Origin"} if origin else {}
    if request.method == "OPTIONS":
        return Response(headers=headers | {"Access-Control-Allow-Methods": "GET, POST, OPTIONS", "Access-Control-Allow-Headers": "Content-Type"})
    try:
        result = await handle(request)
    except (ValueError, KeyError, TypeError) as exc:
        result = JSONResponse({"error": {"message": str(exc)[:400], "type": "invalid_request"}}, status_code=400)
    except Exception as exc:
        # Never serialize HTTP transport headers or credential objects.
        message = str(exc)[:700] if isinstance(exc, RuntimeError) else type(exc).__name__
        result = JSONResponse({"error": {"message": message, "type": "holt_error"}}, status_code=503)
    result.headers.update(headers)
    return result


async def handle(request):
    path = request.url.path
    if request.method == "GET":
        if path == "/health":
            return JSONResponse({"service": "holt-tavern", "ready": True})
        if path == "/v1/models":
            return JSONResponse({"object": "list", "data": [{"id": name, "object": "model", "owned_by": "holt"} for name in MODELS]})
        if path == "/events":
            client = request.query_params.get("client", "")
            if not client or len(client) > 100:
                raise ValueError("A browser client ID is required")
            async def events():
                cursor = int(request.headers.get("last-event-id", "0"))
                if cursor > manager.counter:
                    cursor = 0
                try:
                    while not (manager.server and manager.server.should_exit) and not await request.is_disconnected():
                        # An open event stream keeps speech responsive in background tabs.
                        # Closing the page releases the lease; no throttled timer required.
                        if manager.client == client:
                            manager.lease = time.monotonic()
                        for event in list(manager.events):
                            if event["id"] <= cursor:
                                continue
                            cursor = event["id"]
                            if event["kind"] == "transcript" and event.get("client") != client:
                                continue
                            yield f"id: {event['id']}\ndata: {json.dumps(event)}\n\n"
                        yield ": heartbeat\n\n"
                        await asyncio.sleep(1)
                finally:
                    if manager.client == client:
                        manager.lease = 0
            return StreamingResponse(events(), media_type="text/event-stream", headers={"Cache-Control": "no-cache"})
        if path == "/control":
            after = int(request.query_params.get("after", 0))
            return JSONResponse({"state": manager.state, "connected": manager.connected,
                "running": bool(manager.task and not manager.task.done()), "controls": manager.controls,
                "events": [e for e in manager.events if e["id"] > after and
                           (e["kind"] != "transcript" or e.get("client") == request.query_params.get("client"))]})
        return JSONResponse({"error": "Unknown endpoint"}, status_code=404)
    if request.method != "POST":
        return Response(status_code=405)
    raw = bytearray()
    async for part in request.stream():
        raw.extend(part)
        if len(raw) > 2 * 1024 * 1024:
            return Response(status_code=413)
    body = json.loads(raw)
    if not isinstance(body, dict):
        raise ValueError("Expected a JSON object")
    if path == "/control":
        action = body.get("action")
        for key in ("listen", "speak", "enabled"):
            if key in body and type(body[key]) is not bool:
                raise ValueError(key + " must be a JSON boolean")
        if action == "start":
            await manager.start(bool(body.get("listen", True)), bool(body.get("speak", True)))
        elif action == "stop":
            await manager.stop()
        elif action == "controls":
            manager.controls = bool(body["enabled"])
            if manager.engine:
                manager.engine.settings["allow_actions"] = manager.controls
        elif action == "lease":
            client = str(body["client"])[:100]
            if time.monotonic() - manager.lease < 12 and manager.client != client and body.get("takeover") is not True:
                raise ValueError("Another Tavern tab owns the microphone")
            manager.client, manager.lease = client, time.monotonic()
        elif action == "release":
            if manager.client == body.get("client"):
                manager.client, manager.lease = None, 0
        elif action == "shutdown":
            await manager.stop()
            if manager.server:
                manager.server.should_exit = True
        else:
            raise ValueError("Unknown control action")
        return JSONResponse({"ok": True})
    if path != "/v1/chat/completions":
        return Response(status_code=404)
    messages = normalize_messages(body["messages"])
    meta = body.get("holt", {})
    if not isinstance(meta, dict) or meta.get("mode", "quiet") not in {"chat", "quiet", "capture"}:
        raise ValueError("Invalid HOLT generation metadata")
    if meta.get("mode") == "capture":
        # Built-in benchmark exports ST's real prompt construction without a model/game call.
        case = str(meta.get("case", "capture"))
        if not case.replace("_", "").isalnum() or len(case) > 60:
            raise ValueError("Invalid capture case")
        folder = DATA / "captures"
        folder.mkdir(parents=True, exist_ok=True)
        (folder / (case + ".json")).write_text(json.dumps({"messages": messages, "meta": meta}, indent=2), encoding="utf-8")
        reply = {"text": "Benchmark prompt captured.", "delivery": "normal"}
    else:
        if body["model"] not in MODELS:
            raise ValueError("Select a HOLT model profile")
        if body.get("tools"):
            raise ValueError("HOLT owns the tool loop; disable SillyTavern function calling")
        if not manager.connected or not manager.engine:
            raise RuntimeError("Press Start HOLT and wait for the ship connection")
        if manager.lock.locked() or manager.engine.busy:
            raise RuntimeError("HOLT is busy; the request was not queued or replayed")
        async with manager.lock:
            key = request_key(messages, meta.get("chat", "tavern"))
            cached = manager.receipts.get(key) if meta.get("mode") == "chat" else None
            row = manager.engine.memory.db.execute("SELECT value FROM metadata WHERE key=?", ("receipt:" + key,)).fetchone() if meta.get("mode") == "chat" else None
            if row and not cached:
                saved = json.loads(row[0])
                if time.time() - saved["time"] < 600:
                    cached = saved["reply"]
            if cached:
                reply = cached
            else:
                uncertain = {"text": "This request may already have run. Check the ship before issuing a new order; I have not repeated it.", "delivery": "caution"}
                manager.receipts.put(key, uncertain)
                def receipt(value):
                    db = manager.engine.memory.db
                    db.execute("INSERT OR REPLACE INTO metadata VALUES(?,?)", ("receipt:" + key, json.dumps({"time": time.time(), "reply": value})))
                    db.commit()
                if meta.get("mode") == "chat":
                    receipt(uncertain)
                future = asyncio.get_running_loop().create_future()
                manager.engine.inputs.put(("tavern", (body, future)))
                # The browser's cancellation must cancel this request, never retry it.
                async def disconnected():
                    while not await request.is_disconnected():
                        await asyncio.sleep(.1)
                connection = asyncio.create_task(disconnected())
                done, _ = await asyncio.wait([future, connection], timeout=240, return_when=asyncio.FIRST_COMPLETED)
                connection.cancel()
                if future not in done:
                    future.cancel()
                    manager.engine.cancel()
                    raise RuntimeError("Request interrupted; controls were not replayed. Stop and Start HOLT to reconnect.")
                reply = future.result()
                manager.receipts.put(key, reply)
                if meta.get("mode") == "chat":
                    receipt(reply)
    # ST retries empty model replies. Its extension removes this silent-turn marker.
    text = "\u2063" if reply.get("respond") is False else reply["text"]
    result = {"id": "chatcmpl-holt-" + uuid.uuid4().hex, "object": "chat.completion", "created": int(time.time()),
        "model": body.get("model", "holt-hosted"), "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "holt": {"respond": reply.get("respond", True), "delivery": reply.get("delivery", "normal")}}
    if body.get("stream"):
        async def chunks():
            chunk = {k: v for k, v in result.items() if k != "choices"}
            chunk["object"] = "chat.completion.chunk"
            chunk["choices"] = [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": None}]
            yield "data: " + json.dumps(chunk) + "\n\n"
            chunk["choices"] = [{"index": 0, "delta": {}, "finish_reason": "stop"}]
            yield "data: " + json.dumps(chunk) + "\n\ndata: [DONE]\n\n"
        return StreamingResponse(chunks(), media_type="text/event-stream")
    return JSONResponse(result)


app = Starlette(routes=[Route("/{path:path}", dispatch, methods=["GET", "POST", "OPTIONS", "PUT", "DELETE"])])


def main():
    logging.disable(logging.CRITICAL)
    parser = argparse.ArgumentParser()
    parser.add_argument("command", nargs="?", choices=["start", "stop", "status"], default="start")
    parser.add_argument("--no-browser", action="store_true", help="Internal service/check mode")
    args = parser.parse_args()
    if args.command != "start":
        try:
            with httpx2.Client(trust_env=False, timeout=20) as http:
                response = http.get("http://127.0.0.1:8082/control") if args.command == "status" else http.post("http://127.0.0.1:8082/control", json={"action": "shutdown"})
                response.raise_for_status()
                print(response.text)
        except httpx2.ConnectError:
            print("HOLT Tavern is stopped")
        except httpx2.HTTPError:
            raise SystemExit("HOLT did not confirm the request; inspect the local service log before restarting")
        return
    if healthy("http://127.0.0.1:8082/health"):
        import webbrowser
        if not args.no_browser:
            webbrowser.open("http://127.0.0.1:8000")
        return
    if healthy("http://127.0.0.1:8000"):
        raise RuntimeError("Port 8000 is already occupied; refusing to replace another server")
    node = Path(json.loads((DATA / "installation.json").read_text())["node"])
    logdir = ROOT / "results"
    logdir.mkdir(parents=True, exist_ok=True)
    log = (logdir / "sillytavern.log").open("ab")
    child = subprocess.Popen([str(node), "server.js"], cwd=RUNTIME, stdout=log, stderr=log,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    def open_when_ready():
        import webbrowser
        deadline = time.monotonic() + 90
        while child.poll() is None and time.monotonic() < deadline:
            if healthy("http://127.0.0.1:8000"):
                webbrowser.open("http://127.0.0.1:8000")
                return
            time.sleep(.5)
    if not args.no_browser:
        threading.Thread(target=open_when_ready, daemon=True).start()
    manager.server = uvicorn.Server(uvicorn.Config(app, host="127.0.0.1", port=8082, log_level="error", access_log=False))
    try:
        manager.server.run()
    finally:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
        log.close()


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        from client.diagnostics import record
        record(exc)
        raise

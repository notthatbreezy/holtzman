"""Loopback-only Pocket TTS stream service. No game or reasoning-model access."""
import argparse
import asyncio
import base64
from contextlib import asynccontextmanager
import hashlib
import importlib.metadata
import json
import logging
import os
from pathlib import Path
import queue
import threading
import time
import uuid
from typing import Literal

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
from starlette.background import BackgroundTask
from starlette.middleware.trustedhost import TrustedHostMiddleware
from starlette.responses import StreamingResponse

from client.pocket_effects import PocketEffects

ROOT = Path(__file__).resolve().parents[1]
LANGUAGE = "english_2026-10"
VOICE_FILES = {"k2so": ("HOLT K-2SO", "HOLT-K2SO.wav"),
               "k2so-short": ("HOLT K-2SO — short reference", "HOLT-K2SO-single.wav")}
log = logging.getLogger("holt.pocket")


class SpeechRequest(BaseModel):
    text: str = Field(min_length=1, max_length=6000)
    voice: str = "k2so"
    effect: Literal["original", "warm", "droid", "radio"] = "radio"
    volume: float = Field(default=0.85, ge=0.1, le=1)
    robot_amount: float = Field(default=0.10, ge=0, le=0.5)


class PocketService:
    def __init__(self, threads=2):
        self.threads = threads
        self.model = None
        self.states = {}
        self.lock = threading.Lock()
        self.last_metrics = None
        self.error = None

    def load(self):
        import torch
        from pocket_tts import TTSModel, export_model_state
        torch.set_num_threads(self.threads)
        torch.set_num_interop_threads(1)
        self.model = TTSModel.load_model(language=LANGUAGE)
        if not self.model.has_voice_cloning:
            raise RuntimeError("Pocket cloning access unavailable; authenticate with HF_HOME set to the Pocket cache")
        cache = ROOT / ".local/holt/models/pocket-voices"
        cache.mkdir(parents=True, exist_ok=True)
        for voice, (_, filename) in VOICE_FILES.items():
            reference = ROOT / ".local/holt/chatterbox-server/reference_audio" / filename
            identity = {"reference_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
                        "language": LANGUAGE, "pocket_version": importlib.metadata.version("pocket-tts")}
            state_file, record_file = cache / f"{voice}.safetensors", cache / f"{voice}.json"
            try:
                cached = state_file.is_file() and json.loads(record_file.read_text()) == identity
            except (OSError, ValueError):
                cached = False
            state = self.model.get_state_for_audio_prompt(str(state_file if cached else reference))
            if not cached:
                export_model_state(state, str(state_file))
                record_file.write_text(json.dumps(identity, indent=2), encoding="utf-8")
            self.states[voice] = state

    def health(self):
        return {"service": "holt-pocket-tts", "ready": self.model is not None and bool(self.states) and not self.error,
                "device": "cpu", "threads": self.threads, "language": LANGUAGE,
                "busy": self.lock.locked(), "error": self.error, "last_metrics": self.last_metrics}

    def start_job(self, text, voice, effect="radio", volume=0.85, robot_amount=0.10):
        if voice not in self.states:
            raise HTTPException(400, "Unknown Pocket voice")
        if not text.strip():
            raise HTTPException(422, "Speech text is empty")
        if not self.lock.acquire(blocking=False):
            raise HTTPException(409, "Pocket is already generating speech")
        channel = queue.Queue(maxsize=8)
        stopped = threading.Event()
        request_id = uuid.uuid4().hex
        rate = int(self.model.sample_rate)

        def put(item):
            while not stopped.is_set():
                try:
                    channel.put(item, timeout=0.2)
                    return True
                except queue.Full:
                    pass
            return False

        def worker():
            import numpy as np
            started = time.perf_counter()
            first = None
            samples = chunks = 0
            try:
                effects = PocketEffects(rate, effect, volume, robot_amount)
                for chunk in self.model.generate_audio_stream(self.states[voice], text,
                                                              copy_state=True, stop=stopped):
                    if stopped.is_set():
                        break
                    audio = chunk.detach().cpu().numpy().reshape(-1)
                    if not audio.size:
                        continue
                    if not np.isfinite(audio).all():
                        raise ValueError("Non-finite audio")
                    if first is None:
                        first = time.perf_counter() - started
                    samples += audio.size
                    chunks += 1
                    if samples > rate * 180:
                        raise ValueError("Speech duration limit exceeded")
                    pcm = (effects.process(audio) * 32767).astype("<i2").tobytes()
                    if not put({"type": "audio", "index": chunks - 1, "samples": int(audio.size),
                                "data": base64.b64encode(pcm).decode("ascii")}):
                        break
                if not samples and not stopped.is_set():
                    raise ValueError("No audio generated")
                metrics = {"request_id": request_id, "first_chunk_ms": round((first or 0) * 1000, 1),
                           "generation_ms": round((time.perf_counter() - started) * 1000, 1),
                           "audio_seconds": round(samples / rate, 3), "chunks": chunks,
                           "effect": effect, "volume": volume, "robot_amount": robot_amount}
                self.last_metrics = metrics
                put({"type": "end", **metrics})
                log.info("Speech completed: %s", metrics)
            except Exception:
                log.exception("Pocket generation failed (%s)", request_id)
                put({"type": "error", "message": "Pocket synthesis failed; see the local service log"})
            finally:
                self.lock.release()

        threading.Thread(target=worker, daemon=True, name="pocket-synthesis").start()

        async def stream():
            try:
                yield json.dumps({"type": "start", "request_id": request_id, "sample_rate": rate,
                                  "channels": 1, "format": "s16le"}) + "\n"
                while not stopped.is_set():
                    try:
                        item = await asyncio.to_thread(channel.get, True, 0.5)
                    except queue.Empty:
                        continue
                    yield json.dumps(item, separators=(",", ":")) + "\n"
                    if item["type"] in ("end", "error"):
                        break
            finally:
                stopped.set()
        return stream(), stopped


def create_app(service):
    @asynccontextmanager
    async def lifespan(_app):
        try:
            await asyncio.to_thread(service.load)
        except Exception as exc:
            service.error = type(exc).__name__ + ": " + str(exc)
            log.exception("Pocket startup failed")
        yield

    app = FastAPI(lifespan=lifespan)
    app.add_middleware(TrustedHostMiddleware, allowed_hosts=["127.0.0.1", "localhost"])
    app.add_middleware(CORSMiddleware, allow_origins=["http://localhost:8000", "http://127.0.0.1:8000"],
                       allow_methods=["GET", "POST"], allow_headers=["Content-Type"])

    @app.get("/health")
    def health():
        return service.health()

    @app.get("/voices")
    def voices():
        return [{"id": voice, "name": value[0], "lang": "en-US"} for voice, value in VOICE_FILES.items()]

    @app.post("/stream")
    def speech(body: SpeechRequest):
        if not service.health()["ready"]:
            raise HTTPException(503, "Pocket is not ready; check the local service status")
        stream, stopped = service.start_job(body.text, body.voice, body.effect, body.volume, body.robot_amount)
        return StreamingResponse(stream, media_type="application/x-ndjson",
                                 headers={"Cache-Control": "no-store", "X-Accel-Buffering": "no"},
                                 background=BackgroundTask(stopped.set))
    return app


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8005)
    parser.add_argument("--threads", type=int, default=2)
    args = parser.parse_args()
    if not 1 <= args.threads <= 16:
        parser.error("Use 1 to 16 CPU threads")
    os.environ["HF_HOME"] = str(ROOT / ".local/holt/models/pocket-hf")
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["OMP_NUM_THREADS"] = str(args.threads)
    os.environ["MKL_NUM_THREADS"] = str(args.threads)
    import uvicorn
    logging.basicConfig(level=logging.INFO)
    uvicorn.run(create_app(PocketService(args.threads)), host="127.0.0.1", port=args.port)


if __name__ == "__main__":
    main()

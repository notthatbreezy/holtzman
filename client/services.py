"""Own only this checkout's inference processes; validate PID identity before stopping."""
import json
import subprocess
import time
import os
import threading
from pathlib import Path
from urllib.request import ProxyHandler, build_opener

import psutil

from client.state import ROOT


def matches(record):
    try:
        process = psutil.Process(record["pid"])
        return (Path(process.exe()).resolve() == Path(record["exe"]).resolve()
                and abs(process.create_time() - record["created"]) < 0.1)
    except (psutil.Error, OSError):
        return False


def healthy(url):
    try:
        with build_opener(ProxyHandler({})).open(url, timeout=2) as response:
            return response.status == 200
    except Exception:
        return False


class Services:
    def __init__(self, notify=lambda text: None, root=ROOT):
        self.root = Path(root)
        self.run = self.root / "run"
        self.run.mkdir(parents=True, exist_ok=True)
        self.notify = notify
        self.records = {}
        self.handles = []

    def adopt_existing(self):
        expected = {"model": (self.root / "runtime/llama-server.exe").resolve(),
                    "transcription": (self.root / "runtime/whisper/Release/whisper-server.exe").resolve()}
        for process in psutil.process_iter(["pid", "exe", "create_time"]):
            try:
                path = Path(process.info["exe"]).resolve() if process.info["exe"] else None
                for name, executable in expected.items():
                    if path == executable:
                        if name in self.records:
                            raise RuntimeError(f"Multiple {name} processes exist")
                        self.records[name] = {"pid": process.pid, "exe": str(path), "created": process.create_time()}
            except psutil.Error:
                continue
        self.save()
        return self.records

    def start_one(self, name, executable, arguments, health, stopped):
        executable = executable.resolve()
        if not executable.exists():
            raise RuntimeError(f"Missing {name} runtime: run the client setup first")
        candidates = []
        for process in psutil.process_iter(["pid", "exe", "create_time"]):
            try:
                if process.info["exe"] and Path(process.info["exe"]).resolve() == executable:
                    candidates.append(process)
            except (psutil.Error, OSError):
                continue
        if candidates:
            if len(candidates) != 1:
                raise RuntimeError(f"Multiple {name} processes exist; stop duplicates first")
            process = candidates[0]
            self.notify(f"Using existing {name}; Stop will manage this checkout's process.")
        else:
            if healthy(health):
                raise RuntimeError(f"{name} port belongs to another process; refusing to take it over")
            self.notify(f"Starting {name}…")
            log = (self.root / "results")
            log.mkdir(parents=True, exist_ok=True)
            out = (log / f"desktop-{name}.log").open("ab")
            self.handles.append(out)
            child = subprocess.Popen([str(executable), *arguments], stdout=out, stderr=out,
                                     creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            process = psutil.Process(child.pid)
        record = {"pid": process.pid, "exe": str(executable), "created": process.create_time()}
        self.records[name] = record
        self.save()
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            if stopped.is_set():
                raise InterruptedError()
            if not matches(record):
                raise RuntimeError(f"{name} exited; see its log in .local/holt/results")
            if healthy(health):
                self.notify(f"{name} ready.")
                return
            stopped.wait(0.5)
        raise RuntimeError(f"{name} did not become ready within three minutes")

    def start(self, settings, stopped):
        model = self.root / "models/Qwen3.5-35B-A3B-Q4_K_M.gguf"
        projector = self.root / "models/mmproj-F16.gguf"
        speech = self.root / "models/whisper/ggml-small.en.bin"
        provider = settings.get("model_provider", "local")
        if provider not in ("local", "openai"):
            raise RuntimeError("Unknown model provider")
        for path in ((model, projector, speech) if provider == "local" else (speech,)):
            if not path.exists():
                raise RuntimeError(f"Missing model asset: {path.name}")
        if settings["model_url"] != "http://127.0.0.1:8080" or settings["transcription_url"] != "http://127.0.0.1:8081":
            raise RuntimeError("Managed services require the default loopback endpoints")
        if provider == "local":
            self.start_one("model", self.root / "runtime/llama-server.exe", [
            "-m", str(model), "--mmproj", str(projector), "--image-min-tokens", "1024",
            "--alias", "holt", "--host", "127.0.0.1", "--port", "8080", "-ngl", "99",
            "-c", str(settings["context_size"]), "--parallel", "1", "--jinja", "--reasoning", "off"],
                settings["model_url"] + "/health", stopped)
        self.start_one("transcription", self.root / "runtime/whisper/Release/whisper-server.exe", [
            "-m", str(speech), "--host", "127.0.0.1", "--port", "8081", "-l", "en", "-t", "8", "-ng"],
            settings["transcription_url"] + "/", stopped)

    def save(self):
        temporary = self.run / f"services-{os.getpid()}-{threading.get_ident()}.tmp"
        temporary.write_text(json.dumps(self.records), encoding="utf-8")
        temporary.replace(self.run / "services.json")

    def stop(self):
        path = self.run / "services.json"
        records = json.loads(path.read_text()) if path.exists() else self.records
        for name, record in records.items():
            if matches(record):
                try:
                    process = psutil.Process(record["pid"])
                    children = process.children(recursive=True)
                except psutil.Error:
                    continue
                for child in children:
                    try:
                        child.terminate()
                    except psutil.Error:
                        pass
                try:
                    process.terminate()
                    _, alive = psutil.wait_procs([process, *children], timeout=4)
                    for remaining in alive:
                        remaining.kill()
                except psutil.Error:
                    pass
                self.notify(f"Stopped {name}.")
        self.records = {}
        self.save()
        for handle in self.handles:
            handle.close()
        self.handles.clear()

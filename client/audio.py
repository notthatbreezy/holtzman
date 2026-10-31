"""Continuous, half-duplex voice capture and local speech output."""
import collections
import io
import queue
import threading
import wave
from contextlib import contextmanager

import numpy as np
import sounddevice as sd
import webrtcvad


_audio_lock = threading.RLock()
_audio_users = 0


@contextmanager
def audio_session():
    """Keep device refresh from invalidating an open capture or playback stream."""
    global _audio_users
    with _audio_lock:
        _audio_users += 1
    try:
        yield
    finally:
        with _audio_lock:
            _audio_users -= 1


def refresh_devices():
    """Refresh PortAudio's cached Windows endpoints only before starting audio.

    query_devices alone doesn't re-enumerate reconnected Bluetooth devices. The
    pinned sounddevice library has no public refresh; balance its initialization
    helpers once, never from the microphone retry loop while output may be active.
    """
    with _audio_lock:
        if _audio_users:
            raise RuntimeError("Previous audio stream is still closing; stop HOLT and try Start again")
        sd.stop()
        sd._terminate()
        sd._initialize()


def devices(kind):
    result = []
    for number, device in enumerate(sd.query_devices()):
        if device[f"max_{kind}_channels"]:
            host = sd.query_hostapis(device["hostapi"])["name"]
            result.append((f"{device['name']} [{host}]", number))
    return result


def resolve(selector, kind):
    if selector is None:
        return None
    matches = [number for label, number in devices(kind) if label == selector]
    if len(matches) != 1:
        raise RuntimeError(f"Selected {kind} device is unavailable. Choose another device or Windows default.")
    return matches[0]


def shared_output_settings(device):
    """Let Windows mix Kokoro's 24 kHz speech into a shared WASAPI device format."""
    actual = sd.query_devices(device, "output")
    host = sd.query_hostapis(actual["hostapi"])["name"]
    return sd.WasapiSettings(auto_convert=True) if host == "Windows WASAPI" else None


class Segmenter:
    def __init__(self, silence_ms=850, maximum=25, *, mode="off", tail_ms=150,
                 max_hold_s=30, settings=None, turn_worker=None):
        self.vad = webrtcvad.Vad(2)
        self.pre = collections.deque(maxlen=10)
        self.votes = collections.deque(maxlen=8)
        self.silence_ms, self.maximum = silence_ms, maximum * 1000
        self.mode, self.tail_ms, self.max_hold_s = mode, tail_ms, max_hold_s
        self.settings, self.turn_worker = settings or {}, turn_worker
        self.frames, self.voice_frames, self.quiet = [], 0, 0
        self.held = self.was_down = self.last_held = self.blocked = False
        self.held_ms, self.tail = 0, None
        self.check_at = self.settings.get("min_silence_ms", 200)

    def reset(self):
        self.frames, self.voice_frames, self.quiet = [], 0, 0
        self.pre.clear()
        self.votes.clear()
        self.held, self.held_ms, self.tail = False, 0, None
        self.check_at = self.settings.get("min_silence_ms", 200)
        if self.turn_worker:
            self.turn_worker.cancel()

    def feed(self, frame, ptt_down=False):
        samples = np.frombuffer(frame, dtype=np.int16).astype(np.float32)
        voiced = bool(np.sqrt(np.mean(samples * samples)) > 120 and self.vad.is_speech(frame, 16000))
        edge = ptt_down and not self.was_down
        released = self.was_down and not ptt_down
        self.was_down = ptt_down
        if not ptt_down:
            self.blocked = False
        if self.mode == "off":
            edge = released = False
        if edge and not self.blocked:
            if not self.frames:
                self.frames = list(self.pre)
                self.voice_frames = 0
            self.held, self.tail = True, None
        if not self.frames and not self.held:
            self.pre.append(frame)
            if self.mode == "hold":
                return None
            self.votes.append(voiced)
            if sum(self.votes) >= 5:
                self.frames = list(self.pre)
                self.voice_frames = sum(self.votes)
            return None
        self.frames.append(frame)
        self.voice_frames += int(voiced)
        self.quiet = 0 if voiced else self.quiet + 30
        if voiced and self.turn_worker:
            self.turn_worker.cancel()
            self.check_at = self.settings.get("min_silence_ms", 200)
        if released and self.held:
            self.tail = 0
        if self.held:
            self.held_ms += 30
            if self.tail is not None:
                self.tail += 30
            end = self.tail is not None and self.tail >= self.tail_ms
            if self.held_ms >= self.max_hold_s * 1000:
                end, self.blocked = True, True
        else:
            adaptive = self.settings.get("endpoint_mode") == "adaptive" and self.turn_worker
            decision = None
            if adaptive and self.quiet >= self.check_at:
                decision = self.turn_worker.check(b"".join(self.frames), self.settings.get("turn_threshold", .5))
                self.check_at = self.quiet + self.settings.get("recheck_ms", 100)
            fallback = not adaptive or self.turn_worker.reason is not None
            end = self.quiet >= (self.silence_ms if fallback else self.settings.get("max_silence_ms", 1500)) or decision is True
        end = end or (not self.held and len(self.frames) * 30 >= self.maximum)
        if end:
            self.last_held = self.held
            audio = b"".join(self.frames) if self.voice_frames >= (3 if self.held else 10) else None
            if ptt_down:
                self.blocked = True
            self.reset()
            return audio
        return None


class Microphone:
    def __init__(self, settings, utterances, paused, stopped, report, ptt=None):
        self.settings, self.utterances = settings, utterances
        self.paused, self.stopped, self.report = paused, stopped, report
        self.frames = queue.Queue(maxsize=100)
        self.ptt = ptt

    def run(self):
        while not self.stopped.is_set():
            try:
                self.capture()
            except Exception as exc:
                self.report("microphone", f"Microphone unavailable ({type(exc).__name__}); retrying")
                self.report("error", f"Microphone: {exc}")
                self.stopped.wait(3)

    def capture(self):
        try:
            device = resolve(self.settings["microphone"], "input")
            actual = sd.query_devices(device, "input")
            host = sd.query_hostapis(actual["hostapi"])["name"]
            extra = sd.WasapiSettings(auto_convert=True) if host == "Windows WASAPI" else None
            from client.endpointing import worker
            turn_worker = worker(self.settings, self.report) if self.settings.get("endpoint_mode") == "adaptive" else None
            def new_segmenter():
                state = self.ptt.frame() if self.ptt else {}
                return Segmenter(self.settings["silence_ms"], self.settings["max_utterance_seconds"],
                    mode=self.settings.get("ptt_mode", "off"), tail_ms=state.get("tail_ms", 150),
                    max_hold_s=state.get("max_hold_s", 30), settings=self.settings, turn_worker=turn_worker)
            segmenter = new_segmenter()
            generation = self.ptt.frame()["generation"] if self.ptt else 0

            def callback(data, frames, timing, status):
                state = self.ptt.frame() if self.ptt else {"connected": True, "down": False, "generation": 0}
                barge = self.settings.get("ptt_mode") == "hybrid" and state["connected"] and state["down"]
                if (not self.paused.is_set() or self.settings.get("ptt_mode") == "hold" or barge) and not self.stopped.is_set():
                    try:
                        state = self.ptt.frame() if self.ptt else {"connected": True, "down": False, "generation": 0}
                        self.frames.put_nowait((bytes(data), state))
                    except queue.Full:
                        pass

            with audio_session(), sd.RawInputStream(device=device, samplerate=16000, channels=1, dtype="int16",
                                   blocksize=480, extra_settings=extra, callback=callback) as stream:
                self.report("microphone", f"Listening: {actual['name']}")
                while not self.stopped.is_set():
                    if not stream.active:
                        raise RuntimeError("Input stream stopped")
                    state = self.ptt.frame() if self.ptt else {"connected":True,"down":False}
                    barge = self.settings.get("ptt_mode") == "hybrid" and state["connected"] and state["down"]
                    if self.paused.is_set() and self.settings.get("ptt_mode") != "hold" and not barge:
                        segmenter = new_segmenter()
                        while not self.frames.empty():
                            try:
                                self.frames.get_nowait()
                            except queue.Empty:
                                break
                        self.stopped.wait(0.05)
                        continue
                    try:
                        frame, state = self.frames.get(timeout=0.1)
                        if state["generation"] != generation:
                            segmenter = new_segmenter()
                            generation = state["generation"]
                        if self.settings.get("ptt_mode") == "hold" and not state["connected"]:
                            segmenter.reset()
                            continue
                        segmenter.tail_ms = state.get("tail_ms", 150)
                        segmenter.max_hold_s = state.get("max_hold_s", 30)
                        audio = segmenter.feed(frame, state["down"] if state["connected"] else False)
                    except queue.Empty:
                        continue
                    if audio:
                        try:
                            self.utterances.put_nowait(("audio", {"pcm": audio, "held": segmenter.last_held,
                            "generation": generation, "captured_at": __import__("time").monotonic(),
                            "trace_id": __import__("uuid").uuid4().hex}))
                        except queue.Full:
                            self.report("error", "Audio queue full; utterance dropped without replay.")
        except Exception as exc:
            raise RuntimeError(str(exc)) from exc
        finally:
            if "turn_worker" in locals() and turn_worker:
                turn_worker.close()


def wav_bytes(pcm):
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(16000)
        output.writeframes(pcm)
    return buffer.getvalue()


class Speaker:
    def __init__(self, settings, stopped):
        from pathlib import Path
        import os
        os.environ["HF_HUB_OFFLINE"] = "1"
        os.environ["TRANSFORMERS_OFFLINE"] = "1"
        import torch
        from kokoro import KModel, KPipeline
        from client.state import ROOT
        torch.set_num_threads(8)
        models = ROOT / "models/kokoro"
        model = KModel(repo_id="hexgrad/Kokoro-82M", config=str(models / "config.json"),
                       model=str(models / "kokoro-v1_0.pth")).to("cpu").eval()
        self.pipeline = KPipeline(lang_code="b", repo_id="hexgrad/Kokoro-82M", model=model)
        self.voice = str(models / (settings["voice"] + ".pt"))
        if not Path(self.voice).exists():
            raise RuntimeError("Selected voice asset is missing")
        self.device = resolve(settings["speaker"], "output")
        self.output_settings = shared_output_settings(self.device)
        self.stopped = stopped
        self.effect = settings.get("voice_effect", "dry")
        self.interrupt = threading.Event()

    def say(self, text, delivery="normal"):
        speeds = {"normal": 1.15, "caution": 1.18, "urgent": 1.24, "critical": 1.28}
        for result in self.pipeline(text, voice=self.voice, speed=speeds.get(delivery, 1.0)):
            if self.stopped.is_set() or self.interrupt.is_set():
                return
            from client.voice_effects import robot_filter
            with audio_session():
                sd.play(robot_filter(np.asarray(result.audio), preset=self.effect), 24000,
                        device=self.device, extra_settings=self.output_settings)
                sd.wait()

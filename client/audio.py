"""Continuous, half-duplex voice capture and local speech output."""
import collections
import io
import queue
import threading
import wave

import numpy as np
import sounddevice as sd
import webrtcvad


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


class Segmenter:
    """Require sustained speech, preserve a short pre-roll, finish after silence."""
    def __init__(self, silence_ms=850, maximum=25):
        self.vad = webrtcvad.Vad(2)
        self.pre = collections.deque(maxlen=10)
        self.votes = collections.deque(maxlen=8)
        self.frames = []
        self.quiet = 0
        self.voice_frames = 0
        self.silence_frames = max(5, silence_ms // 30)
        self.maximum = int(maximum / 0.03)

    def feed(self, frame):
        samples = np.frombuffer(frame, dtype=np.int16).astype(np.float32)
        voiced = bool(np.sqrt(np.mean(samples * samples)) > 120 and self.vad.is_speech(frame, 16000))
        if not self.frames:
            self.pre.append(frame)
            self.votes.append(voiced)
            if sum(self.votes) >= 5:
                self.frames = list(self.pre)
                self.voice_frames = sum(self.votes)
            return None
        self.frames.append(frame)
        self.voice_frames += int(voiced)
        self.quiet = 0 if voiced else self.quiet + 1
        if self.quiet >= self.silence_frames or len(self.frames) >= self.maximum:
            audio = b"".join(self.frames) if self.voice_frames >= 10 else None
            self.frames = []
            self.pre.clear()
            self.votes.clear()
            self.voice_frames = self.quiet = 0
            return audio
        return None


class Microphone:
    def __init__(self, settings, utterances, paused, stopped, report):
        self.settings, self.utterances = settings, utterances
        self.paused, self.stopped, self.report = paused, stopped, report
        self.frames = queue.Queue(maxsize=100)

    def run(self):
        try:
            device = resolve(self.settings["microphone"], "input")
            actual = sd.query_devices(device, "input")
            host = sd.query_hostapis(actual["hostapi"])["name"]
            extra = sd.WasapiSettings(auto_convert=True) if host == "Windows WASAPI" else None
            segmenter = Segmenter(self.settings["silence_ms"], self.settings["max_utterance_seconds"])

            def callback(data, frames, timing, status):
                if not self.paused.is_set() and not self.stopped.is_set():
                    try:
                        self.frames.put_nowait(bytes(data))
                    except queue.Full:
                        pass

            with sd.RawInputStream(device=device, samplerate=16000, channels=1, dtype="int16",
                                   blocksize=480, extra_settings=extra, callback=callback):
                self.report("microphone", f"Listening: {actual['name']}")
                while not self.stopped.is_set():
                    if self.paused.is_set():
                        segmenter = Segmenter(self.settings["silence_ms"], self.settings["max_utterance_seconds"])
                        while not self.frames.empty():
                            try:
                                self.frames.get_nowait()
                            except queue.Empty:
                                break
                        self.stopped.wait(0.05)
                        continue
                    try:
                        audio = segmenter.feed(self.frames.get(timeout=0.1))
                    except queue.Empty:
                        continue
                    if audio:
                        self.paused.set()
                        self.utterances.put(("audio", audio))
        except Exception as exc:
            self.report("error", f"Microphone: {exc}")


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
        self.stopped = stopped
        self.interrupt = threading.Event()

    def say(self, text, delivery="normal"):
        self.interrupt.clear()
        speeds = {"normal": 1.0, "caution": 1.03, "urgent": 1.12, "critical": 1.18}
        for result in self.pipeline(text, voice=self.voice, speed=speeds.get(delivery, 1.0)):
            if self.stopped.is_set() or self.interrupt.is_set():
                return
            sd.play(np.asarray(result.audio), 24000, device=self.device)
            sd.wait()

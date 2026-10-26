"""Opt-in local hardware check: synthetic speech-format tone, VBAN relay, mic capture.

Does not contact SKIPPY or start HOLT. Temporarily redirects the two dedicated VBAN
streams to loopback, then restores every changed parameter in finally. Run while
HOLT is stopped or still using physical devices, never during a remote conversation.
"""
import importlib.util
import json
from pathlib import Path
import socket
import sys
import threading
import time

import numpy as np
import sounddevice as sd

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from client.audio import shared_output_settings
from client.state import ROOT


def main():
    spec = importlib.util.spec_from_file_location("bridge", Path(__file__).with_name("setup-holt-audio-host.py"))
    bridge = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bridge)
    remote = bridge.Remote()
    outgoing = [i for i, d in enumerate(sd.query_devices()) if d["name"].startswith("Voicemeeter Input (")
                and sd.query_hostapis(d["hostapi"])["name"] == "Windows WASAPI"]
    incoming = [i for i, d in enumerate(sd.query_devices()) if d["name"].startswith(("Voicemeeter Out B1 (", "Voicemeeter Output ("))
                and sd.query_hostapis(d["hostapi"])["name"] == "Windows WASAPI"]
    if len(outgoing) != 1 or len(incoming) != 1:
        remote.close()
        raise RuntimeError("Could not uniquely identify bridge devices")
    # Do not redirect a physical microphone or unrelated mixer strip by accident.
    if remote.get("vban.instream[0].name", True) != "SKIPPY_MIC" or remote.get("vban.outstream[0].name", True) != "HOLT_VOICE":
        remote.close()
        raise RuntimeError("Dedicated bridge streams are not configured")
    keys = {"vban.instream[0].ip": True, "vban.instream[0].port": False,
            "vban.outstream[0].ip": True, "vban.outstream[0].port": False,
            "vban.instream[0].on": False, "vban.outstream[0].on": False}
    saved = {key: remote.get(key, string) for key, string in keys.items()}
    capture, packets = [], []
    stop = threading.Event()
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", 16980))
    sock.settimeout(.1)
    def relay():
        while not stop.is_set():
            try:
                payload, _ = sock.recvfrom(4096)
            except socket.timeout:
                continue
            if payload[:4] == b"VBAN" and payload[8:24].rstrip(b"\0") == b"HOLT_VOICE":
                packets.append(payload)
                # Preserve VBAN's format, sample count and sequence; change only stream name.
                sock.sendto(payload[:8] + b"SKIPPY_MIC".ljust(16, b"\0") + payload[24:], ("127.0.0.1", 6980))
    thread = threading.Thread(target=relay, daemon=True)
    try:
        remote.set("vban.instream[0].on", 0)
        remote.set("vban.outstream[0].on", 0)
        remote.set("vban.instream[0].ip", "127.0.0.1")
        remote.set("vban.outstream[0].ip", "127.0.0.1")
        remote.set("vban.outstream[0].port", 16980)
        remote.set("vban.instream[0].on", 1)
        remote.set("vban.outstream[0].on", 1)
        time.sleep(.3)
        thread.start()
        def collect(data, *_):
            capture.append(data.copy())
        with sd.InputStream(device=incoming[0], samplerate=16000, channels=1, dtype="int16",
                            extra_settings=sd.WasapiSettings(auto_convert=True), callback=collect):
            time.sleep(.2)
            tone = (np.sin(2 * np.pi * 440 * np.arange(24000) / 24000) * .03).astype("float32")
            sd.play(tone, 24000, device=outgoing[0], extra_settings=shared_output_settings(outgoing[0]))
            sd.wait()
            time.sleep(.5)
        peak = max((int(np.max(np.abs(frame.astype(np.int32)))) for frame in capture), default=0)
        result = {"test": "24 kHz HOLT-format output -> A2/VBAN -> local relay -> mic strip/B1 -> 16 kHz capture",
                  "passed": bool(packets and peak > 100), "vban_packets": len(packets),
                  "captured_peak_pcm_sample": peak, "remote_peer_tested": False}
        (ROOT / "audio-bridge/two-way-local-check.json").write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
        if not result["passed"]:
            raise RuntimeError("Two-way local format/routing check failed")
    finally:
        sd.stop()
        stop.set()
        if thread.is_alive():
            thread.join(timeout=1)
        sock.close()
        remote.set("vban.instream[0].on", 0)
        remote.set("vban.outstream[0].on", 0)
        for key, value in saved.items():
            if not key.endswith(".on"):
                remote.set(key, value)
        remote.set("vban.instream[0].on", saved["vban.instream[0].on"])
        remote.set("vban.outstream[0].on", saved["vban.outstream[0].on"])
        try:
            for _ in range(50):
                time.sleep(.1)
                if all(remote.get(key, keys[key]) == value for key, value in saved.items()):
                    break
            else:
                raise RuntimeError("Peer settings restoration was not confirmed; reapply the saved host profile before use")
        finally:
            remote.close()


if __name__ == "__main__":
    main()

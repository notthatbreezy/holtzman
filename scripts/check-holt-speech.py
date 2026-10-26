"""Test cached CPU speech synthesis; no microphone capture or game connection."""
import argparse
import json
import os
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / ".local/holt"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--devices", action="store_true", help="List audio devices and exit")
    parser.add_argument("--play", action="store_true", help="Play the generated test reply")
    parser.add_argument("--text", default="Captain, local voice systems are ready.")
    args = parser.parse_args()
    if args.devices:
        import sounddevice as sd
        print(sd.query_devices())
        return
    # All required assets are explicit local files. Unexpected downloads fail.
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    setup = time.perf_counter()
    import numpy as np
    import soundfile as sf
    import torch
    from kokoro import KModel, KPipeline

    torch.set_num_threads(8)
    models = ROOT / "models/kokoro"
    model = KModel(repo_id="hexgrad/Kokoro-82M", config=str(models / "config.json"),
                   model=str(models / "kokoro-v1_0.pth")).to("cpu").eval()
    pipeline = KPipeline(lang_code="a", repo_id="hexgrad/Kokoro-82M", model=model)
    report = {"device": "cpu", "setup_seconds": time.perf_counter() - setup, "runs": []}
    results = ROOT / "results"
    results.mkdir(parents=True, exist_ok=True)
    for run in range(2):
        started = time.perf_counter()
        first = None
        chunks = []
        for result in pipeline(args.text, voice=str(models / "af_heart.pt")):
            if first is None:
                first = time.perf_counter() - started
            chunks.append(np.asarray(result.audio))
        elapsed = time.perf_counter() - started
        audio = np.concatenate(chunks)
        path = results / f"voice-test-{run + 1}.wav"
        sf.write(path, audio, 24000)
        row = {"run": run + 1, "first_chunk_seconds": first, "total_seconds": elapsed,
               "audio_seconds": len(audio) / 24000, "path": str(path)}
        report["runs"].append(row)
        print(json.dumps(row), flush=True)
    (results / "speech-synthesis.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    if args.play:
        import sounddevice as sd
        sd.play(audio, 24000)
        sd.wait()


if __name__ == "__main__":
    main()

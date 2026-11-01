"""Audition Pocket TTS locally; writes streaming timings and WAVs, changes no services."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import threading
import time


DOCKING = "Captain, docking clearance is confirmed. Try to arrive with the same number of hull panels we left with."
LINES = {
    "greeting": "Systems online, Captain. Ready when you are.",
    "fuel": "Your fuel is at sixty percent. We have enough for the next jump.",
    "urgent": "Hull integrity is falling, Captain. Stop demonstrating the docking clamps with the rest of the ship.",
    "briefing": ("Captain, the route is ready. We have enough fuel for the next three jumps, "
                 "and the destination station accepts our landing pad size. Finish the current "
                 "scan before we depart. Once we arrive, I suggest refueling, repairing the hull, "
                 "and checking the passenger lounge for a return contract. We are trying to earn "
                 "a quiet living, after all. Shooting the docking clamps would make that considerably "
                 "less quiet. I have marked the station in our navigation plan and will keep an eye "
                 "on fuel consumption along the way."),
}


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=root / ".local/holt/chatterbox-server/reference_audio/HOLT-K2SO.wav")
    parser.add_argument("--extra-reference", type=Path)
    parser.add_argument("--stock-voice", help="Independent baseline using an official preset instead of cloning")
    parser.add_argument("--language", default="english_2026-10")
    parser.add_argument("--threads", type=int, default=2)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--output", type=Path, default=root / ".local/holt/results/pocket-tts-2026-11-01")
    args = parser.parse_args()
    if args.threads < 1 or not args.reference.is_file():
        parser.error("Choose a positive thread count and an existing reference WAV")
    os.environ.setdefault("HF_HOME", str(root / ".local/holt/models/pocket-hf"))
    os.environ.setdefault("HF_HUB_DISABLE_XET", "1")
    os.environ["OMP_NUM_THREADS"] = str(args.threads)
    os.environ["MKL_NUM_THREADS"] = str(args.threads)
    args.output.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter()
    import numpy as np
    import soundfile as sf
    import torch
    from pocket_tts import TTSModel, export_model_state
    imports_seconds = time.perf_counter() - started
    torch.set_num_threads(args.threads)
    torch.set_num_interop_threads(1)
    metadata = {"pocket_tts": importlib.metadata.version("pocket-tts"), "torch": torch.__version__,
                "language": args.language, "threads": args.threads, "seed": 42,
                "imports_seconds": round(imports_seconds, 4), "device": "cpu",
                "note": "Direct streaming engine; first chunk is not measured speaker latency. No filter or speed adjustment."}
    results, references = [], []

    def save():
        (args.output / "results.json").write_text(json.dumps({"metadata": metadata,
            "references": references, "results": results}, indent=2), encoding="utf-8")

    print("Loading Pocket TTS (downloads are included in first load timing)...", flush=True)
    started = time.perf_counter()
    model = TTSModel.load_model(language=args.language)
    metadata.update(model_load_seconds=round(time.perf_counter() - started, 4),
                    sample_rate=model.sample_rate, parameters=sum(p.numel() for p in model.parameters()),
                    has_voice_cloning=model.has_voice_cloning)
    assert str(model.device) == "cpu", "This profile is specifically a CPU comparison"
    print(json.dumps(metadata), flush=True)
    save()
    if not args.stock_voice and not model.has_voice_cloning:
        metadata["status"] = "blocked_voice_cloning_access"
        save()
        raise SystemExit("K-2SO test blocked: accept the official Hugging Face model conditions and authenticate locally. No clone samples generated.")

    def condition(reference, label):
        started = time.perf_counter()
        prompt = str(reference.resolve()) if isinstance(reference, Path) else reference
        state = model.get_state_for_audio_prompt(prompt)
        elapsed = time.perf_counter() - started
        export_model_state(state, str(args.output / f"{label}-voice.safetensors"))
        record = {"label": label, "conditioning_seconds": round(elapsed, 4)}
        if isinstance(reference, Path):
            record.update(filename=reference.name, sha256=hashlib.sha256(reference.read_bytes()).hexdigest(),
                          audio_seconds=round(sf.info(reference).duration, 4))
        else:
            record["stock_voice"] = reference
        references.append(record)
        save()
        print(json.dumps(references[-1]), flush=True)
        return state

    def run(state, label, text, cold=False):
        torch.manual_seed(42)
        chunks, arrivals = [], []
        cancel = threading.Event()
        started = time.perf_counter()
        frames = 0
        for chunk in model.generate_audio_stream(state, text, copy_state=True, stop=cancel):
            elapsed = time.perf_counter() - started
            samples = chunk.detach().cpu().numpy().reshape(-1)
            if not samples.size:
                continue
            frames += samples.size
            chunks.append(samples.copy())
            arrivals.append({"seconds": round(elapsed, 4), "audio_end_seconds": round(frames / model.sample_rate, 4)})
            if frames > model.sample_rate * 90 or elapsed > 180:
                cancel.set()
                raise RuntimeError("Synthesis exceeded the diagnostic duration limit")
        elapsed = time.perf_counter() - started
        if not chunks:
            raise RuntimeError("Pocket TTS returned no audio")
        audio = np.concatenate(chunks)
        if not np.isfinite(audio).all():
            raise RuntimeError("Pocket TTS returned non-finite audio")
        duration = audio.size / model.sample_rate
        filename = label + ".wav"
        sf.write(args.output / filename, audio, model.sample_rate, subtype="PCM_16")
        # Minimal ideal initial buffer to avoid any underrun, based on actual
        # chunk arrival times. Playback/backend/device buffering is additional.
        previous_end = 0
        continuous_start = 0
        for arrival in arrivals:
            continuous_start = max(continuous_start, arrival["seconds"] - previous_end)
            previous_end = arrival["audio_end_seconds"]
        result = {"label": label, "text": text, "cold_first_generation": cold,
                  "first_chunk_seconds": arrivals[0]["seconds"],
                  "first_chunk_audio_seconds": arrivals[0]["audio_end_seconds"],
                  "continuous_start_seconds": round(continuous_start, 4),
                  "synthesis_seconds": round(elapsed, 4), "audio_seconds": round(duration, 4),
                  "realtime_factor": round(elapsed / duration, 4), "chunks": len(chunks),
                  "peak": round(float(np.max(np.abs(audio))), 4), "audio_file": filename,
                  "arrivals": arrivals}
        results.append(result)
        save()
        print(json.dumps({k: v for k, v in result.items() if k != "arrivals"}), flush=True)

    state = condition(args.stock_voice or args.reference, "stock" if args.stock_voice else "benchmark")
    for repeat in range(3):
        run(state, f"docking-{repeat}", DOCKING, cold=repeat == 0)
    for label, text in LINES.items():
        if not args.quick or label == "urgent":
            run(state, label, text)
    if args.extra_reference and not args.stock_voice:
        state = condition(args.extra_reference, "current-short")
        run(state, "short-reference-docking", DOCKING)
        run(state, "short-reference-urgent", LINES["urgent"])


if __name__ == "__main__":
    main()

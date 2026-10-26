"""Generate a short local Kokoro voice audition; optionally play it."""
import argparse
import os
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"

import numpy as np
import soundfile as sf
import torch
from kokoro import KModel, KPipeline

ROOT = Path(__file__).resolve().parents[1] / ".local/holt"
RATE = 24000
PHRASE = "Captain, long range sensors are online. Navigation is ready. Awaiting your command."
SAMPLES = [
    ("01-sky", "Sample one. Sky, calm ship computer.", "af_sky", "a", 1.0),
    ("02-fenrir", "Sample two. Fenrir, deeper voice.", "am_fenrir", "a", 0.95),
    ("03-george", "Sample three. George, British assistant.", "bm_george", "b", 1.0),
    ("04-emma", "Sample four. Emma, British ship computer.", "bf_emma", "b", 1.0),
    ("05-fenrir-comms", "Sample five. Fenrir, communications filter.", "am_fenrir", "a", 0.95),
]
DEADPAN_SAMPLES = [
    ("01-george-formal", "Sample one. George, formal.", "bm_george", "b", 1.0),
    ("02-george-measured", "Sample two. George, measured.", "bm_george", "b", 0.90),
    ("03-george-lowered", "Sample three. George, lower register.", "bm_george", "b", 0.97),
    ("04-michael-deadpan", "Sample four. Michael, American deadpan.", "am_michael", "a", 0.94),
]
DEADPAN_PHRASE = (
    "Captain. The route is ready. The galaxy remains inconveniently large. "
    "I have filed no complaint. Yet."
)
URGENCY_SAMPLES = [
    ("01-caution", "Sample one. Caution.", 1.03,
     "Captain. Hull integrity is falling. Clear the structure. "
     "I would prefer our next docking attempt to involve the entrance."),
    ("02-urgent", "Sample two. Urgent.", 1.12,
     "Captain! Hull integrity, thirty percent and falling. Break off the approach. "
     "Move clear of the station. Now!"),
    ("03-critical", "Sample three. Critical.", 1.18,
     "Hull critical! Twelve percent. Collision imminent! Pull clear! "
     "Captain, I need your attention. Now!"),
    ("04-recovery", "Sample four. After the danger.", 1.0,
     "Hull damage has stopped. We are clear. "
     "I have classified that maneuver as educational. Let us not repeat the lesson."),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--play", action="store_true")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--deadpan", action="store_true", help="Audition formal, dry variants")
    mode.add_argument("--urgency", action="store_true", help="Audition fictional hull warnings")
    args = parser.parse_args()
    torch.set_num_threads(8)
    models = ROOT / "models/kokoro"
    folder = "george-urgency" if args.urgency else "george-deadpan" if args.deadpan else "voice-audition"
    output = ROOT / "results" / folder
    output.mkdir(parents=True, exist_ok=True)
    model = KModel(repo_id="hexgrad/Kokoro-82M", config=str(models / "config.json"),
                   model=str(models / "kokoro-v1_0.pth")).to("cpu").eval()
    pipelines = {lang: KPipeline(lang_code=lang, repo_id="hexgrad/Kokoro-82M", model=model)
                 for lang in ("a", "b")}
    compilation = []
    samples = DEADPAN_SAMPLES if args.deadpan else SAMPLES
    phrase = DEADPAN_PHRASE if args.deadpan else PHRASE
    rows = [(name, label, voice, lang, speed, phrase) for name, label, voice, lang, speed in samples]
    if args.urgency:
        rows = [(name, label, "bm_george", "b", speed, text)
                for name, label, speed, text in URGENCY_SAMPLES]
    for name, label, voice, lang, speed, text in rows:
        chunks = [np.asarray(result.audio) for result in pipelines[lang](
            label + " " + text, voice=str(models / (voice + ".pt")), speed=speed)]
        audio = np.concatenate(chunks)
        if name.endswith("lowered"):
            # One semitone lower through resampling, with a small duration increase.
            ratio = 2 ** (-1 / 12)
            audio = np.interp(np.arange(0, len(audio) - 1, ratio), np.arange(len(audio)), audio)
        if name.endswith("comms"):
            # Mild radio coloration, short echo and a subtle modulation.
            low = np.convolve(audio, np.ones(5) / 5, mode="same")
            audio = low - np.convolve(low, np.ones(80) / 80, mode="same")
            timeline = np.arange(len(audio)) / RATE
            audio *= 0.94 + 0.06 * np.sin(2 * np.pi * 70 * timeline)
            delay = int(0.035 * RATE)
            audio[delay:] += 0.12 * audio[:-delay].copy()
        peak = np.max(np.abs(audio))
        if peak:
            audio = (audio * (0.7 / peak)).astype(np.float32)
        path = output / (name + ".wav")
        sf.write(path, audio, RATE)
        compilation.extend([audio, np.zeros(int(0.8 * RATE), dtype=np.float32)])
        print(f"{name}: {len(audio) / RATE:.1f}s - {path}", flush=True)
    combined = np.concatenate(compilation)
    sf.write(output / "all-voices.wav", combined, RATE)
    if args.play:
        import sounddevice as sd
        print("Playing on the Windows default output device.", flush=True)
        sd.play(combined, RATE)
        sd.wait()
        print("Audition playback finished.", flush=True)


if __name__ == "__main__":
    main()

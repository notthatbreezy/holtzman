"""Same George line through three mechanical timbres, plus a dry reference."""
import sys
import threading
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import numpy as np
import soundfile as sf

from client.audio import Speaker
from client.state import ROOT, load_settings
from client.voice_effects import robot_filter

speaker = Speaker(load_settings(), threading.Event())
text = "Captain, docking clearance is confirmed. Try to use the entrance this time. I have grown rather attached to the hull."
audio = np.concatenate([np.asarray(r.audio) for r in speaker.pipeline(text, voice=speaker.voice, speed=1.15)])
folder = ROOT / "results/robot-voices"
folder.mkdir(parents=True, exist_ok=True)
for preset in ("dry", "protocol", "protocol_tight", "protocol_tighter", "security", "subtle"):
    output = folder / (preset + ".wav")
    sample = robot_filter(audio, preset=preset)
    sf.write(output, sample, 24000)
    print(preset, round(len(sample) / 24000, 1), str(output), flush=True)

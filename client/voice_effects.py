"""Lightweight metallic voice coloration, using only NumPy."""
import numpy as np


def robot_filter(audio, rate=24000, preset="subtle"):
    if preset == "dry":
        return np.asarray(audio, dtype=np.float32)
    parameters = {"protocol": (1.045, 64, .26, .011, .19),
                  "protocol_tight": (1.045, 68, .24, .006, .07),
                  "protocol_tighter": (1.045, 72, .18, .004, .035),
                  "security": (.90, 43, .16, .019, .16),
                  "subtle": (1.0, 55, .085, .013, .09)}
    pitch, carrier, depth, delay, echo = parameters[preset]
    original = np.asarray(audio, dtype=np.float32)
    audio = np.interp(np.arange(0, len(original) - 1, pitch), np.arange(len(original)), original)
    # Band limiting, soft compression, amplitude sidebands and short comb echoes.
    width = 5 if preset.startswith("protocol") else 8
    low = np.convolve(audio, np.ones(width) / width, mode="same")
    body = low - np.convolve(low, np.ones(180) / 180, mode="same")
    body = np.tanh(body * 1.55) / 1.55
    timeline = np.arange(len(body)) / rate
    metal = body * (1 - depth + depth * np.cos(2 * np.pi * carrier * timeline))
    dry = .34 if preset.startswith("protocol_tight") else .22
    result = dry * audio + (1 - dry) * metal
    for length, gain in [(delay, echo), (delay * 1.73, echo * .4)]:
        offset = int(length * rate)
        result[offset:] += gain * metal[:-offset]
    peak = np.max(np.abs(result)) if len(result) else 0
    if peak:
        # Match overall level to the dry clip without clipping.
        target = min(.90, float(np.max(np.abs(original))))
        result *= target / peak
    return result.astype(np.float32)

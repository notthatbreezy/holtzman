"""Low-latency voice coloration; filter and oscillator state survive PCM chunks."""
import numpy as np
from scipy.signal import butter, lfilter, sosfilt


class PocketEffects:
    def __init__(self, rate, effect="radio", volume=0.85, robot_amount=0.10):
        self.rate, self.effect = rate, effect
        self.volume, self.robot_amount = volume, robot_amount
        # Retain the low voice fundamentals; soften sibilance and generated hiss.
        low, high = (140, 3200) if effect == "radio" else (65, 3800)
        self.sos = np.vstack((butter(2, low, "highpass", fs=rate, output="sos"),
                              butter(4, high, "lowpass", fs=rate, output="sos")))
        self.filter_state = np.zeros((len(self.sos), 2))
        self.envelope_state = np.zeros(1)
        self.gain_state = np.ones(1) * np.exp(-1 / (0.015 * rate))
        self.position = 0

    def process(self, audio):
        audio = np.asarray(audio, dtype=np.float64).reshape(-1)
        if not audio.size:
            return audio
        if not np.isfinite(audio).all():
            raise ValueError("Non-finite audio")
        if self.effect != "original":
            if self.effect == "radio":
                # Mild transmitter-like saturation, then bandwidth limiting.
                # Unity small-signal gain; no hiss, squelch bursts or makeup boost.
                audio = np.tanh(1.5 * audio) / 1.5
            audio, self.filter_state = sosfilt(self.sos, audio, zi=self.filter_state)
            pole = np.exp(-1 / (0.010 * self.rate))
            energy, self.envelope_state = lfilter([1 - pole], [1, -pole], audio * audio,
                                                 zi=self.envelope_state)
            # Gentle downward expansion below -48 dB, never a hard silence gate.
            target = np.clip((np.sqrt(np.maximum(energy, 0)) / 10**(-48 / 20))**0.5, 0.35, 1)
            pole = np.exp(-1 / (0.015 * self.rate))
            gain, self.gain_state = lfilter([1 - pole], [1, -pole], target, zi=self.gain_state)
            audio *= gain
            if self.effect in ("droid", "radio"):
                phase = (self.position + np.arange(audio.size)) * (2 * np.pi * 70 / self.rate)
                audio *= 1 - self.robot_amount + self.robot_amount * np.cos(phase)
        self.position += audio.size
        # Fixed gain, no per-chunk peak normalization, added noise or pitch shift.
        return np.clip(audio * self.volume, -1, 1)

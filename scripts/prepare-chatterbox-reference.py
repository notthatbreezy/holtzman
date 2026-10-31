"""Convert or combine local speech clips into a Chatterbox reference WAV."""

import argparse
from pathlib import Path

import librosa
import numpy as np
import soundfile as sf


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('clips', nargs='+', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f'Output already exists; choose a new filename: {args.output}')
    if args.output.suffix.lower() != '.wav':
        parser.error('Output must be a .wav file')
    parts = []
    rate = 24000
    for clip in args.clips:
        audio, source_rate = sf.read(clip, dtype='float32', always_2d=True)
        if not len(audio) or not np.isfinite(audio).all():
            parser.error(f'Empty or invalid audio: {clip}')
        mono = librosa.resample(audio.mean(axis=1), orig_sr=source_rate, target_sr=rate)
        if parts:
            parts.append(np.zeros(int(rate * 0.12), dtype=np.float32))
        parts.append(mono)
    reference = np.concatenate(parts)
    if len(reference) / rate > 30:
        parser.error('Reference exceeds 30 seconds; select fewer/shorter clips')
    peak = float(np.max(np.abs(reference)))
    if peak == 0:
        parser.error('Reference is silent')
    reference /= max(1.0, peak / 0.95)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    sf.write(args.output, reference, rate, subtype='PCM_16')
    print(f'{args.output}: {len(reference) / rate:.2f} seconds, 24 kHz mono PCM16')


if __name__ == '__main__':
    main()

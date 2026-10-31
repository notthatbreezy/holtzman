"""Generate local Chatterbox reference-voice auditions and record actual latency."""

import argparse
import io
import json
import time
import wave
from pathlib import Path

import httpx


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--voice', default='HOLT-K2SO.wav')
    parser.add_argument('--text', default='Captain, docking clearance is confirmed. Try to arrive with the same number of hull panels we left with.')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output or root / '.local/holt/results' / f'chatterbox-{Path(args.voice).stem}.wav'
    request = dict(text=args.text, voice_mode='clone', reference_audio_filename=args.voice,
                   temperature=0.7, exaggeration=0.4, cfg_weight=0.5, seed=42,
                   speed_factor=1.1, language='en', split_text=False, output_format='wav')
    started = time.monotonic()
    with httpx.Client(base_url='http://127.0.0.1:8004', trust_env=False, timeout=600) as client:
        response = client.post('/tts', json=request)
        response.raise_for_status()
    elapsed = time.monotonic() - started
    with wave.open(io.BytesIO(response.content)) as audio:
        seconds = audio.getnframes() / audio.getframerate()
        assert seconds > 0 and audio.getnchannels() > 0, 'Empty output audio'
        rate = audio.getframerate()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(response.content)
    result = dict(voice=args.voice, text=args.text, elapsed_seconds=round(elapsed, 3),
                  audio_seconds=round(seconds, 3), sample_rate=rate,
                  realtime_factor=round(elapsed / seconds, 3), output=str(output))
    output.with_suffix('.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()

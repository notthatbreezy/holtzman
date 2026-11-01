"""Profile the installed Chatterbox engine in isolation; stop its server first.

Uses the existing GPU environment and cached weights. Does not edit its settings,
install packages, invoke a reasoning model, play audio or execute game controls.
"""
import argparse
import functools
import importlib.metadata
import json
import os
from pathlib import Path
import socket
import sys
import time
import wave


def main():
    repo = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server-dir', type=Path, default=repo / '.local/holt/chatterbox-server')
    parser.add_argument('--output', type=Path, default=repo / '.local/holt/results/chatterbox-profile')
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--text', default='Captain, docking clearance is confirmed. Try to arrive with the same number of hull panels we left with.')
    parser.add_argument('--precision', choices=['fp32', 'bf16', 'both'], default='both')
    parser.add_argument('--model', choices=['original', 'turbo', 'nano'], default='original')
    parser.add_argument('--checkpoint', type=Path, help='Optional cached Turbo checkpoint directory')
    parser.add_argument('--varied', action='store_true', help='Also measure previously unseen conversational sentences')
    parser.add_argument('--long', action='store_true', help='Also measure a longer synthetic briefing')
    args = parser.parse_args()
    with socket.socket() as sock:
        sock.settimeout(1)
        if sock.connect_ex(('127.0.0.1', 8004)) == 0:
            parser.error('Stop HOLT-Chatterbox.cmd before profiling; only one model process should run.')
    server = args.server_dir.resolve()
    checkpoint = args.checkpoint.resolve() if args.checkpoint else None
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    os.environ['HF_HOME'] = str(server / 'hf-cache')
    os.environ.setdefault('HF_HUB_DISABLE_XET', '1')
    os.environ['OMP_NUM_THREADS'] = '8'
    os.environ['MKL_NUM_THREADS'] = '8'
    os.environ['TTS_BF16'] = 'off'
    os.chdir(server)
    sys.path.insert(0, str(server))
    import numpy as np
    import torch
    import engine

    assert torch.cuda.is_available(), 'This comparison requires the installed GPU backend'
    assert args.repeats >= 2
    torch.set_num_threads(8)
    if args.model == 'nano':
        from chatterbox_runtime import nano_class
        cls = nano_class()
        if checkpoint:
            cls.from_pretrained = classmethod(lambda cls, device: cls.from_local(checkpoint, device, nano=True))
        engine._get_model_class = lambda _selector: (cls, 'turbo')
    elif args.model == 'turbo':
        assert engine.TURBO_AVAILABLE, 'Installed package has no Turbo implementation'
        if checkpoint:
            engine.ChatterboxTurboTTS.from_pretrained = classmethod(
                lambda cls, device: cls.from_local(checkpoint, device))
        engine._get_model_class = lambda _selector: (engine.ChatterboxTurboTTS, 'turbo')
    else:
        # Profile the requested model even when production is configured for Turbo.
        engine._get_model_class = lambda _selector: (engine.ChatterboxTTS, 'original')
    started = time.perf_counter()
    assert engine.load_model(), 'Model failed to load'
    model = engine.chatterbox_model
    assert engine.loaded_model_type == ('turbo' if args.model == 'nano' else args.model)
    builtin = model.conds
    stages = {}

    def timed(label, fn):
        @functools.wraps(fn)
        def wrapper(*a, **kw):
            torch.cuda.synchronize()
            before = time.perf_counter()
            value = fn(*a, **kw)
            torch.cuda.synchronize()
            stages[label] = stages.get(label, 0) + time.perf_counter() - before
            if label == 'tokens':
                stages['speech_token_count'] = int(value.shape[-1])
            return value
        return wrapper

    model.prepare_conditionals = timed('conditioning', model.prepare_conditionals)
    token_method = 'inference_turbo' if args.model in ('turbo', 'nano') else 'inference'
    setattr(model.t3, token_method, timed('tokens', getattr(model.t3, token_method)))
    model.s3gen.inference = timed('waveform', model.s3gen.inference)
    model.watermarker.apply_watermark = timed('watermark', model.watermarker.apply_watermark)
    metadata = {'text': args.text, 'model': args.model, 'chatterbox': importlib.metadata.version('chatterbox-tts'),
                'torch': torch.__version__, 'hip': torch.version.hip,
                'device': torch.cuda.get_device_name(), 'bf16_supported': torch.cuda.is_bf16_supported(),
                'model_load_seconds': round(time.perf_counter() - started, 3),
                'sample_rate': model.sr, 'postprocess_speed_factor': 1.0,
                'miopen_find_mode': os.environ.get('MIOPEN_FIND_MODE', 'default'),
                'cudnn_benchmark': torch.backends.cudnn.benchmark,
                'note': 'Direct engine timings; first run per case excluded from warm comparisons.'}
    results = []

    def save():
        (output / 'results.json').write_text(json.dumps({'metadata': metadata, 'results': results}, indent=2), encoding='utf-8')

    def run(precision, voice, path, repeats=None):
        if voice == 'builtin':
            model.conds = builtin
        for repeat in range(args.repeats if repeats is None else repeats):
            stages.clear()
            torch.cuda.synchronize()
            before = time.perf_counter()
            audio, rate = engine.synthesize(args.text, audio_prompt_path=str(path) if path else None,
                temperature=.7, exaggeration=.4, cfg_weight=.5, seed=42)
            torch.cuda.synchronize()
            elapsed = time.perf_counter() - before
            result = {'precision': precision, 'voice': voice, 'repeat': repeat, 'text': args.text,
                      'warm': repeat > 0, 'seconds': round(elapsed, 4),
                      'stages': {k: round(v, 4) for k, v in stages.items()}}
            if audio is None:
                result['error'] = 'Synthesis failed; see profiler stderr'
                results.append(result); save(); print(json.dumps(result), flush=True)
                return
            samples = audio.detach().cpu().numpy().squeeze()
            assert np.isfinite(samples).all() and samples.size > 0, 'Invalid waveform'
            pcm = (np.clip(samples, -1, 1) * 32767).astype('<i2')
            filename = f'{precision}-{voice}-{repeat}.wav'
            with wave.open(str(output / filename), 'wb') as wav:
                wav.setnchannels(1); wav.setsampwidth(2); wav.setframerate(rate); wav.writeframes(pcm.tobytes())
            duration = samples.size / rate
            result.update(audio_seconds=round(duration, 4), realtime_factor=round(elapsed / duration, 4),
                          peak=round(float(np.max(np.abs(samples))), 4), audio_file=filename)
            results.append(result); save(); print(json.dumps(result), flush=True)

    paths = [('clone', server / 'reference_audio/HOLT-K2SO.wav'),
             ('predefined', server / 'voices/Thomas.wav'), ('builtin', None)]
    for _, path in paths:
        if path:
            assert path.is_file(), f'Missing reference: {path}'
    print(json.dumps(metadata), flush=True)
    def varied(precision):
        if not args.varied and not args.long:
            return
        original_text = args.text
        texts = [
            'Systems online, Captain. Ready when you are.',
            'Your fuel is at sixty percent. We have enough for the next jump.',
            'Hull integrity is falling, Captain. Stop demonstrating the docking clamps with the rest of the ship.',
        ] if args.varied else []
        if args.long:
            texts.append('Captain, the route is ready. We have enough fuel for the next three jumps, '
                         'and the destination station accepts our landing pad size. Finish the current '
                         'scan before we depart. Once we arrive, I suggest refueling, repairing the hull, '
                         'and checking the passenger lounge for a return contract. We are trying to earn '
                         'a quiet living, after all. Shooting the docking clamps would make that considerably '
                         'less quiet. I have marked the station in our navigation plan and will keep an eye '
                         'on fuel consumption along the way.')
        for index, text in enumerate(texts):
            args.text = text
            run(precision, f'clone-varied-{index}', paths[0][1], repeats=1)
        args.text = original_text

    if args.precision in ('fp32', 'both'):
        for voice, path in paths:
            run('fp32', voice, path)
        varied('fp32')
    if args.precision in ('bf16', 'both'):
        if not metadata['bf16_supported']:
            print('BF16 unsupported; leaving it untested.', flush=True)
        else:
            model.t3.bfloat16()
            engine.BF16_ENABLED = True
            engine._conds_cache.clear()
            run('bf16', 'clone', paths[0][1])
            varied('bf16')
    save()


if __name__ == '__main__':
    main()

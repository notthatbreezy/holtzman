# Pocket TTS trial — 2026-11-01

## Status

Pocket TTS is installed in `.local/holt/pocket-env`, separate from Chatterbox's
working ROCm environment. No persistent Pocket server was started, and ST's voice
provider, K-2SO mapping, microphone settings and Chatterbox installation were not
changed.

**The requested K-2SO clone comparison is pending Hugging Face access.** The
official cloning weights are gated. An unauthenticated load downloaded the
stock-only model, whose `has_voice_cloning` flag was false. It explicitly rejected
`HOLT-K2SO.wav`. No K-2SO Pocket output or clone latency has been measured.

The user must review the conditions at the
[official model page](https://huggingface.co/kyutai/pocket-tts) and authenticate
locally. The gate was not bypassed, and the reference recording was not uploaded.
Once access is granted, conditioning and synthesis will run locally.

## Completed CPU baseline — stock voice only

Pocket TTS 4.0.1, CPU PyTorch 2.15.0, model `english_2026-10`, two intra-op threads,
one inter-op thread, seed 42, default generation parameters, 24 kHz PCM. The model
contains 109,501,826 parameters. No filter, pitch change or speed adjustment was
applied. Qwen and Chatterbox were not stopped; no game controls or hosted reasoning
requests were sent. The official stock voice was **marius**, not K-2SO.

| Input | First chunk | Complete synthesis | Generated audio |
| --- | ---: | ---: | ---: |
| Docking, first generation in process | 0.106 s | 1.294 s | 5.28 s |
| Docking, warm repeat 1 | 0.085 s | 1.283 s | 5.28 s |
| Docking, warm repeat 2 | 0.082 s | 1.279 s | 5.28 s |
| Greeting | 0.077 s | 0.791 s | 3.20 s |
| Fuel | 0.074 s | 0.982 s | 4.00 s |
| Urgent hull warning | 0.085 s | 1.120 s | 4.48 s |
| Longer navigation briefing | 0.087 s | 6.777 s | 27.92 s |

Every chunk contained 80 ms of audio. Recorded arrival times indicated the first
chunk was also a sufficient theoretical starting buffer to avoid underruns in
these runs. That is an offline timing calculation, not an actual playback test.
First chunk availability does not include device/browser/network buffering, the
initial silence inside audio, speech recognition, reasoning or game-tool time.

The repeat docking line averaged **83 ms first chunk** and **1.281 s complete
synthesis**, about 4.1 times real-time throughput. The comparison is not a claim
that Pocket finishes faster than Nano: our prior Nano GPU docking engine average
was 1.08 s for 5.64 s of audio. Pocket's main potential advantage here is starting
playback while synthesis continues. ST's current Chatterbox provider waits for a
returned WAV; exploiting Pocket streaming requires a compatible playback path.

After the download was cached, a fresh process took 3.47 s for imports and 0.57 s
to load the model. Loading the predefined voice state took 0.78 s. These startup
costs are separate from the table; a persistent service would reuse model/state.

Local Whisper transcribed the docking line verbatim. It transcribed the warning's
opening as "Whole integrity" instead of "Hull integrity", with the rest matching.
This flags a listening check; ASR alone does not establish a TTS pronunciation
error or voice quality. No actor-similarity assessment is possible from the stock
baseline. WAVs, voice states, chunk arrivals and transcripts remain private under
`.local/holt/results/pocket-stock-2026-11-01/`.

## Enable and run the K-2SO trial

After reviewing/accepting the model access conditions, run from the repo root:

```powershell
$env:HF_HOME = "$PWD\.local\holt\models\pocket-hf"
.local\holt\pocket-env\Scripts\hf.exe auth login
```

The HF CLI offers local authentication. Enter credentials only in its browser or
hidden terminal prompt, never in chat or as literal command-line arguments.
No Git credential helper is required. Credentials and model cache stay under the
ignored `.local` directory with this environment setting.

```powershell
.local\holt\pocket-env\Scripts\python.exe scripts/profile-pocket-tts.py `
  --extra-reference .local/holt/chatterbox-server/reference_audio/HOLT-K2SO-single.wav
```

The script defaults to the original **10.91 s** `HOLT-K2SO.wav` reference used by
the Chatterbox benchmarks. The extra reference is the **4.14 s** single clip
currently selected in Chatterbox. It records conditioning separately, saves
reusable voice states, then measures first chunk, full synthesis and all chunk
arrival times. It preserves the voice state across requests with `copy_state`.
It writes `results.json` after every completed case. Defaults use the same docking,
greeting, fuel, urgency and briefing text as the earlier Chatterbox investigation.

To reproduce only the completed baseline:

```powershell
.local\holt\pocket-env\Scripts\python.exe scripts/profile-pocket-tts.py `
  --stock-voice marius --output .local/holt/results/pocket-stock-new
```

The command exits when finished; there is no Pocket service to stop. Ctrl+C stops
a running benchmark. To preserve evidence, use a new `--output` directory on rerun.
Do not switch ST to Pocket yet: this trial has not integrated streaming playback.

## Installation record

The installer used the existing private uv runtime and Python 3.12.15:

```powershell
.local/holt/runtime/uv/uv.exe venv --python .local/holt/voice-env/Scripts/python.exe .local/holt/pocket-env
.local/holt/runtime/uv/uv.exe pip install --python .local/holt/pocket-env/Scripts/python.exe -r requirements/pocket-tts-windows.lock.txt
```

The selected package config pins cloning weights to revision
`db04f0e653473e5c7333c69686ac92b7460b0c1d` and stock-only weights to
`a7056f505a4a8ca72d8b5fb1440071d6f1a35739`. Both are official Kyutai repositories.
The model cache is `.local/holt/models/pocket-hf`; Windows used regular cache
files rather than symlinks. No system settings were changed to enable symlinks.

See the [official Python API](https://kyutai-labs.github.io/pocket-tts/API%20Reference/python-api/)
for streaming generation and voice-state export. Clone support must be checked
explicitly: a successful model download alone can mean the stock-only fallback.

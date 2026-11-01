# Pocket TTS trial — 2026-11-01

## Status

**Update:** Pocket is now integrated with ST's automatic narration and continuous
browser playback. See [operation and live integration checks](pocket-tts-sillytavern.md).
The sections below retain the earlier engine-only trial measurements.

The initial trial installed Pocket TTS in `.local/holt/pocket-env`, separate from
Chatterbox's working ROCm environment. At that stage no persistent Pocket server
was started, and ST's provider and voice mapping were left unchanged. The linked
integration instructions describe the current running setup.

**The K-2SO clone comparison is complete.** After the user accepted the official
model's access conditions and authenticated locally, Pocket loaded with
`has_voice_cloning: true`. Both K-2SO references worked. Conditioning and synthesis
ran locally on CPU; the reference recordings were not uploaded. The initial
unauthenticated attempt had loaded a stock-only fallback and rejected cloning;
the stock baseline below is kept separate from the completed clone results.

## Completed K-2SO results

Same Pocket version, language model, CPU settings and benchmark sentences as the
stock baseline below. The primary voice used the original **10.91-second**
`HOLT-K2SO.wav`, matching the earlier Chatterbox test reference. Two warm docking
repeats followed the first generation; other sentences were measured once.

| Input / reference | First chunk | Complete synthesis | Generated audio |
| --- | ---: | ---: | ---: |
| Docking, first generation | 0.090 s | 1.599 s | 6.48 s |
| Docking, warm repeat 1 | 0.087 s | 1.577 s | 6.48 s |
| Docking, warm repeat 2 | 0.083 s | 1.552 s | 6.48 s |
| Greeting | 0.076 s | 0.934 s | 3.76 s |
| Fuel | 0.073 s | 1.330 s | 5.44 s |
| Urgent hull warning | 0.083 s | 1.345 s | 5.60 s |
| Long briefing | 0.087 s | 7.426 s | 30.80 s |
| Docking, current 4.14-second reference | 0.082 s | 1.583 s | 6.64 s |
| Hull warning, current short reference | 0.080 s | 1.331 s | 5.52 s |

Warm docking averages: **85 ms first chunk**, **1.565 s complete synthesis**,
**6.48 s audio**, approximately **4.1 times real time**. At every measured chunk
arrival, enough audio had accumulated for the first-chunk start to avoid an
idealized underrun. Physical playback and ST integration are still untested.

A sequential four-thread check averaged **77 ms first chunk** and **1.517 s full
synthesis** for the same docking output: about 3% faster to finish, for twice the
configured intra-op threads. Two threads remain a sensible starting point; this
small test does not establish an optimal thread count under every workload.

The original reference took **0.650 s** to condition, and the shorter reference
**0.221 s**. Each state was then reused across requests and exported to a private
safetensors file. A fresh process with cached weights took **3.481 s imports**
plus **0.424 s model load**, separately from conditioning and generation. The
earlier first download/load took about 20 s and is not warm conversational latency.

### Comparison with Chatterbox Nano

| Same docking text | First audio available | Full engine synthesis | Output audio |
| --- | ---: | ---: | ---: |
| Pocket, K-2SO, CPU, two threads | 0.085 s | 1.565 s | 6.48 s |
| Nano, K-2SO, GPU, prior warm mean | Not measured as a stream | 1.08 s | 5.64 s |

Nano still finishes this entire utterance faster, including after normalizing for
its shorter generated audio. Pocket's advantage is **native incremental output**:
an appropriately integrated player could start well before synthesis finishes.
Our current Chatterbox/ST route waits for a returned WAV. These engine timings do
not prove an end-to-end ST speedup and do not include reasoning or tool latency.

### Audio checks and auditions

All seven local Whisper checks matched the requested words after ignoring
punctuation, whitespace and numeric formatting: docking, greeting, fuel, warning,
long briefing and the two short-reference samples. This is an intelligibility
check, not a subjective listening test or actor-similarity measurement.

Some generated peaks slightly exceeded full scale (maximum 1.050). The profiler
now applies **gain-only normalization** when needed before PCM export instead of
clipping. It records the original peak and export gain; generation timings precede
this postprocessing. No robot effect, pitch change or playback-speed adjustment
was added. The final files are under:

```text
.local/holt/results/pocket-k2so-audition-2026-11-01/docking-1.wav
.local/holt/results/pocket-k2so-audition-2026-11-01/urgent.wav
.local/holt/results/pocket-k2so-audition-2026-11-01/short-reference-docking.wav
.local/holt/results/pocket-k2so-audition-2026-11-01/short-reference-urgent.wav
```

The first unnormalized clone run is retained separately. Raw chunk arrivals,
transcripts and reusable voice states stay private; the committed benchmark
summary contains measurements and synthetic text only. ST was still on Chatterbox
at the end of this engine-only trial.

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

## Reproduce the K-2SO trial

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
These commands reproduce engine-only auditions. For normal ST usage, use the
[integrated service and provider](pocket-tts-sillytavern.md).

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

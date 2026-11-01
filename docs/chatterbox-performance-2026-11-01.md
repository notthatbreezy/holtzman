# Chatterbox latency investigation — 2026-11-01

The useful improvement was **Turbo + BF16 + MIOpen FAST**, while retaining
`HOLT-K2SO.wav`. Short replies through the running HTTP service took **1.88–2.39 s**
after conditioning warmed. The first request after starting the service took
**8.63 s**. Those are synthesis times, excluding reasoning and tool calls.

Selecting a predefined voice alone did not solve the delay. The original model's
Thomas sample took 8.71 s versus K-2SO's 10.15 s, but also spoke a shorter clip.
Normalized time/audio ratios were nearly equal: 1.72 versus 1.74. Both paths encode
a reference and then run the same generator; reference conditioning is cached.

## Measurements

Hardware: Radeon 8060S, native Windows ROCm 7.2.1, PyTorch 2.9.1, eight CPU threads.
Qwen remained loaded in Ollama. No game controls or hosted inference were used.
Automatic ST narration was paused for the isolated measurements and restored after.

The fixed sentence was: “Captain, docking clearance is confirmed. Try to arrive
with the same number of hull panels we left with.” Fixed seed 42, temperature .7,
exaggeration .4, CFG .5. Turbo ignores CFG/exaggeration. Direct engine output is
24 kHz; the HTTP checks also include the configured 1.1 speed postprocessing.

| Engine / precision / voice | Warm synthesis | Audio duration | Time/audio |
| --- | ---: | ---: | ---: |
| Original FP32 / K-2SO | 10.15 s | 5.84 s | 1.74 |
| Original FP32 / predefined Thomas | 8.71 s | 5.08 s | 1.72 |
| Original FP32 / model's built-in conditioning | 8.62 s | 5.44 s | 1.58 |
| Original BF16 / K-2SO | 6.84 s | 6.44 s | 1.06 |
| Turbo FP32 / K-2SO | 10.64 s | 5.80 s | 1.83 |
| Turbo BF16 / K-2SO | 3.43 s | 7.12 s | .48 |
| Turbo BF16 + FAST / K-2SO | 2.94 s | 7.12 s | .41 |

Each row is the mean of two repeats after its first run, except FAST's fixed-line
row, which has one warm repeat. Different models/voices produce different pacing
and durations; these are not identical waveforms. Original package was 0.1.1;
Turbo package is published `chatterbox-tts==0.1.6`. No ROCm/PyTorch upgrade occurred.

### New sentences, rather than repeated identical text

MIOpen's default solver search imposed substantial first-use overhead for new
waveform lengths. Merely reporting repeated-text timings would have overstated
conversational performance. With Turbo BF16 and a warm reference cache:

| Previously unseen text | Default find mode | FAST mode | Audio duration |
| --- | ---: | ---: | ---: |
| Systems online, Captain. Ready when you are. | 7.04 s | 1.50 s | 3.04 s |
| Your fuel is at sixty percent. We have enough for the next jump. | 8.06 s | 1.72 s | 3.64 s |
| Hull integrity is falling, Captain. Stop demonstrating the docking clamps with the rest of the ship. | 10.24 s | 2.24 s | 4.96 s |

The FAST run preceded the default-mode confirmation run. This is a small practical
comparison, not randomized research; kernel caches and process startup affect
results. Stage timings nevertheless locate the difference in waveform generation:
default mode took 5.98–8.51 s there, versus .43–.48 s with FAST. Speech-token time
remained approximately 1.04–1.74 s in both runs.

Local Whisper transcribed the FAST fixed line and all three new lines correctly,
allowing formatting differences such as “60%.” Original FP32 and BF16 clone checks
also preserved the sentence. ASR checks intelligibility, not voice similarity or
subjective audio quality; Turbo can have different pacing/timbre with the same clip.

### Actual running-service checks

| K-2SO request | HTTP time | Returned audio |
| --- | ---: | ---: |
| First request after startup: Systems online… | 8.63 s | 2.76 s |
| Following request: Your fuel is at sixty percent… | 1.88 s | 3.31 s |
| Following request: Hull integrity is falling… | 2.39 s | 4.51 s |

HTTP timings include response encoding and 1.1 speed adjustment. All files were
nonempty valid mono WAV. Model-load time was about eight seconds before these
requests; do not describe cold startup as a two-second operation.

## Applied configuration

- One existing Chatterbox environment and one server on `127.0.0.1:8004`.
- Package `chatterbox-tts==0.1.6`; GPU dependencies retained. Exact working pins:
  [Turbo ROCm lock](../integrations/sillytavern/chatterbox-turbo-windows-rocm.lock.txt).
- Private `config.yaml`: `model.repo_id: chatterbox-turbo`.
- The normal launcher sets `TTS_BF16=auto` and `MIOPEN_FIND_MODE=FAST`, scoped to
  Chatterbox. BF16 support was verified on this GPU.
- The repository's `run-chatterbox.py` loads the pinned public Turbo checkpoint
  `749d1c1a46eb10492095d68fbcf55691ccf137cd`. It downloads only runtime files
  (about 3 GB), reuses the existing HF cache, and fixes 0.1.6's unnecessary token
  requirement without editing the installed package. No HF credential was needed.
- ST automatic narration restored; both paragraph-narration options enabled.
  This lets ST queue/play paragraphs instead of waiting for a whole long reply.
  Setting server `stream=true` alone would not suffice: the installed provider
  normally waits for `response.blob()`.
- The current speaker named “Assistant” was mapped to `[Clone] HOLT-K2SO.wav`;
  it had inherited a disabled default voice. The K-2SO reference was retained.
- Managed Stop now waits for its validated processes to exit, fixing a short
  port-release race during immediate Stop/Start. Port closure and restart passed.

Final checks: the live API reports Turbo loaded on the GPU; saved ST settings
confirm automatic narration, both paragraph options and the K-2SO mappings.
ST-originated clone requests completed successfully through the running server.
The final browser audio element was idle, so these checks do not establish which
physical output the listener heard after this change. No further chat message was
injected while the commander was using ST. Python syntax, PowerShell parsing and
Git whitespace checks passed. The earlier browser-playback checks remain in the
installation guide; subjective listening to the new Turbo voice is still useful.

Normal operation remains:

```powershell
.\HOLT-Chatterbox.cmd start
.\HOLT-Chatterbox.cmd status
.\HOLT-Chatterbox.cmd stop
```

To start without the new math/kernel settings, stop first, then use
`HOLT-Chatterbox.cmd start -Precision off -KernelMode DEFAULT`. These parameters
apply at process start, not to an already running server.

To fully restore the original engine: stop Chatterbox, reinstall
`.local/holt/chatterbox-engine` with `uv pip install --no-deps --python` pointing
at its existing venv, set `model.repo_id: chatterbox` in the private config, and
start with `-Precision off -KernelMode DEFAULT`. Original CPU/GPU locks and source
remain available. Private pre-test config backup is under `.local/holt/results`.

## Reproduction and evidence

Run `scripts/profile-chatterbox.py` with the Chatterbox GPU venv while its server
is stopped. It checks for an occupied port, profiles conditioning/token/waveform/
watermark stages with synchronized GPU boundaries, records cold and warm cases,
and saves local WAV samples. `--varied` adds the conversational sentences above.
Use `--model turbo --checkpoint <absolute cached snapshot>` for Turbo and scope
`MIOPEN_FIND_MODE=FAST` to that process for the fast-mode comparison. The script
does not change production configuration or play its samples.

For HTTP auditions, `scripts/check-chatterbox.py` supports `--voice-mode clone`
and `--voice-mode predefined --voice Thomas.wav`. All benchmark text is synthetic.
[Raw measurements](benchmarks/chatterbox-2026-11-01.json) contain 34 engine trials,
three HTTP checks and the ASR checks. Audio remains in `.local/holt/results`.

Upstream references: [Chatterbox model family](https://github.com/resemble-ai/chatterbox),
[server performance options](https://github.com/devnen/Chatterbox-TTS-Server#-performance-tuning),
[AMD MIOpen find modes](https://rocm.docs.amd.com/projects/MIOpen/en/docs-7.2.1/reference/env_variables.html#find-mode-configuration).

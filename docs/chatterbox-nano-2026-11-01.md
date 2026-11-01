# Nano versus Turbo — 2026-11-01

Nano is enabled for a listening trial in the **same existing Chatterbox environment**.
It improves synthesis latency on this PC, but its long-form pronunciation/text
fidelity needs listening judgment. No startup warmup was added: the measurements
show reference conditioning reused on following requests, and no evidence of
Chatterbox unloading between them.

## What “3×” means

The [official Nano model card](https://huggingface.co/ResembleAI/chatterbox-nano)
advertises three times faster than real-time on an eight-core CPU. It does not
claim three times faster than Turbo, and our GPU test does not establish CPU
performance. Both Nano and Turbo were already faster than real-time when warm.

## Controlled GPU comparison

Radeon 8060S, Windows ROCm 7.2.1, PyTorch 2.9.1, Transformers 4.46.3, eight CPU
threads, BF16 enabled, `MIOPEN_FIND_MODE=FAST`. Qwen remained resident in Ollama.
Automatic ST narration was paused during isolated synthesis and restored after.
No reasoning requests or game controls were sent by these benchmarks. Nano ran
first, then Turbo in a separate process; kernel caches can affect comparisons.

Both used `HOLT-K2SO.wav`, seed 42, temperature .7, and the same input text.
The fixed docking line has two warm repeats after its cold trial. Other rows
have one trial with the voice already conditioned. Direct engine output uses
speed 1.0; audio duration changes with the model even for identical text.

| Input | Turbo synthesis | Nano synthesis | Wall-time gain | Turbo / Nano audio |
| --- | ---: | ---: | ---: | ---: |
| Docking line, warm mean | 2.20 s | 1.08 s | 2.03× | 7.12 / 5.64 s |
| Systems online… | 1.16 s | .82 s | 1.41× | 3.04 / 3.00 s |
| Your fuel is at sixty percent… | 1.31 s | .97 s | 1.35× | 3.64 / 3.84 s |
| Hull integrity is falling… | 1.72 s | 1.23 s | 1.39× | 4.96 / 5.24 s |
| Longer navigation briefing, unsplit | 10.32 s | 5.45 s | 1.89× | 27.48 / 26.40 s |

Docking time/audio ratios were .308 for Turbo and .192 for Nano: about 1.61×
better normalized throughput. The larger wall-time gain also reflects Nano's
shorter delivery. For the briefing, normalized throughput improved about 1.82×.
This is a small practical benchmark, not a general performance guarantee.

## Actual running Nano service

`http://127.0.0.1:8004/tts`, clone voice, WAV, configured speed factor 1.1:

| Request | HTTP time | Audio duration |
| --- | ---: | ---: |
| First docking line after startup | 8.86 s | 5.13 s |
| Following identical docking line | 1.56 s | 5.13 s |
| Briefing with ST's split_text=true, chunk_size=120 | 9.27 s | 26.27 s |

Model initialization was about 4.88 seconds in the offline Nano profiler;
first-request timing is additional. Cold requests remain slower. Splitting the
briefing generated six chunks and adds overhead versus the single engine call.
The provider still waits for the returned WAV; Nano does not itself fix buffering.
These timings exclude reasoning, tool calls and browser playback startup.

## Intelligibility checks

Local Whisper small.en, CPU, transcribed five samples from each engine plus two
Nano HTTP samples. Greeting and fuel lines matched both models (ignoring numeric
formatting). Turbo's docking line matched; Nano's was transcribed with “at” instead
of “with” and one rendering of hull as “whole.” Nano's unsplit briefing had a
garbled passage around “considerably less quiet.” The split HTTP briefing removed
that passage but still transcribed “shaking” for “checking” and “plants” for
“clamps.” Turbo's long sample also had a clamps/clams substitution.

ASR can introduce its own errors and does not measure voice similarity. These
results are a reason to listen, not proof every substitution originated in TTS.
Do not report Nano as equally faithful or that every sample passed verbatim.
Prepared WAVs and transcripts are private under `.local/holt/results`.

## Installation and operation

- Existing package remains `chatterbox-tts==0.1.6`; no GPU or Transformers upgrades.
- Two unmodified official source files from commit
  `5de7a54aa4e5e2baadb0182dde554908b48b85c2`, with MIT license, are stored in
  [nano-compat](../integrations/sillytavern/chatterbox-nano-compat/README.md).
  They add the official GPT2-small configuration and Nano loader. Runtime code
  imports them under a separate module name and retains normal Turbo selection.
- Nano weights pinned to `71ccd1d0081b430592cea481f4307e764e07bc64`.
  The new T3 file is approximately 870 MB; large checkpoint SHA256 values were
  verified against the HF metadata. The matching cached Turbo decoder was reused.
- Private config: `model.repo_id: chatterbox-nano`. Same port, environment,
  reference, speed and ST mappings. Normal `HOLT-Chatterbox.cmd start/stop/status`
  commands work. Status now names the variant.
- Health identifies `class_name: ChatterboxNanoTTS`, `variant: Nano`, `device: cuda`.
  The server's internal `type: turbo` denotes the shared cache/tag implementation.
  The upstream Web UI's model dropdown does not add a Nano choice automatically.
- ST automatic speech was restored after tests; no unsent chat draft was replaced.
  Its K-2SO voice preview was triggered through the normal Chatterbox provider;
  status returned to Ready with no preview error logged. The saved settings show
  narration enabled and HOLT mapped to the same reference. Physical audibility
  and preferred voice quality still require the listener's judgment.

Validation also passed Python syntax, PowerShell parsing, whitespace checks, and
Nano/Turbo/original selector routing without loading a second speech model.

To switch back: stop Chatterbox, edit only `model.repo_id` in
`.local/holt/chatterbox-server/config.yaml` to `chatterbox-turbo`, then start again.
No reinstall or download is required. The pre-Nano config is also backed up in
`.local/holt/results/chatterbox-before-nano-2026-11-01.yaml`.

## Evidence and reproduction

[Raw measurements and ASR](benchmarks/chatterbox-nano-2026-11-01.json) include 14
engine trials and three HTTP trials, all synthetic text. Stop the server before
running `scripts/profile-chatterbox.py --model nano --precision bf16 --varied --long`
with the existing Chatterbox Python and process environment
`MIOPEN_FIND_MODE=FAST`. Use `--model turbo --checkpoint <cached Turbo snapshot>`
for its comparison. The profiler never edits production configuration.
`scripts/check-chatterbox.py --split-text --text <briefing> --output <new WAV>`
checks the running service's chunked path.

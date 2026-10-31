# Chatterbox speech for standalone SillyTavern

Setup on Archie, 2026-10-31. This serves speech locally at
`http://127.0.0.1:8004`; SillyTavern remains at `http://127.0.0.1:8000`.
It is independent of the old HOLT desktop/gateway speech service.

## Everyday start and stop

From the repository folder in PowerShell:

```powershell
cd C:\Users\chris\holtzman
.\HOLT-Chatterbox.cmd start
.\HOLT-Chatterbox.cmd status
.\HOLT-Chatterbox.cmd stop
```

Double-clicking **HOLT-Chatterbox.cmd** also starts it. It runs in the background;
closing a browser tab does not stop it. Use `stop` when finished. The launcher does
not install an automatic startup task. Start Chatterbox before using ST's TTS.
Initial model loading takes time even after the files are cached. A responding
API alone does not prove generation works; use a voice preview as the final check.

The launcher records its process ID and start time, validates its executable and
server command before stopping it, and includes the Windows venv Python child.
It does not stop Ollama, SillyTavern or the older HOLT services. Another service
already listening on port 8004 prevents a second managed copy from starting.

Web UI: `http://127.0.0.1:8004/`. API documentation: `/docs` on the same server.
Upstream opens its Web UI after model loading; leaving that page open does not
itself start synthesis. Clicking Generate does share the queue with ST.
Logs: `.local/holt/results/chatterbox-stdout.log` and `chatterbox-stderr.log`, plus
`.local/holt/chatterbox-server/logs/tts_server.log`. Logs may contain spoken text;
keep them private. Launcher output logs are replaced on each start.

## What is installed

| Component | Location / version |
| --- | --- |
| Server | `.local/holt/chatterbox-server`, devnen/Chatterbox-TTS-Server commit `915ae289340e10c6047f27f47e22eae9bf350c32` |
| Engine source | `.local/holt/chatterbox-engine`, devnen/chatterbox-v2 commit `df73ac81662c9354e0d408832a876ddd72a42480` |
| Python | Private Python 3.12.15; server's single `venv` |
| Model | Original English Chatterbox, `ResembleAI/chatterbox` |
| Weight revision downloaded | `5bb1f6ee58e50c3b8d408bc82a6d3740c2db6e18` |
| Execution | Radeon 8060S via native Windows ROCm 7.2.1 / PyTorch 2.9.1; 8 OpenMP/MKL threads for CPU work |
| Model cache | `.local/holt/chatterbox-server/hf-cache` |
| Voice references | `.local/holt/chatterbox-server/reference_audio` |

This uses the original English model because the server's recommended engine fork
at the pinned commit supplies that model. The server UI also advertises Turbo and
Multilingual, but those classes are absent from this engine build. Do not select
them and assume they were installed. The server's own Strix Halo instructions
describe Linux/Docker ROCm. This installation instead uses AMD's documented native
Windows PyTorch packages, which list the Ryzen AI Max+ 395 as supported. The
installed Radeon driver was `32.0.32015.7029`; no driver change was required for
the GPU tensor check. In this backend `device: cuda` means AMD ROCm through
PyTorch's CUDA-compatible API, not NVIDIA hardware. Model loading confirmed
`device: cuda`. CPU files were replaced in place with the user's explicit approval;
there is no second Chatterbox virtual environment or concurrently running server.

Sources: [server and installation instructions](https://github.com/devnen/Chatterbox-TTS-Server),
[engine fork](https://github.com/devnen/chatterbox-v2), and
[original Chatterbox project](https://github.com/resemble-ai/chatterbox).
AMD sources: [native Windows installation](https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/install/installryz/windows/install-pytorch.html)
and [Ryzen support matrix](https://rocm.docs.amd.com/projects/radeon-ryzen/en/latest/docs/compatibility/compatibilityryz/windows/windows_compatibility.html).
ROCm 7.2.1 is the pinned release used here, not a claim about the newest ROCm SDK.

## Reinstall from a fresh Holtzman checkout

Prerequisites: Git, internet access, and HOLT's bundled `uv` at
`.local/holt/runtime/uv/uv.exe` (or install uv from its official distribution and
substitute its executable). Run from the repository root. These commands are for
a **fresh** installation; do not clone over existing folders. Stop the service
before repairing an existing environment, and preserve its reference audio and
configuration.

```powershell
$env:UV_PYTHON_INSTALL_DIR = Join-Path (Get-Location) '.local/holt/python'
$env:UV_CACHE_DIR = Join-Path (Get-Location) '.local/holt/uv-cache'
$uv = '.local/holt/runtime/uv/uv.exe'

git clone https://github.com/devnen/Chatterbox-TTS-Server.git .local/holt/chatterbox-server
git -C .local/holt/chatterbox-server checkout 915ae289340e10c6047f27f47e22eae9bf350c32
git clone https://github.com/devnen/chatterbox-v2.git .local/holt/chatterbox-engine
git -C .local/holt/chatterbox-engine checkout df73ac81662c9354e0d408832a876ddd72a42480

& $uv venv --python 3.12.15 .local/holt/chatterbox-server/venv
$python = '.local/holt/chatterbox-server/venv/Scripts/python.exe'
& $uv pip install --python $python --no-deps -r integrations/sillytavern/chatterbox-windows-rocm.lock.txt
& $uv pip install --python $python --no-deps .local/holt/chatterbox-engine
& $python -c 'import chatterbox.tts, torch, perth; assert callable(perth.PerthImplicitWatermarker); print(torch.__version__); assert torch.cuda.is_available(); print(torch.cuda.get_device_name(0)); x=torch.randn(256,256,device="cuda"); print(float((x@x.T).mean()))'
```

The lock records the installed dependency versions. `--no-deps` is deliberate:
the upstream server already installs the engine without dependencies to avoid its
conflicting torch/ONNX requirements. This setup additionally pins NumPy 1.26.4 and
protobuf 3.20.3. The default protobuf 3.19.6 failed to import ONNX; the replacement
passes the Chatterbox import check. Setuptools 80.9.0 supplies `pkg_resources`,
which Perth's watermarker needs; the initially resolved 84.2.1 removed it and
caused model loading to fail. Watermarking is retained. Package metadata checks can still flag the old
`descript-audiotools` protobuf ceiling and the engine's torch pin; do not let an
unqualified dependency upgrade replace this working environment.

Edit `.local/holt/chatterbox-server/config.yaml` before the first start:

```yaml
server:
  host: 127.0.0.1
  port: 8004
model:
  repo_id: chatterbox
tts_engine:
  device: cuda
  default_voice_id: Thomas.wav
generation_defaults:
  temperature: 0.7
  exaggeration: 0.4
  cfg_weight: 0.5
  speed_factor: 1.1
  language: en
```

Merge those values into the existing sections; retain the other upstream paths
and audio settings. Then run `HOLT-Chatterbox.cmd start`. First startup downloads
the model weights from Hugging Face. Later starts reuse the cache. The download
needs no API key. If startup waits on model downloads, inspect the logs and cache;
do not start multiple servers against the same cache. `hf-xet` is included in the
lock because the initial plain HTTP weight download stalled during setup and the
accelerated retry completed.

### CPU reinstall fallback

The previous CPU dependency lock is retained; a second installed environment is
not needed. If GPU mode fails, `device: cpu` in config can first be tried with the
same GPU-capable PyTorch environment. Stop/start the service after editing it.
For a full return to the measured original CPU runtime, stop Chatterbox and rebuild
**only its existing venv**:

```powershell
.\HOLT-Chatterbox.cmd stop
$env:UV_PYTHON_INSTALL_DIR = Join-Path (Get-Location) '.local/holt/python'
$env:UV_CACHE_DIR = Join-Path (Get-Location) '.local/holt/uv-cache'
$uv = '.local/holt/runtime/uv/uv.exe'
$python = '.local/holt/chatterbox-server/venv/Scripts/python.exe'
# --clear replaces the packages in this exact Chatterbox venv, not voices/models.
& $uv venv --clear --python 3.10.22 .local/holt/chatterbox-server/venv
& $uv pip install --python $python --no-deps --extra-index-url https://download.pytorch.org/whl/cpu --index-strategy unsafe-best-match -r integrations/sillytavern/chatterbox-windows-cpu.lock.txt
& $uv pip install --python $python --no-deps .local/holt/chatterbox-engine
```

Then change `tts_engine.device` to `cpu` in `config.yaml` and start the service.
To return to GPU later, repeat the venv rebuild with Python 3.12.15 and the ROCm
lock instead, and set the device to `cuda`. No model re-download is needed while
the `hf-cache` folder is retained. Do not run the venv-replacement commands against
the older HOLT `voice-env`, which is a different service's environment.

## K-2SO reference audio

User-supplied source clips are in `C:\Users\chris\Downloads\K2SO-Samples`.
The original MP3 files are untouched. Prepared private references:

- **HOLT-K2SO.wav**: 24 kHz mono PCM16, approximately 10.91 seconds. Combines
  `Use Against You.mp3`, `Vague.mp3`, `Congratulations.mp3` and `Bad Idea.mp3`,
  with 120 ms gaps. This is a starting reference, not a guarantee of clean dialogue
  or exact character similarity.
- **HOLT-K2SO-single.wav**: the single `Use Against You.mp3` clip converted to
  24 kHz mono PCM16, approximately 4.14 seconds, for comparison.

Keep references under the server's `reference_audio` folder, or upload them through
its Web UI. References and generated audio stay in ignored `.local` storage;
they are not committed to the repository. No external voice-cloning service is
used. Background effects in a reference may carry into the generated voice.

For a different reference, use clear speech, little background music/effects and
one speaker. The original model uses the beginning of the clip for conditioning
(first 6 seconds for encoder tokens and up to 10 seconds for decoder conditioning
in the pinned engine). A long compilation is not automatically better.

To prepare a new reference yourself, run the converter with Chatterbox's Python
(it has the audio libraries). Choose a new output filename; the script refuses to
overwrite an existing reference or source clip:

```powershell
$python = '.local/holt/chatterbox-server/venv/Scripts/python.exe'
& $python scripts/prepare-chatterbox-reference.py 'C:/Users/chris/Downloads/K2SO-Samples/Use Against You.mp3' 'C:/Users/chris/Downloads/K2SO-Samples/Vague.mp3' --output .local/holt/chatterbox-server/reference_audio/HOLT-K2SO-alternative.wav
```

It averages stereo channels, resamples to 24 kHz, adds a 120 ms gap between clips
and reduces peaks above 0.95. It does not remove music or other voices. Click
**Reload** in ST's TTS settings after adding a reference, then select the new clone.
The original setup compilation included a final 120 ms gap; this converter only
adds gaps between clips, so reproducing it has slightly less trailing silence.

## SillyTavern configuration

In **Extensions → TTS**:

1. Select **Chatterbox**.
2. Set Server Endpoint to `http://127.0.0.1:8004` and click **Reload**.
3. Set HOLT's voice to **[Clone] HOLT-K2SO.wav**. Keep the default voice disabled
   and user-message narration off if only HOLT should speak.
4. Enable TTS and Auto Generation for automatic playback of future replies.
5. Start with temperature 0.7, exaggeration 0.4, CFG 0.5, speed 1.1 and WAV output.
   These are ST provider settings and override server defaults when sent.

The reference voice's internal ID is `ref_HOLT-K2SO.wav`; the `ref_` prefix tells
ST's provider to send clone mode and the reference filename to `/tts`. A successful
preview/generation is stronger evidence than the provider's Ready label alone.
ST plays audio through the browser's Windows audio output. This setup does not
change Windows defaults, microphone selection, VBAN or game audio routing.

Test speech without generating a new model response or sending game controls by
entering this in ST's message box and pressing Enter:

```text
/speak voice="HOLT" Systems online, Captain. Ready when you are.
```

## How the reference voice is reused

Chatterbox uses one pretrained speech model. It encodes the reference into voice
conditioning data; it does not train a separate model for each character. The
server caches the conditionals in memory, keyed by reference path, file modification
time and exaggeration value. Repeated requests can reuse this data. Changing the
reference/exaggeration or restarting the server requires encoding it again.

Every new utterance still needs speech-token generation and waveform synthesis.
The conditioning cache saves that preparation step; it does not eliminate the
main synthesis cost or make all future sentences instant. The server is not
configured to save and replay a persistent cache of complete utterances.

## Testing record

CPU installation, model loading, reference-voice generation and ST playback passed.
The documented start/stop/status commands were exercised. A `/speak voice="HOLT"`
request went through ST's Chatterbox provider; its `tts_audio` element reached the
end of its 2.8-second output with no media error. This verifies browser playback,
not which physical speakers the user heard.

| CPU audition | Request time | Audio duration | Time / audio |
| --- | ---: | ---: | ---: |
| Composite reference, first test | 39.688 s | 7.745 s | 5.124 |
| Single reference, following test | 16.188 s | 4.127 s | 3.922 |

Both requested the same docking-clearance line with seed 42 and the starting
settings above. These are initial observed timings, not a controlled warmed-up
CPU/GPU benchmark: references, output durations and cache/JIT state differ. Local
Whisper transcribed substantially the requested sentence from both files, with
minor word substitutions; voice similarity still needs listening judgement.

To repeat a local audition using HOLT's existing Python with httpx:

```powershell
.local/holt/voice-env/Scripts/python.exe scripts/check-chatterbox.py --voice HOLT-K2SO.wav
```

Audio and timing JSON are saved privately under `.local/holt/results`. Use
`--output` to preserve separate trials. No hosted inference or game action is used.

The CPU environment was replaced in place for an explicitly approved GPU test;
the CPU lock above remains a reinstall fallback. GPU model loading and synthesis
passed on the Radeon 8060S. Local Whisper transcribed the full requested sentence
from the final GPU sample correctly.

| GPU audition, composite reference | Request time | Audio duration | Notes |
| --- | ---: | ---: | --- |
| First run | 78.391 s | 5.309 s | First-use MIOpen activity; shared server was also being explored in the Web UI |
| Warm repeat | 11.016 s | 5.309 s | Same text, seed and reference |
| Later repeat, queued | 80.938 s | 5.309 s | Waited behind a six-chunk Web UI generation; own synthesis logged about 10 s |
| Immediately following repeat | 11.016 s | 5.309 s | No preceding job; same output duration |

The warm request time was repeatable at about 11 seconds, but this is still a
noticeable conversational delay. Do not report a controlled CPU/GPU speedup ratio:
the original CPU runs had different audio durations/cache state and some GPU runs
were affected by simultaneous Web UI use. The API and ST provider share one
process; Web UI generation can queue ahead of ST and make it appear slow. Closing
the UI alone need not cancel a generation already running. Avoid simultaneous
generation while timing; a service restart clears the running queue.

After these tests, stop/status/start were exercised on the GPU environment too.
Only the single managed server is started for normal use. ST's saved settings were
read back: Chatterbox enabled, automatic generation enabled, user narration off,
endpoint `http://127.0.0.1:8004`, HOLT mapped to `[Clone] HOLT-K2SO.wav`, and
temperature/exaggeration/CFG/speed `0.7/0.4/0.5/1.1`.

After that restart, ST's `/speak` command generated and played a new greeting on
the GPU. Browser `tts_audio` reached the end of its **2.655-second** file with no
media error. The request began at approximately 12:55:55 local time and the server
finished synthesis at 12:56:18: allow roughly **23 seconds for that first utterance
after restart**, even with weights cached. Subsequent warm synthesis is faster;
do not describe first-use latency as 11 seconds. This original-model setup is
functional, but not instant conversational TTS. A shorter/optimized model could
be a future comparison; Turbo was not benchmarked here.

Ready settings screenshot is private at
`.local/holt/results/chatterbox-sillytavern-ready-2026-10-31.png`. The GPU audition
`chatterbox-HOLT-K2SO-gpu-repeat2.wav` and its timing JSON are in the same folder.

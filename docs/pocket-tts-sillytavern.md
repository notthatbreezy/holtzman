# Pocket TTS in SillyTavern

Pocket is installed, running and selected for HOLT. It synthesizes the K-2SO voice
locally on CPU and streams speech into a continuous browser audio schedule.
Chatterbox is retained as an alternative. No SillyTavern core files were edited
for this integration.

## Use it

1. Run **`HOLT-ST-Voice.cmd start`** as usual. When ST's saved provider is
   **HOLT Pocket**, this also starts Pocket and waits for it to be ready.
2. Open ST at `http://localhost:8000/` and open the HOLT chat you want to use.
3. In **Extensions → HOLT voice input**, click **Start listening** after a page
   refresh or chat change. Hold **Numpad Enter on SKIPPY** to speak into this PC's
   selected/default microphone. Typed messages work as usual.
4. HOLT's replies automatically play through the browser's Windows-default audio
   output. If the browser suspends audio, use **Extensions → TTS → Enable audio**.

The current chat has already been reopened, Pocket selected, HOLT mapped to
**HOLT K-2SO**, and PTT listening armed. No new chat is required for this change.

## Start, stop and status

Double-click **`HOLT-Pocket.cmd`** for a small start/stop/status menu. From the repo:

```powershell
.\HOLT-Pocket.cmd start
.\HOLT-Pocket.cmd status
.\HOLT-Pocket.cmd stop
```

These commands control the Pocket service only. The normal voice launcher now
also manages it:

```powershell
.\HOLT-ST-Voice.cmd start   # Start input bridge; also Pocket when selected in ST
.\HOLT-ST-Voice.cmd status  # Report input bridge and Pocket status
.\HOLT-ST-Voice.cmd stop    # Stop input bridge and managed Pocket process
```

SillyTavern itself stays running independently. Pocket's model and both saved
voice states remain loaded until its service stops. There is no per-request
reload or idle unload timer. Normal startup uses cached files offline; it requires
no hosted API or online credential check.

To change the output voice, open **Extensions → TTS**, choose **HOLT Pocket**, and
set HOLT's voice to either **HOLT K-2SO** (the original benchmark reference) or
**HOLT K-2SO — short reference**. The default voice is disabled, so Commander
Brown's messages are not narrated. The service uses two CPU threads.

To return to Chatterbox, start it with `HOLT-Chatterbox.cmd start`, then select
**Chatterbox** in ST's TTS provider menu. Its saved voice mapping is preserved.
You can stop Pocket separately afterward. Select a different provider before
disabling/removing its extension, so ST does not retain an unavailable provider.

## Playback and scope

The extension reads 24 kHz mono PCM chunks as they arrive. It schedules each
buffer immediately after the previous buffer on Web Audio's clock, using a
**100 ms initial buffer**. It does not queue each 80 ms piece as a separate HTML
audio clip. ST's normal voice mapping, text selection and automatic narration
still decide what HOLT says.

ST receives an async-generator provider whose job remains active until the last
scheduled audio buffer ends. No dummy silence clips or extra HTML audio jobs are
created. The microphone bridge sees Pocket as speaking during generation **and
the complete playback tail**, then resumes accepting PTT normally.

The provider shows first-audio scheduling latency and detected gap count. Those
measure audio scheduled by the browser, not sound physically measured at the
headphones. Browser/device buffering, speech recognition, reasoning and game
tools add their own latency. ST's existing text/paragraph detection can also delay
when a TTS job starts; this integration does not rewrite that part of ST.

The initial version focuses on smooth sequential playback. Use **Stop audio**
in Pocket's settings for a manual stop. Full interruption/swipe/chat-change
cancellation behavior was explicitly deferred; native ST's global stop is not
claimed to cancel every Pocket playback path. Stopping the backend does not
necessarily silence audio already buffered in the browser.

The service applies a consistent 0.9 gain and clips only residual out-of-range
samples before PCM conversion. No extra robot filter, pitch shift or speaking-rate
adjustment is applied. ST's generic Audio Playback Speed slider controls its HTML
audio player and does not change this provider's Web Audio speed.

## Installation and files

Pocket's environment, model downloads and HF access were prepared during the
[voice trial](pocket-tts-trial-2026-11-01.md). The environment dependency lock is
`requirements/pocket-tts-windows.lock.txt`. With it installed, copy the extension:

```powershell
$pocketExtension = "$env:USERPROFILE\SillyTavern\public\scripts\extensions\third-party\holt-pocket-tts"
New-Item -ItemType Directory -Force $pocketExtension | Out-Null
Copy-Item integrations/sillytavern/pocket-tts/* $pocketExtension
Copy-Item integrations/sillytavern/voice-input/index.js,integrations/sillytavern/voice-input/manifest.json `
  "$env:USERPROFILE\SillyTavern\public\scripts\extensions\third-party\holt-voice-input"
```

Start Pocket, refresh ST, and select the provider/voice as above. The extension
registers before ST's built-in TTS initialization so saved selections survive a
refresh without an unknown-provider constructor error. It temporarily suppresses
only the registration API's premature auto-load before ST's settings DOM exists;
the saved selection is restored synchronously and loaded by normal ST startup.

The service is `python -m client.pocket_service`, bound to **127.0.0.1:8005**.
Its browser origins are limited to ST on localhost/127.0.0.1 port 8000; it has no
game-control or reasoning-model access. Endpoints are `/health`, `/voices` and
`POST /stream`. One synthesis runs at a time. The stream has a start record, PCM
records and an explicit completion/error record, so a broken connection does not
look like successful speech completion.

Voice states are cached under `.local/holt/models/pocket-voices/`, keyed by the
reference hash, model language and package version. Replacing a reference WAV
causes reconditioning at the next service startup. The launcher stores process
identity under `.local/holt/run/pocket.json`; stop checks PID, start time, executable
and module before stopping it and its matching Python child. It does not kill
unrelated Python processes.

Private logs: `.local/holt/results/pocket-stdout.log` and `pocket-stderr.log`.
If port 8005 is occupied, startup refuses to launch an additional copy. If cloning
weights or references are missing, status reports not ready; consult the log and
the trial installation instructions. Model files, credentials and voice recordings
are not committed.

## Verification — 2026-11-01

- A live service request delivered its first audio over HTTP in **87 ms**, with
  later chunks arriving incrementally. The model was loaded before the request.
- The browser preview completed with **271 ms first scheduled audio**, **0 gaps**.
- A long briefing submitted through ST's normal `/speak voice=HOLT` command
  completed with **228 ms first scheduled audio**, **0 gaps**. Speaking state
  remained true through playback and cleared afterward.
- A normal HOLT chat request produced “Systems online, Captain. Ready when you
  are.” Automatic Pocket narration completed at **180 ms first scheduled audio**,
  **0 gaps**, then returned to Ready.
- Start, stop, stopped status, restart, idempotent start and selected-provider
  startup were exercised. The normal HOLT voice launcher and PTT connection were
  checked, and listening was armed for the user's test.
- **21 Node checks** passed across existing voice/schema/image integration and
  Pocket scheduling, transport failures, saved-provider registration and playback
  tail gating. **4 model-free Python API checks** passed for PCM streaming,
  explicit failure, validation/busy handling, host and browser-origin boundaries.
  Python syntax and PowerShell parsing also passed.

No game controls were sent during these tests. Physical audibility, subjective
voice quality and preferred volume remain the user's listening check. Browser
proof is retained privately under `.local/holt/results/`.

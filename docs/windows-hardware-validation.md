# Windows ship-computer validation

Inspected October 26, 2026 on the second PC. The memory correction does not
require replacing the PC or changing the two-PC architecture. Capacity is
adequate for the proposed quantized model; latency and accuracy still need
measurement. This is a preparation checkpoint, not a completed installation.

## Hardware and graphics

| Measurement | Result |
| --- | --- |
| Computer | Corsair AI Workstation 300 |
| CPU | Ryzen AI Max+ 395, 16 cores / 32 threads |
| GPU | Radeon 8060S |
| Installed memory | Eight 16 GiB devices, 128 GiB total |
| Windows-visible system memory | 33,981,009,920 bytes, approximately 31.6 GiB |
| Likely graphics reservation | 96 GiB, inferred from installed versus system memory |
| Initial graphics driver | 32.0.22018.5, dated September 17, 2025 UTC |
| Initial Vulkan GPU API | 1.4.320, AMD proprietary driver 25.20.18 |
| BIOS | AXB35-02 3.07, October 16, 2025 UTC |
| Windows | Windows 11 Home, build 26200 |

Vulkan reports device-local heaps of 32 GiB and 64 GiB, with budgets of
30.4 GiB and 60.8 GiB respectively. It also reports a 15.82 GiB nonlocal
heap. llama.cpp reports the GPU as 114,507 MiB total and 108,782 MiB free.
These are driver/runtime accounting views, not extra physical memory.
Only successful allocations and model loading establish what is usable.
No BIOS or graphics-memory allocation settings were changed.

## Prepared runtime

Local, ignored installation root: `.local/holt/` under this checkout.

- llama.cpp v0.7.0 resolves to build b11669. Official Windows x64 Vulkan ZIP
  SHA-256: `2c07574e7bde71321cb785282b20cc821e951132149eeceeea32ddebdbf0598e`.
  Download hash verified against the GitHub release asset digest.
- Model repository: `unsloth/Qwen3.5-35B-A3B-GGUF`, revision
  `bc014a17be43adabd7066b7a86075ff935c6a4e2`.
- Q4_K_M weights: 22,016,023,168 bytes. Matching F16 projector:
  899,283,648 bytes. Expected file hashes are pinned in the resumable
  `scripts/download-holt-model.py`; both downloads passed SHA-256 verification.
- Standalone Python 3.12.15 in `.local/holt/python/` with its isolated
  environment in `.local/holt/voice-env/`, MCP SDK 2.3.0, Windows keyring and
  jsonschema installed. `pip check` passes. Resolved package versions are
  recorded in `.local/holt/results/python-packages.txt`. The portable uv
  0.13.5 download was checked against its GitHub asset digest. Normal
  operation does not depend on Codex's bundled interpreter. The temporary
  `.local/holt/.venv/` was used only for initial preparation/downloads.

The benchmark's streaming parser passed two local HTTP tests, covering
fragmented tool calls, streamed text and usage accounting. These tests do
not measure or validate model behavior.

The official prebuilt runtime avoids installing the full C++ compiler and
Vulkan SDK for this validation. The installed AMD driver supplies Vulkan.

## Driver update checkpoint

AMD's processor-specific support page lists Adrenalin 26.10.1 WHQL,
released October 20, 2026. Its downloaded installer has a valid Advanced
Micro Devices Authenticode signature. The installer is saved at
`.local/holt/drivers/amd-adrenalin-26.10.1.exe`.

The first installation attempt returned 202 because Windows Update needed
a restart. That restart completed, and the second AMD installation returned
0. The active graphics driver is now **32.0.32015.7029**, with Vulkan API
1.4.359 / AMD Adrenalin 26.10.1. Windows Update reports no pending restart.
The user saw a PCI Bus update-completion prompt; both PCI Bus and Radeon
devices currently report healthy (ConfigManagerErrorCode 0). If a Windows
device-install notification still requests another restart, complete it
before treating measurements as final.

The approved Windows restart completed. The one-time restart helper is
saved under `.local/holt/`; no startup task is registered.

Use `scripts/check-holt-hardware.ps1` after restarting to capture the active
driver, Windows Update restart state, Vulkan and model-runtime device views.

Corsair's published downloads list BIOS 3.06, older than the installed 3.07;
no firmware downgrade is indicated. Other drivers have not been declared
current. Check chipset updates after Windows Update and graphics installation.

Sources: [AMD processor downloads](https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-ai-max-series/amd-ryzen-ai-max-plus-395.html),
[AMD 26.10.1 release notes](https://www.amd.com/en/resources/support-articles/release-notes/RN-RAD-WIN-26-10-1.html),
[AMD error 202](https://www.amd.com/en/resources/support-articles/faqs/GPU-KB202.html),
[Corsair downloads](https://www.corsair.com/us/en/p/gaming-computers/cs-9080002-na/corsair-ai-workstation-300).

## Connection and credentials

SKIPPY resolves to 192.168.50.41. TCP 7777 is reachable and unauthenticated
HTTP returns 401 as expected. The user saved the token in this Windows
account's `HoltzmanVoice` / `holtzman-bearer` Credential Manager entry.
Authentication, tool discovery and `status` now succeed.

SKIPPY's SMB share enumeration and the Users/C$ paths deny access to this
PC's current Windows account. The user was given commands to create an
encrypted, read-only `Holtzman` share and a Private-profile firewall rule
for this PC's current Wi-Fi address, 192.168.50.16. File sharing is no longer
needed for HOLT setup: both credentials and prompt discovery work without it.

In a local interactive PowerShell terminal, save only the token value from
SKIPPY using the hidden prompt (never paste it into chat):

```powershell
.\.local\holt\voice-env\Scripts\python.exe scripts\check-holt-mcp.py --save-credential
.\.local\holt\voice-env\Scripts\python.exe scripts\check-holt-mcp.py --schemas .local\holt\results\mcp-tools.json --discovery .local\holt\results\discovery
```

The script discovers tools and calls only `status`. It never executes game
controls and never supplies credentials to the model. The updated branch
now includes screenshots, `press_binding(observe)`, nav-target tools,
`ship_computer` prompt and ten guide resources. The deployed prompt matches
the fetched repository prompt after newline normalization. Prompt updates
made locally afterward require updating/restarting SKIPPY before discovery
returns them. The latest returned Status.json timestamp was several hours
old; the read-only client correctly reports that reading as historical.

## CPU speech component results

whisper.cpp v1.9.5 maps to official build b5454. Its CPU Windows x64 archive
was checked against GitHub's SHA-256 digest. The English small model was
pinned at `ggerganov/whisper.cpp` revision
`5359861c739e955e79d9a303bcbc70fb988958b1` and its LFS SHA-256 verified.

The official 11-second JFK WAV sample was transcribed correctly. A fresh
CLI process took 5.72 seconds wall time (1.74 seconds reported internal
processing). Two persistent CPU server requests took 1.24 and 1.13 seconds.
Start that server with `scripts/start-holt-transcription.ps1`; it listens
only on 127.0.0.1:8081 and uses eight CPU threads.

Kokoro 0.9.4 with CPU PyTorch 2.14.1+cpu and the af_heart test voice generated
a 2.85-second reply in 0.35 and 0.29 seconds, with 7.56 seconds of initial
setup/import/model-loading time. This supports keeping speech models warm.
The Kokoro assets are pinned to revision
`f3ff3571791e39611d31c381e3a41a3af07b4987`; model and voice LFS hashes were
verified. `scripts/check-holt-speech.py` synthesizes entirely from local
files, writes WAVs and timings, and plays audio only with `--play`.

English tokenizer model en_core_web_sm 3.8.0 is installed. Misaki 0.9.4 uses
its bundled espeakng-loader library. The official x64 eSpeak 1.52.0 MSI was
also extracted privately under `.local/holt/runtime/espeak/`; it was not
installed system-wide. No C++ compiler or system-wide eSpeak installation
was necessary for these component tests.

Initial audio enumeration showed only WDM-KS microphone endpoints. After
the user connected USB audio, both Jabra Evolve2 40 SE and Elgato Wave XLR
appeared as normal MME/DirectSound/WASAPI recording devices. Windows selected
the Jabra as the default input; the default output remained Senary headphones.
Confirm the intended microphone/output pair before recording or playback.
No microphone audio was recorded.

These sample results do not establish system-name recognition, background
audio rejection or complete conversation latency.

Voice auditions use `scripts/audition-holt-voices.py --play`: Sky, Fenrir,
George, Emma, and Fenrir with a light comms filter. The added voice assets
were downloaded from the same pinned Kokoro revision and hash-verified.
Individual WAVs and a combined audition are under
`.local/holt/results/voice-audition/`. The user preferred George as the
baseline, then requested a more formal, sarcastic, dry delivery inspired
by Captain Holt's demeanor. Further samples use `--deadpan`: George at
normal and measured pace, George lowered one semitone, and Michael as an
American voice comparison. These WAVs are in `.local/holt/results/george-deadpan/`.
The user then preferred the normal George sample (no pitch shift), with
more dynamic delivery as situations become urgent. `--urgency` auditions
fictional caution, urgent and critical hull warnings, followed by a dry
recovery comment. Samples use speeds 1.03, 1.12, 1.18 and 1.0, respectively;
the user liked this urgency direction. Favor precise, understated phrasing;
keep operational facts clear and suppress jokes during urgent situations.
The prototype loads the live personality prompt and uses George; automatic
alert delivery is not implemented.

## Local language-model results

The language model and F16 vision projector loaded successfully on Vulkan,
with an 8K context and one slot. The server is listening only on loopback
port 8080; no startup service has been registered.

Initial text fixture tests returned valid tool-call schemas in all 24
requests. Strict comparisons flagged case-insensitive Cobalt capitalization
and an excessive recent-events count. The market comparison was corrected
to match the server's documented case-insensitive behavior. The journal
tool's fixture description was clarified: it filters event types before
applying the requested limit. With those changes, 24/24 fixture requests
passed, including ambiguous-destination clarification and advice without
action proposals. This is a small smoke test, not a reliability estimate.

The first uncached simple request took 2.35 seconds total. With the clarified
schemas, median time to first streamed output was 0.306 seconds; tool-call
requests generally finished in 0.37–0.90 seconds and explanatory replies in
about 2.1 seconds. These are local text timings, not end-to-end voice latency.
Raw results are in `.local/holt/results/model-benchmark-*.json`.

The startup script now uses the runtime's supported `--reasoning off` flag
and requests a minimum of 1024 image tokens for future grounding tests,
following the runtime's model-loading guidance. Image interpretation has
not been validated with game screenshots. After authentication, the deployed
schemas passed the same 24-request smoke test. Including the full deployed
prompt exposed a destination-guessing failure; see the prototype notes below.

## Remaining validation order

1. Check whether the PCI Bus notification still requests a restart. The
   graphics installation and initial Windows Update restart are complete.
2. The model is running. After a restart, use `scripts/start-holt-model.ps1`:
   loopback only, GPU offload, 16K context, one slot, thinking disabled,
   matching vision projector.
3. Authentication and discovered-schema smoke tests pass. After SKIPPY's
   prompt update/restart, rediscover and repeat the full-contract tests:

   ```powershell
   .\.local\holt\voice-env\Scripts\python.exe scripts\benchmark-holt.py
   .\.local\holt\voice-env\Scripts\python.exe scripts\benchmark-holt.py --schemas .local\holt\results\mcp-tools.json
   ```

   It records first streamed output and total latency, proposed calls and
   schema validation. It never executes a proposed tool. Read the replies:
   a structural pass is not proof of correct reasoning or authorization.
4. Test several real saved Contacts-panel PNGs with `--image <path>`.
   Visually compare selected station, tab and docking highlight. No real
   cockpit screenshots are present here yet; vision is unvalidated.
5. Integrate local speech recognition and synthesis with push-to-talk.
   Measure end-of-utterance to first useful audible reply. Aim initially
   for a few seconds on routine status requests; this is an acceptance
   target, not a measured result. Compare a smaller model if needed.
6. Enable read-only status/trade conversations, then supervised route
   plotting and screenshot docking. Reconcile server capabilities and add
   cancellation/status/held-input cleanup before advertising a reliable stop.

## Read-only conversation prototype

`scripts/holt-client.py` now discovers the deployed instructions, personality
prompt and tool schemas on connection, then uses the local model to answer
through validated read-only MCP tools. The local allow-list rejects all
game-control and calibration tools regardless of model output. A live
status question completed successfully and identified the stale timestamp.
Use `--speak` for local George speech output; microphone input, proactive
alerts, mode switching and supervised control workflows remain unfinished.

```powershell
.\.local\holt\voice-env\Scripts\python.exe scripts\holt-client.py
.\.local\holt\voice-env\Scripts\python.exe scripts\holt-client.py --speak
```

The full deployed prompt and all schemas used roughly 7,700 prompt tokens,
so the model now runs with a 16K context. The full-prompt smoke test caught
an ambiguous destination being guessed despite the existing prompt rule.
The source prompt now explicitly forbids filling in partial destinations
and asking for permission while acting in the same turn. With the proposed
source prompt, all 27 repeated control/discovery checks passed. Added lore
questions returned the recorded maker, origin and serial and admitted the
broker's name is unknown. SKIPPY still needs to load the new prompt after
updating. Actions remain disabled in the prototype; prompt wording alone
is not an authorization mechanism.

One later benchmark request stalled for about 69 seconds during evaluation
of a small cached prompt tail; subsequent requests returned to normal.
The cause is unresolved. Do not treat median warm timings as a latency
guarantee. Startup, idle-to-active behavior and tail latency need further
measurement on the updated driver.

The client preserves final reply text and, if needed, uses a separate
constrained-output request to select the delivery enum. It does not ask
the formatter to rewrite facts. Eight local policy/discovery/streaming tests
pass; a lightweight Windows CI job runs them. Rust server checks require a
Rust toolchain, which is not installed on this client PC.

The tested Windows Python package snapshot is saved in
`requirements/holt-windows.lock`. Install CPU PyTorch using its official
CPU index first, as in the Windows guide, then restore the remaining locked
dependencies into the isolated environment. Models and credentials are
not included in the lock or source control.

The complete microphone-to-tools-to-speech loop remains to be built.
No game controls, model training, firewall changes or scheduled startup
tasks were needed for this checkpoint.

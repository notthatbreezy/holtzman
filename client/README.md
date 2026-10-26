# HOLT desktop client

On the configured Windows ship-computer PC, double-click **HOLT.cmd** in
the repository root. The panel opens stopped. Choose microphone and output
devices, then click **Start HOLT**. Start loads the language/vision model,
Whisper transcription service, George speech synthesis, and the authenticated
MCP connection to SKIPPY. Services are loopback-only and run as your user.

**Stop HOLT** ends listening, speech, the client connection and this checkout's
local inference processes. Closing the panel also stops them. It does not
stop Elite or the MCP server on SKIPPY. An already-running server-side input
recipe may finish after the client stops; cancellation is not guaranteed.

Command-line equivalents, from the repository root:

```powershell
.\HOLT.cmd          # Open the control panel, initially stopped
.\HOLT.cmd start    # Open the panel and start HOLT
.\HOLT.cmd stop     # Stop HOLT and its local inference services
.\HOLT.cmd status   # Report whether the managed processes are running
```

## Speaking and audio devices

Listening is continuous while the mic is unmuted. Your physical mute button
controls whether speech reaches the client; no push-to-talk is required.
Voice detection waits for about 850 ms of silence before submitting an
utterance. Capture pauses during processing and speech playback, avoiding
self-triggering. Speak one request at a time. There is no voice barge-in;
use Stop to interrupt the client.

Windows default devices are the default. Explicit selections are stored by
device name and audio backend, not a numeric device ID. If a selected device
is missing, the panel reports it rather than quietly switching microphones.
Device changes apply on the next Start. The text box remains an alternative
to microphone input.
The panel shows the opened microphone separately from conversation state.
Disconnected or stopped input streams are retried every three seconds.
Capture pauses while HOLT thinks or speaks; physical mute naturally produces silence.
George's standard delivery runs at 1.15x speed. Ordinary replies are shortened
to one or two sentences; ask for detail to get a longer explanation.
HOLT may intentionally leave acknowledgements/asides unanswered. Such turns
are recorded with `respond:false`, produce no speech or assistant chat bubble,
and do not appear as errors. Command outcomes and important warnings still need reporting.

## Optional hosted conversation

Choose **OpenAI · GPT-5.4 mini** in the conversation-model selector, save a key
with **Set OpenAI key**, then Stop/Start. The key is held in Windows Credential
Manager (`HOLT.OpenAI` / `api-key`), or supplied via `OPENAI_API_KEY`.
The model receives conversation history, selected ship facts/tool results, and
cockpit images only when **Allow game-window images to OpenAI** is enabled.
Whisper transcription, George speech and all game
controls remain local. API usage is billed separately from ChatGPT.

The provider uses OpenAI's Responses API at a fixed HTTPS endpoint, disables response storage
with `store:false`, and never attaches its credential to MCP, Spansh or speech
requests. Conversation uses low reasoning effort; auxiliary formatting uses
none. Local Qwen is not started in hosted mode. Select **Local · Qwen** and
restart to return to local inference; provider errors never trigger silent
fallbacks or replay controls.

Token counts, cache hits and estimated API costs are written to
`.local/holt/results/openai-usage.jsonl`; per-call/session estimates appear in
Service details. GPT-5.4 mini price estimates use the documented rates as of
2026-10-26, including cached input; the provider's bill is authoritative.
Web search is available when the hosted model needs external knowledge, with
up to two built-in search calls per conversational turn. Search fees are counted
at $0.01 per call plus token costs. Spoken answers omit citation markers and
URLs; source links remain clickable in the transcript. Local instruments remain
the authority on live ship state.
`python -m client.evaluate_conversation --openai` runs five paid, offline
conversation tests without game input. Plain `client.evaluate_conversation`
compares local prompt/thinking configurations and incurs no OpenAI usage.

Give direct spoken or typed game commands naturally; **HOLT** is optional:

> HOLT, plot a course to AD Leonis.

The explicit order authorizes that bounded chore without a confirmation
popup. Casual discussion, negation and ambiguous destinations do not grant
permission. Game actions are schema-validated, announced and bounded by a
per-request call/time budget. In-system navigation and docking menu actions
use cockpit observations and the server's verification. Calibration, raw
flight/fire keys, unattended loops and arbitrary desktop control are excluded.
Untick **Explicit game commands** to use HOLT in read-only mode.
An explicit route order with a destination dispatches directly to the server's
`plot_route` recipe. HOLT reports success only from the recipe's verified
NavRoute.json result; reading an existing route is not completion. Mass lock
does not prevent map plotting. Offline dispatch checks are available with
`python -m client.check_route_dispatch` in the configured voice environment.
The local read-only `find_module` tool searches public Spansh outfitting
reports for an exact system and module (optionally class/rating). It requires
large pads, excludes planetary ports and fleet carriers, returns report times,
and never buys or installs anything. Community reports are not live stock.
An immediate order such as **HOLT, request docking now** runs a bounded menu
chore using cockpit observations. Docking requires being within 7.5 km of the
station. Conditional orders such as “when I am in range, request docking” are
not scheduled; HOLT explicitly reports that limitation and asks for a new
command when ready. Disabled controls are reported before consulting the model.

## Memory and context

- Complete question/reply pairs are committed to SQLite as they finish.
  Reopening resumes the latest session.
- The preferences field is saved separately and included on every request.
- Near the context budget, older turns are summarized while four recent
  turns remain verbatim. Original transcripts remain in the database.
- Prompt, tool schemas, summaries and the pending request are counted using
  llama.cpp's template/token APIs, with a conservative fallback. Large tool
  results are bounded and old images are removed from model context.
- Current game facts are fetched from MCP, not inferred from old memory.
  Before each main conversation-model call, the client fetches `status`
  automatically and supplies a separate current-instruments context. The model
  need not request basic ship facts. Failed reads explicitly mark them unavailable.
  Fuel is labelled in tonnes and docked/landed/mass-lock flags are separate.
  Elapsed ages are computed in code. A status timestamp is the last file
  change, not a heartbeat. Clock disagreement is reported and blocks controls.
- **New conversation** starts a fresh session without deleting old sessions
  or saved preferences.

Data stays under `.local/holt/`: `settings.json`, `memory.sqlite3`, runtime
PID records in `run/`, and diagnostic logs in `results/`. These paths are
git-ignored. Microphone audio is sent only to the local transcription server
and is not saved. Transcriptions and replies are saved in conversation history.
Credentials remain in Windows Credential Manager, never in model context.
`results/client-tools.jsonl` records each request's permit, model tool proposals,
client rejections, tool executions and returned text. Corresponding cockpit
observations are saved locally by content hash for diagnosis. These contain
conversation/game data; no HTTP headers or credentials are recorded.

The client fetches `ship_computer`, instructions, tools and guide resources
from the live MCP server on every connection/reconnect. It does not require
a copied personality file. Basic read-only hull/heat/low-fuel watches notify
only on threshold transitions with current telemetry, not on a timer.
The two-second instrument watch also stores a six-hour rolling sample history
in SQLite. Each conversational turn refreshes selected journal transitions;
seven days of those events survive restarts. Prompt context includes the last
eight jumps, continuous observed system/state durations, matching journal arrival
times, and recent net main-tank fuel change. A rate needs at least a minute of
continuous samples and is omitted across jumps, refuelling, stale readings or
gaps. It is labelled a net tank change, not an endurance or engine-burn estimate.

## Setup and verification

The configured PC already has the verified runtime/model files and portable
Python environment. See [the Windows checkpoint](../docs/windows-hardware-validation.md)
and [Windows component guide](../docs/windows-client-install.md) for paths
and initial provisioning. CPU PyTorch must be installed from its CPU index;
the remaining tested dependencies are in `requirements/holt-windows.lock`.

Run `scripts/save-holt-credential.ps1` once to enter the Holtzman token in a
hidden local prompt. Never put the token in chat, command arguments or Git.

```powershell
.\.local\holt\voice-env\Scripts\python.exe -m unittest discover -s tests -p 'test_holt_*.py' -v
.\.local\holt\voice-env\Scripts\python.exe -m client.check
```

`client.check` tests status, game-window vision and real model compaction in
a temporary session. It never opens the microphone or executes game controls.
The older `scripts/holt-client.py` remains a read-only terminal prototype.

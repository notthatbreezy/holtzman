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

Spoken game-control commands must begin with **HOLT**, for example:

> HOLT, plot a course to AD Leonis.

The explicit order authorizes that bounded chore without a confirmation
popup. Casual discussion, negation and ambiguous destinations do not grant
permission. Game actions are schema-validated, announced and bounded by a
per-request call/time budget. In-system navigation and docking menu actions
use cockpit observations and the server's verification. Calibration, raw
flight/fire keys, unattended loops and arbitrary desktop control are excluded.
Untick **Explicit game commands** to use HOLT in read-only mode.
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

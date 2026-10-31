# HOLT fast commands, push-to-talk and event stream

Oct 27, 2026 · Chris · exported from the Claude doc "HOLT fast commands, push-to-talk and event stream"

## Goals and scope

The target: "HOLT, heat sink" fires as soon as its words are transcribed, is verified, and is acknowledged in HOLT's voice. Transcription is the floor: today's CPU Whisper takes about 1.2 s per request, so no latency target is set until Phase 4 measures one. Open conversation keeps its current quality with less dead air.

In scope:

- A `ship_action` server tool for ship systems (heat sink, hardpoints, gear, scoop, lights, silent running), with Status.json verification.
- Configurable push-to-talk from a button of the Captain's choosing, or off, plus adaptive end-of-speech detection when PTT is not held.
- A client fast path that executes simple commands without the conversation model.
- A server-side event stream that replaces the two-second `status` poll and carries PTT presses.
- Prioritised callouts and interruptible speech.
- Server-side action cancellation and action status.
- A captain's log: journal-derived session facts, dictated entries and voyage reports.

Not in scope: arrival briefings, simpit lights, and wake words. Pipecat is not adopted; the existing `client/audio.py` pipeline is extended.

Fair play stays as the README states it. Every input is one short chore the Captain explicitly asked for. Alerts never trigger game input. Nothing is scheduled or repeated unattended.

## What exists today

Most of the plumbing exists; the gaps are ship-system input, latency and push events. Read at commit `2c9d558` (Oct 26, 2026), then rechecked against the working tree on SKIPPY on Oct 27.

| Area | Today | File |
| --- | --- | --- |
| Bindings | Parses `.binds`, keeps keyboard Primary/Secondary chords; ignores `ToggleOn` (hold mode). `list_bindings` returns exact action names and chords | `src/bindings.rs`, `src/server.rs` |
| Input | SendInput scan codes; one `controls` mutex, `try_lock` refuses overlap; `focus_game` may tap Alt and wait 150 ms | `src/input.rs`, `src/server.rs` |
| Binding access | No allow-list since Oct 27: `press_binding` can press any keyboard-bound action. `pressable` is accepted but ignored. `status` reports `server_build` and the menu bindings | `src/config.rs`, `docs/docking-and-controls.md` |
| Status flags | Decodes `Flags`/`Flags2`; no `HudInAnalysisMode` (bit 27) or `NightVision` (bit 28) | `src/journal.rs` |
| Permissions | `converse` builds `Permit(natural=True)`, which checks only: controls enabled, no failure, at most 8 calls in 120 s. The per-tool checks run only in an unused strict mode | `client/policy.py`, `client/engine.py` |
| Direct dispatch | A route order naming its destination skips the model and calls `plot_route` | `client/engine.py` |
| Control path | Every control call first reads `status`, takes a PNG screenshot, then speaks "Working the cockpit controls, Captain." before pressing; `press_binding` also needs an observation under 15 s old | `Engine.invoke` |
| Playbooks | Revisioned procedural memory shared by MCP clients (`search`/`read`/`save_playbook`), stored beside the config; a draft docking playbook is seeded by `client.seed_playbooks` | `src/playbooks.rs`, `docs/playbooks.md` |
| Context | Hosted GPT-6 Luna uses its real 1.05M-token window with request-local compaction; local models keep their configured size | `client/context.py`, `docs/context-management.md` |
| Transcription | whisper.cpp `small.en` on CPU (`-ng`); 1.13–1.24 s per request, roughly independent of clip length | `start-holt-transcription.ps1`, `windows-hardware-validation.md` |
| Turn loop | Serial: transcribe, model turn, speak. `paused` is set for the whole turn, and the mic discards frames while paused. Streams open inside `audio_session()`, and device refresh refuses while one is open | `Engine.connected`, `client/audio.py` |
| Alerts | Polls `status` every 2 s; overheat and low-fuel flags. The hull watch reads `Loadout.HullHealth`, written at login and refit, so it is stale in combat | `client/telemetry.py`, `src/journal.rs` |
| Topology | Mic on SKIPPY, sent over VBAN to the HOLT PC, ARCHIE (192.168.50.16), which runs Whisper, the model and Kokoro. `client/ship_link.py` pins `skippy` to IPv4 | `docs/skippy-audio-handoff.md` |

Three facts shape the design. The cockpit buttons are wired to SKIPPY while listening happens on the HOLT PC, so a PTT press must cross the LAN. The client's permission model is looser than its regexes suggest, so new controls need their own hard gate. And the SillyTavern gateway shares `Engine` and `client/audio.py` but runs its own turn loop (`TavernEngine.connected` hands transcripts to SillyTavern), so loop-level changes need a second hook there.

```
SKIPPY · holtzman server, beside Elite
  Event watcher (journal, Status.json, 100 ms) | PTT reader (mouse, key, joystick)
  Ship actions (flag-checked, verified, cancellable) | Log and plan (sessions, entries, objectives, legs)
        │ events + PTT edges (SSE)                      ▲ MCP: actions, log, plan
        ▼                                               │
ARCHIE (HOLT PC) · voice client, mic via VBAN
  Mic + Segmenter (PTT or adaptive endpoint, Whisper) → Fast path (phrase table → ship_action, ack)
  → Engine + model (everything else: briefings, plans) → Speech queue (priority, expiry, barge-in, Kokoro)
```

The fast path is the shortcut: a matched command goes straight to `ship_action` on SKIPPY, and anything that isn't purely a command goes to the model instead.

## Phase 1: `ship_action` tool

Add one server tool that presses a named ship system, refuses unsafe states, and reports what Status.json shows afterwards. Since `press_binding` lost its allow-list, it can already press `DeployHeatSink`; `ship_action` adds what it lacks: state checks, toggles that never flip the wrong way, verification, and no screenshot round trip. The same preconditions also apply inside `press_binding` whenever its binding is in the action table, so a model-chosen `LandingGearToggle` while landed is refused there too. That is a state check, not a return of the allow-list.

### Action table

Defaults live in a new `src/actions.rs`; `holtzman.toml` can override hold times, preconditions and enable/disable each action under `[ship_actions.<name>]`.

| Action | Binding | Kind | Verified by |
| --- | --- | --- | --- |
| `hardpoints` | `DeployHardpointToggle` | toggle | `HardpointsDeployed` |
| `landing_gear` | `LandingGearToggle` | toggle | `LandingGearDown` |
| `cargo_scoop` | `ToggleCargoScoop` | toggle | `CargoScoopDeployed` |
| `lights` | `ShipSpotLightToggle` | toggle | `LightsOn` |
| `silent_running` | `ToggleButtonUpInput` | toggle | `SilentRunning` |
| `night_vision` | `NightVisionToggle` | toggle | `NightVision` (bit 28; add to `FLAGS`) |
| `flight_assist` | `ToggleFlightAssist` | toggle | `FlightAssistOff` (inverted) |
| `heat_sink` | `DeployHeatSink` | one-shot | none: reported as pressed |
| `chaff` | `FireChaffLauncher` | one-shot | none |
| `shield_cell` | `UseShieldCell` | one-shot | none |

Binding names should be checked against a current `.binds` file before merging. ECM is left out because it needs a timed hold; add it once the hold length is calibrated.

### Call shape

```json
{"action": "hardpoints", "state": "on"}     // toggles: "on" | "off" | "toggle"
{"action": "heat_sink"}                    // one-shots: no state
```

Result, always structured so the client can speak it without a model:

```json
{"action": "hardpoints", "outcome": "done", "pressed": true,
 "before": false, "after": true, "elapsed_ms": 410}
```

`outcome` is one of `done`, `already` (no key pressed), `pressed_unverified`, `not_confirmed`, `refused`, `busy`, `unbound`.

### Execution

1. Look up the action; refuse if disabled or the binding has no keyboard chord (`unbound`, naming the binding to add as Secondary). Refuse a toggle whose binding is in hold mode (`<ToggleOn Value="0"/>` in `.binds`, which `bindings.rs` must start parsing), since a tap would never latch.
2. Read Status.json and refuse when:
   - not `InMainShip`, `Docked`, or `Landed` (lights and night vision excepted): retracting gear while landed lifts off, which counts as launching;
   - `GuiFocus` is not 0, because keys pressed with a panel open mean something else;
   - `FsdCharging` or `FsdJump` is set, so a press cannot interfere with a jump;
   - the action's own supercruise rule fails. Defaults: gear and cargo scoop refused in supercruise; hardpoints in supercruise only with `HudInAnalysisMode` (bit 27, added to `FLAGS`). Calibrate these in game.
3. For a toggle, compare the flag with the requested state. If it already matches, return `already` without pressing.
4. Refuse if the Captain is physically holding Shift, Ctrl or Alt (`GetAsyncKeyState`), since a held modifier would turn the injected key into a different binding.
5. Take the `controls` mutex with `try_lock`. If a recipe holds it, return `busy`.
6. `focus_game()`, then `press()` with the action's hold (default `tap_ms`), then release the mutex.
7. For a toggle, poll Status.json every 50 ms for up to 1,500 ms, outside the mutex, until the flag flips; otherwise `not_confirmed`. One-shots return `pressed_unverified` at once. The fast path calls actions one at a time, so within one utterance it runs one-shots first: "hardpoints out and heat sink" fires the heat sink before the hardpoints check starts.

The tool carries no model-facing ambiguity: there is no free-text argument. Annotate it as non-read-only so `Engine.is_control` treats it as a control.

### Bindings coverage

`list_bindings` and `holtzman bindings` mark every enabled ship action whose binding has no keyboard chord as `MISSING`. HOTAS-only bindings are invisible to SendInput, so each needs a keyboard Secondary.

### Client permission

The live client runs `Permit` in natural mode, which never consults `CONTROL` or checks arguments. `ship_action` therefore stays out of the model's `tools` list in `Engine.converse` and the SillyTavern path; the fast path (Phase 4) is its only caller. Any utterance the grammar fully matches is executed before the model sees it, so the model never needs it. The model can still reach the same bindings through `press_binding`, as you chose; that path is slower (observation and announcement first) and now shares the preconditions above. If the Captain asks for something conditional, such as "drop a heat sink when we're hot", HOLT says it can't wait on a condition and asks for the command when it's needed. Commentary and callout model calls pass `tools=[]`.

## Phase 2: Configurable push-to-talk

Push-to-talk is a setting, not a requirement: off keeps today's hands-free listening, and on uses whatever button the Captain picks, such as mouse button 5.

### Modes

| `ptt_mode` | Utterance starts | Utterance ends | Use |
| --- | --- | --- | --- |
| `off` (default) | Voice detected | Silence rule (fixed 850 ms, or Phase 3 adaptive) | Today's behaviour, unchanged |
| `hold` | Button down | Button up + 150 ms tail | Combat, noisy rooms, game audio on speakers |
| `hybrid` | Voice or button | Button up if held, else silence rule | Hands-free chat plus instant commands |

In `hold` mode, speech with the button up is discarded before transcription, which also fixes background video and game dialogue triggering HOLT. The 150 ms tail covers the VBAN jitter buffer, so the last syllable is not clipped; make it a setting and measure it.

### Choosing the button

The button lives in `holtzman.toml` on SKIPPY, as an ordered list of inputs. Holtzman uses the first one whose device is plugged in, so a HOTAS button can come first with the mouse as the fallback when the HOTAS isn't connected:

```toml
[ptt]
inputs = ["Joy:T16000M:5", "Mouse5"]   # first connected device wins
tail_ms = 150
max_hold_s = 30
```

Holtzman re-reads `[ptt]` when the file changes, so editing it takes effect without a restart or a code change; the panel shows which input is active. If the edited file fails to parse, holtzman keeps the previous `[ptt]` and the panel shows the error. `tail_ms` and `max_hold_s` reach the client in each `snapshot` event, since the client's `Segmenter` applies them.

`Config` uses `deny_unknown_fields`, so the new `[ptt]` and `[ship_actions]` sections are `#[serde(default)]`. An older binary, including the `.bak` that `update-holtzman-server.ps1` restores after a failed start, will refuse them; rolling back means removing those sections. Input names:

- `Mouse5` / `Mouse4` (XButton2 / XButton1), `MouseMiddle`
- A keyboard key in Elite's names, e.g. `Key_F13`
- A joystick button: `Joy:<device name>:<button>`, e.g. a BBI-32 button box or a HOTAS button

A **Set push-to-talk button** control in the HOLT panel is the alternative to editing by hand. Learn mode accepts one press within 10 seconds, returns only the input's name, and patches `inputs` in place with `toml_edit`, so the file's comments and formatting survive. (The existing `Config::save` rewrites the whole file; `record_click_target` should move to `toml_edit` too.) It rejects letters, digits, Enter and Space, and the modifiers Shift, Ctrl, Alt and Win, which would clash with the modifier check in Phase 1 and with `focus_game`'s Alt tap. The panel's `ptt_mode` setting (off, hold, hybrid) stays on the HOLT PC.

Pick an input Elite does not use. Holtzman observes inputs without consuming them, so the game also receives the press.

### Where the button is read

The mic and buttons are on SKIPPY, so holtzman reads the button there and sends edges to the client over the Phase 5 event stream as `{"type": "ptt", "down": true, "t": ...}`.

- **Capture:** Raw Input (`RegisterRawInputDevices` with `RIDEV_INPUTSINK`) on a hidden message-only window, which receives input while Elite has focus. It covers mouse buttons, keys and HID button boxes such as the BBI-32. Normally register only the class of the active input; learn mode registers all classes for its 10 seconds. `RIDEV_DEVNOTIFY` delivers device arrival and removal, so the first-connected-device fallback updates when a HOTAS is plugged in or pulled. DirectInput8 in background, non-exclusive mode is the fallback for joysticks Raw Input reads poorly.
- Not low-level hooks, which Windows silently removes if a callback is slow, losing key-up events and leaving PTT stuck down. Not `Windows.Gaming.Input`, whose controller readings may be limited to the focused app, and holtzman runs hidden.
- **Device identity:** what `Joy:<name>` matches (the HID product string, or the device name in `.binds`) is settled in spike 1.
- **Configuration stays off MCP.** `[ptt]` in `holtzman.toml` names the input; an authenticated `POST /ptt` endpoint sets it or runs learn mode. It shares the bearer token every MCP client holds, so it is not exclusive to HOLT. The learn-mode limits above are what keep it from becoming a keystroke logger.
- **Stuck-button guards:** the current PTT state rides in every `snapshot` event, so a reconnect resynchronises it, and a hold longer than `[ptt] max_hold_s` (default 30) ends the utterance.
- Off Windows, a test double emits scripted edges, like the existing keypress `Recorder`.
- Validate capture with the actual mouse, BBI-32 and HOTAS while Elite is focused, before committing to one API.

### Client changes in `client/audio.py`

- **A pipeline, not a loop.** Today `Engine.connected` handles one input at a time, `Microphone.capture` pauses itself after every utterance, and the engine keeps `paused` set for the whole turn. Split it into three tasks joined by queues: capture and segmenting, transcription, and model turns. The fast path (Phase 4) dispatches from the transcription task, so a command is heard and executed while a model turn is still running.
- **Pause semantics, per mode.** Remove the capture self-pause and the turn-long `paused.set()`. In `off` and `hybrid`, pause only while `speaker.say` is playing, so HOLT doesn't hear itself. In `hold`, never pause; the button gates input.
- **Trace ids:** each utterance gets its own trace id, instead of sharing `Engine.request_id` with whatever model turn is in flight.
- `Segmenter.feed` takes a `ptt_down` flag per frame. In `hold`, a segment starts at button-down regardless of VAD, includes the 10-frame pre-roll, and ends at release plus `tail_ms`. Drop the 10-voiced-frame minimum to 3 when held, so a clipped "chaff" or "stop" survives.
- In `hybrid`, a press during a voice-started segment converts it to a held one, which then ends on release.
- A press while HOLT is speaking stops speech (`speaker.interrupt`, `sd.stop()`), so PTT doubles as barge-in.
- **SillyTavern:** `TavernEngine.connected` is also a serial loop that keeps `paused` set while busy; give it the same three-task split. Its fast-path hook runs before `report("transcript")`. A handled command emits a new `fastpath` event, never `transcript`, and never sets `awaiting_voice`, so it cannot start a SillyTavern turn or leave the mic paused. `index.js` gets a display branch that shows it as a command line in the chat.
- The panel shows `PTT: Mouse5 (held)` beside the mic state. If the event stream drops in `hold`, HOLT says so and stops listening rather than falling back to an open mic.
- Open the new capture and playback streams inside the existing `audio_session()`, so `refresh_devices()` keeps refusing while any stream is open.

## Phase 3: Adaptive endpointing

When PTT is not ending the utterance, a turn-detection model decides whether the Captain has finished: about 200 ms after a finished sentence, up to 1.5 s after a trailing "uh". The fixed 850 ms rule stays available as a setting.

### Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `endpoint_mode` | `fixed` until validated, then `adaptive` | `fixed` uses `silence_ms`; `adaptive` uses the model |
| `silence_ms` | 850 | Fixed-mode silence, unchanged |
| `min_silence_ms` | 200 | Silence before the first model check |
| `recheck_ms` | 100 | Further silence between checks |
| `max_silence_ms` | 1500 | Always end after this much silence |
| `turn_threshold` | 0.5 | Completion probability that ends the turn |

### Model

Use Pipecat's open-weights Smart Turn model on its own, through ONNX Runtime on the CPU; the Pipecat framework is not needed. It reads the last few seconds of 16 kHz audio and returns a completion probability. Pin the model file and its checksum the way `download-holt-model.py` pins the language model, and benchmark inference time on the HOLT PC before relying on it.

### Changes to `Segmenter`

1. Keep `webrtcvad` for speech detection and the pre-roll exactly as now.
2. Track trailing silence in milliseconds rather than frames.
3. At `min_silence_ms`, hand the segment to a turn worker thread, so the mic loop never blocks.
4. If the worker returns `complete`, end the segment. If not, check again after each `recheck_ms` of further silence. Speech resuming cancels the pending check.
5. At `max_silence_ms`, end regardless.
6. If the model fails to load, or three checks in a row take over 150 ms, fall back to `fixed` for five minutes before retrying, and the panel shows `Endpointing: fixed (model unavailable)`.

`hybrid` PTT uses the same path whenever the button is not held.

### Measuring it

Add `client/check_endpointing.py`, which replays recorded WAV utterances through both modes and reports end delay and cut-offs. A cut-off is a segment ending before the Captain finished, judged by comparing Whisper text of the cut segment with the full recording. Include slow destinations ("plot a course to… AD Leonis") and short commands ("heat sink"). Switch the default to `adaptive` only when it has no more cut-offs than `fixed` on that set.

## Phase 4: Fast-path command router

An utterance that is nothing but ship commands is matched by an anchored grammar in code and executed through `ship_action`, with a canned acknowledgement. Anything else, even one extra word, goes to the model untouched. Heat sinks and shield cells are consumable and can't be undone, so the matcher is built to miss rather than misfire.

### Where it runs

A new `client/fastpath.py`, called from the transcription task of the Phase 2 pipeline, so it runs even while a model turn is in progress. It calls `session.call_tool("ship_action", ...)` directly instead of going through `Engine.invoke`. That skips the `status` read, the full screenshot and the spoken "Working the cockpit controls" announcement, which would add seconds; the server already checks state. It still honours **Explicit game commands** and writes `trace` entries. The SillyTavern hook from Phase 2 calls the same function.

### Tier 1: anchored grammar

```
utterance := [address] command { ("and" | "then") command } [polite]
address   := "holt" | "hey holt" | "okay holt"
polite    := "please" | "now" | "captain" | "thanks"
command   := action-phrase [state-word]      (state word required for toggles)
```

The whole normalised utterance must parse; there is no remainder. Phrases per action:

| Action | Phrases | On words | Off words |
| --- | --- | --- | --- |
| `heat_sink` | heat sink, heatsink, drop/pop/fire a heat sink | – | – |
| `hardpoints` | hardpoints, weapons | out, deploy | in, retract, stow |
| `landing_gear` | gear, landing gear | down, deploy | up, retract |
| `cargo_scoop` | scoop, cargo scoop | out, open, deploy | in, close, retract |
| `silent_running` | silent running, go dark (on only) | on | off |
| `lights` | lights | on | off |
| `cancel` | stop, cancel, belay that (Milestone 2, with Phase 7; until then HOLT answers "Nothing to cancel yet.") | – | – |
| `flight_assist` | flight assist, FA | on | off |
| `chaff` | chaff, fire chaff, launch chaff | – | – |
| `shield_cell` | shield cell, use a shield cell | – | – |
| `night_vision` | night vision | on | off |

Matching rules:

1. Normalise with `policy.normalized`. Compare each phrase token with `rapidfuzz` `fuzz.ratio` of at least 85, token by token. Do not use `token_ratio`: it scores any utterance that contains the phrase's words at 100, so "did you drop a heat sink" would fire.
2. State words match exactly, with no fuzzy matching, so "in" and "out" can never be confused.
3. Known transcription garbles, such as "heat think" and "hard points", go in an explicit alias list grown from logs, rather than loosening the threshold.
4. Reject outright if the transcript ends in "?", or starts with a question or hearsay word: should, did, what, what's, is, are, have, has, can, could, would, why, how, does, they said. Negations (don't, do not, never) reject too.
5. The `address` ("HOLT") is always optional; PTT is the main guard against stray audio. Without PTT held, Elite's own cockpit voice (COVAS) is still unlikely to fire anything: its lines use forms like "Hardpoints deploying" or "Heat sink launched", which the state words and the no-extra-words rule reject. Other voices are a different matter: a wingmate on Discord shouting "chaff!" or "heat sink!" is a clean bare command. So a client setting, `open_mic_fastpath`, governs what fires with the button up. Its default, `toggles_only`, lets reversible toggles fire without PTT, while one-shots (heat sink, chaff, shield cell) need the button held, or the optional address. `all` removes that limit.
6. One-shot actions also need a confident transcript. Request Whisper's `verbose_json` and reject if `no_speech_prob` is above 0.5 or `avg_logprob` below -0.8; tune both on recorded audio.

The client already sends Whisper a prompt of system names. Add the action vocabulary to it only while PTT is held. On near-silence Whisper tends to repeat prompt words, so an always-on "heat sink" in the prompt invites phantom commands.

### Tier 2: router model (optional)

Deferred: add it only if logs show the grammar missing real commands. A small local model served by the existing `llama-server` in its multi-model mode, with a JSON-schema grammar so it can only return:

```json
{"actions": [{"action": "heat_sink"}]}   // or {"actions": []} to send the whole utterance to the model
```

Hard timeout, set from the measured latency budget; on timeout or invalid output, no action and the whole utterance goes to the main model. It stays local even when the main model is hosted.

The one requirement is that router requests never queue behind a main-model turn. Qwen runs with `--parallel 1`, so the router must be a separately loaded model with its own slot, not a second prompt to Qwen. Confirm multi-model mode on the installed build (b11669); use a second process only if it can't guarantee that.

### After execution

- **Instant acknowledgement.** Speak a short line as soon as the outcome is back, picked at random from several in-character variants per action and outcome: "Heat sink away.", "Dumping heat, Captain.", "Hardpoints are already out, Captain.", "Not with a panel open, Captain." The table lives beside the phrase table, so the persona stays editable.
- **The model hears about it.** A receipt (action, outcome, flags before and after, time) goes into conversation history with `memory.append`, so HOLT can refer to it later: "That's our third heat sink this fight, Captain. Two left."
- **Flavour follow-up.** Right after the acknowledgement, the client makes a short model call with the receipt and current instruments, using `tools=[]`, asking for one optional in-character line. It is spoken at `normal` priority with a 6-second expiry, so it plays when there's a gap and is dropped if the moment has passed or an alert or new command takes over. The model may also return nothing.
- **Frequency.** A `fastpath_flavor` setting (`off`, `sometimes`, `always`; default `sometimes`) limits follow-ups to about one per 30 seconds, so a fight doesn't become a monologue. The acknowledgement itself is never delayed by the follow-up.
- The model never presses anything here; this call only talks.

### Safety

- Only actions in the Phase 1 table. Jettison, self-destruct, eject all cargo and similar never enter the phrase table.
- If **Explicit game commands** is off, reply with the existing `unavailable_reply` text.
- One utterance runs each action at most once. A timeout is reported as `not_confirmed`, never retried.

### Latency budget

Stages from PTT release, each logged in `trace`: tail (150 ms), Whisper (about 1.2 s today), matching (about 1 ms), the LAN call plus `focus_game` and `tap_ms`, TTS start. Whisper dominates. Options to cut it, measured by `check_fastpath.py`:

- `--audio-ctx` scaled to clip length, since whisper.cpp otherwise processes a full 30-second window;
- `base.en` or `tiny.en` for held-PTT command clips only;
- the chosen direction: whisper.cpp's Vulkan GPU build with a larger model (large-v3-turbo first), replacing CPU small.en. It shares the iGPU with Qwen, so measure both while a model turn is running.

Set a latency target only once those numbers exist.

## Phase 5: Server-side event stream

Holtzman watches the journal and Status.json itself and streams numbered events, so the client hears of an interdiction or a PTT press within about 100 ms instead of on its next two-second poll or turn. This delivers the README roadmap item "Journal watch → MCP notifications".

**Split for delivery.** Phase 5a ships in Milestone 1: `GET /events` with its Host middleware, carrying only `ptt`, `snapshot` and heartbeats, since that is all push-to-talk needs. Phase 5b ships in Milestone 2 with the callouts: the journal and Status watcher, derived events, the ring buffer, `events_since`, and replacing `Engine.watch`.

### Server: `src/events.rs`

- **Watcher task** started by `serve`. Poll every 100 ms:
  - Status.json: re-read when its mtime changes; diff against the last copy.
  - The newest `Journal.*.log`: read from the saved byte offset, keep any partial last line for the next pass, and switch files when a newer journal appears.
  - Cargo, NavRoute and Market JSON: mtime change only, as a `file_changed` event.
  - Polling is used instead of file-change notifications because it is predictable against how Elite rewrites files, and cheap at this rate.
- **Derived events**:
  - `flag`: one per changed `Flags`/`Flags2` bit, e.g. `{"flag": "HardpointsDeployed", "on": true}`
  - `status`: changed `GuiFocus`, `Destination`, `Fuel`, `Cargo` values
  - `journal`: each new journal line, minus the existing noise filter in `recent_events`
  - `ptt`: Phase 2 button edges
  - `action`: Phase 7 action lifecycle
- **Ring buffer**: the last 2,000 events, each with a monotonic `seq`, server UTC `t`, `source`, `type` and `data`. A `snapshot` event carries the full current status for resync.

**Noise control.** Returning to the main menu or closing the game drops `Flags` to 0, which would emit an "off" edge for every bit and make HOLT announce shields down at every menu. So:

- Suppress `flag` events while `Flags` is 0 or `InMainShip` is off, and emit one `snapshot` when they return.
- Quantise `Fuel` and `Cargo` status events: emit on a 1% change or every 5 s at most, so scooping and supercruise don't flood the ring buffer.

### Transports

1. **`GET /events`** (Server-Sent Events) on the existing axum router, behind `require_token` plus a new Host-header check: `allowed_hosts` is enforced only inside rmcp's config for `/mcp`, so the router needs its own middleware for `/events`. `Last-Event-ID` resumes from a `seq`. A resume older than the buffer gets a `reset` with a fresh `snapshot`. Heartbeat comment every 10 s. This is what the HOLT client uses: one connection, lowest latency, carries PTT.
2. **`events_since(cursor, wait_ms ≤ 25000, types?)`** MCP tool, read-only, long-poll: returns at once if events are waiting, otherwise holds until one arrives or `wait_ms` passes. It serves MCP clients like Claude Desktop that cannot open the SSE stream.

Standard MCP notifications are skipped for now. Generic resource-update notifications add a read round trip, and there is no standard notification for custom events.

### Client

- New `client/events.py`: an `httpx2` streaming reader (the client's existing HTTP library) with reconnect, using `Last-Event-ID`, the same bearer credential as MCP, and ship\_transport from client/ship\_link.py so skippy resolves over IPv4.
- `Engine.watch` stops polling `status` every two seconds. It applies `flag`/`status` events to a live snapshot and runs `Watches` on each change. It still calls `status` every 30 s and after any `reset`, to resync.
- `InstrumentHistory.observe` keeps its two-second sampling, now from the live snapshot instead of a fresh call.
- `journal` events replace the per-turn journal refresh feeding `InstrumentHistory.events`.
- The SillyTavern gateway uses the same `Engine`, so it inherits all of this.
- A `snapshot` or `reset` updates watch state silently. Without that, a reconnect or a return from the menu could announce "Shields are down" just because the snapshot differs from the last known state. Cover it with a test.

## Phase 6: Callouts and speech priority

All speech goes through one priority queue, so an urgent callout interrupts banter, stale lines are dropped, and a PTT press silences HOLT at once. Rule-based callouts grow from three to about ten; model-written commentary is optional and rate-limited.

### Speech queue: `client/speech.py`

- Replaces `Engine.speech_lock` and the interrupt code in `Engine.watch`. `Engine.speak()` becomes an enqueue.
- Keeps the existing levels: `normal` < `caution` < `urgent` < `critical`.
- A new item at `urgent` or above interrupts anything lower that is playing, using `speaker.interrupt` and `sd.stop()`.
- Every item has an expiry: 5 s for alerts, 30 s for replies. Expired items are dropped unspoken but stay in the transcript.
- An identical alert key within its cooldown is merged, not repeated.
- A PTT press interrupts everything. An interrupted alert is not replayed unless its condition still holds after release.
- The mic pauses during playback only in `off` and `hybrid` modes. In `hold`, the button already gates input.

### Rule callouts (no model)

Triggered by Phase 5 events and extending `client/telemetry.py` `Watches`:

| Trigger | Level | Line (editable) | Cooldown |
| --- | --- | --- | --- |
| `BeingInterdicted` on | urgent | "Interdiction, Captain!" | 30 s |
| `HullDamage` journal event (`PlayerPilot`, not `Fighter`) (written at 20% steps): urgent at 40%, critical at 20%, of `Health` | urgent / critical | "Hull at 40 percent." (the actual value) | per band |
| `ShieldsUp` off, in main ship, flags non-zero, shield generator in `Loadout` | urgent | "Shields are down." | 30 s |
| `OverHeating` on (existing) | urgent | as now | 20 s |
| `UnderAttack` journal event | caution | "We're taking fire." | 20 s |
| `LowFuel` on (existing) | caution | as now | once per flag |
| Route outlasts fuel: jumps left on `FuelMain` at the last `FSDJump.FuelUsed` is fewer than jumps remaining, with no scoopable star before then | caution | "Fuel for about three jumps, and none of them scoopable." | once per route |
| No scoopable star in the next three NavRoute systems | caution | "No scoopable stars for the next three jumps." | once per route |
| `DockingGranted` / `DockingDenied` | normal | "Docking granted, pad 12." / the mapped reason | none |
| Mission within an hour of `MissionAccepted.Expiry` | caution | "The Ray Gateway delivery expires in an hour." | once per mission |

Data notes:

- The pad number is `DockingGranted.LandingPad`. `DockingDenied.Reason` is a code (`NoSpace`, `TooLarge`, `Hostile`, `Distance` and others) mapped to a spoken phrase.
- NavRoute gives each system's `StarClass`. Match on its first letter (K, G, B, F, O, A, M), so giants such as `K_OrangeGiant` count as scoopable.
- The fuel estimate divides `FuelMain` by the last jump's `FuelUsed`; HOLT always says "about", since jump cost varies with distance and cargo.
- Retire today's hull watch on `Loadout.HullHealth`, which only updates at login and refit. Engine.urgency\_floor reads the same stale value; switch it to the latest HullDamage too.
- Alerts still never trigger game input.

### Commentary (model)

Off by default, behind a `commentary: off | low | normal` setting. Candidate events: undiscovered bodies from `Scan` (`WasDiscovered: false`), rank promotions, mission completions, arrival in a new system.

- At most one line per 2 minutes (`low`) or per 45 s (`normal`).
- Never while `IsInDanger`, while PTT is held, or while a model turn is running.
- One short model call with the event, the HOLT prompt and `brief_reply`, spoken at `normal` with a 20 s expiry.
- The call passes `tools=[]`, so commentary can never reach a control.

## Phase 7: Cancellation and action status

Every control call becomes a tracked action that can be cancelled between steps, always releases held keys, and reports its state on the event stream; "HOLT, stop" then means something. This closes the gap `docs/local-voice-architecture.md` already names.

### Server

- `src/actions.rs` gains an `ActionRegistry`: one running action at a time (the `controls` mutex already ensures that), with `id`, `tool`, `started`, `step`, and `state` = `running | done | failed | cancelled`.
- Each action gets a `tokio_util::sync::CancellationToken`, checked in more places than `run_steps`, because the main recipes don't use it:
  - `Pilot::run_step` itself, covering `run_steps`;
  - `Pilot::plot_route`'s own step loop and its NavRoute verify loop (`automation.rs` \~L125–175);
  - Nav's `move_cursor`, `settle` and `verify` loops (`nav.rs` \~L207–222, 322–351);
  - every `wait`, `wait_for_gui` and the `ship_action` verify loop, through `tokio::select!` on the token.
- A `HeldInputs` wrapper around `KeySender` records which keys and mouse buttons are down. On cancel, error or drop, it sends `key_up` and `left_button(false)` for each, so a cancelled recipe never leaves a key stuck.
- New tools, neither of which takes the `controls` mutex:
  - `cancel_action()`: cancels the running action through the registry, waits up to 500 ms for it to stop, and reports where, e.g. `cancelled at step 4 of 7 (galaxy map open)`.
  - `action_status()`: read-only, returns the running or last action.
- `action` events on the Phase 5 stream: `started`, `step`, `finished` with outcome.

### No automatic preemption

A `ship_action` during a running recipe returns `busy` rather than cancelling it. A recipe usually has a panel open, such as the galaxy map, and Phase 1 refuses ship actions with a panel open anyway, so preempting would not get the heat sink out faster. HOLT says "Busy plotting, Captain. Say stop to cancel."

### Client

- "Stop", "cancel" and "belay that" are in the Phase 4 grammar and call `cancel_action` directly, like other fast-path commands, with the spoken report taken from the result.
- In `client/policy.py`, `cancel_action` goes in a new always-allowed set, so it works even with controls off, after a failure, or past the 8-call limit; `action_status` goes in `READ`. Neither passes through `invoke`'s screenshot and announcement.
- **Stop HOLT** in the panel calls `cancel_action` before shutting the client down. That fixes the caveat in `client/README.md` and the SillyTavern README that a running recipe may finish after Stop; update both texts.
- After a cancel, HOLT reports state from Status.json, e.g. "Stopped. The galaxy map is still open." Nothing is replayed automatically.

## Phase 8: Captain's log

Holtzman serves a captain's log built from three layers kept apart: facts computed from the journals, entries the Captain dictates, and HOLT's written narrative. Any MCP client can ask "what did we do last night?" and get a grounded answer, and HOLT can file a voyage report at the end of a session.

### Layers

| Layer | Written by | Stored | Can be wrong? |
| --- | --- | --- | --- |
| Session facts | Code, from journals | Recomputed; cached | Only if the journals are |
| Captain's entries | The Captain, verbatim | `captains_log/entries.jsonl` | No: quoted as said |
| HOLT narrative | The model, from the facts | `captains_log/narratives.jsonl` | Yes: labelled as HOLT's account |

The store lives beside the config, like `playbooks_dir`, with a `captains_log_dir` override. Facts are never stored as prose, so a narrative can never overwrite what the journals say.

**Playbooks and the log don't overlap.** Playbooks (already shipped) are procedural memory: how to do something, revisioned, edited by agents. The captain's log is episodic: what happened, with facts from the journals and entries kept verbatim. The log follows the playbook conventions where they fit: storage beside the config, a config override, JSON on disk, no new service. A lesson that comes up in a log entry becomes a playbook only when the Captain asks.

### Session facts: `src/captains_log.rs`

- A session runs from `LoadGame` to `Shutdown`, the next `LoadGame`, or a 30-minute journal gap. It scans historical journals the way `src/fleet.rs` already does.
- Per session: start and end time, ship(s), jumps and light years (`FSDJump.JumpDist`), systems visited, stations docked at.
- Credits: the start balance from `LoadGame.Credits`; the end balance from the next `LoadGame` when there is one, otherwise the start plus the session's summed credit deltas. No event reports credits at shutdown.
- Earnings: `MarketSell`, `MissionCompleted` rewards, `RedeemVoucher`, `SellExplorationData`, `MultiSellExplorationData`.
- Spending: `MarketBuy`, `ModuleBuy`, `ShipyardBuy`, `Repair`, `RepairAll`, `RefuelAll`, `RefuelPartial`, `BuyAmmo`, `BuyDrones`, `PayFines`, `PayBounties`.
- Discoveries: `Scan` with `WasDiscovered: false`, and mappings from `SAAScanComplete` where `Scan.WasMapped` was false.
- **Avoidable incidents**, for HOLT's ambition: `Died`, `HeatDamage`, `Interdicted` (submitted) versus `EscapeInterdiction` (escaped, counted as a save), `DockingCancelled`/`DockingTimeout`, and `HullDamage` at 20% or lower. Each entry has a time and system.

### Tools

- `captains_log(range, include?)`, read-only. `range`: `last`, `today`, a date, a date range, or a session id. `include`: any of `facts`, `entries`, `narrative`; default all. Long ranges return per-session summaries, not every event.
- `voyage_report(session?)`, read-only: one session's facts in the report shape (summary, earnings, discoveries, avoidable incidents), for HOLT to speak or write up.
- `log_entry(text, author: "captain" | "holt")`. It stamps the time and attaches the current system, station and ship from `JournalDir::summary()`; Status.json has none of them. It is a knowledge write: add it to `KNOWLEDGE_WRITE` in `client/policy.py` so it needs no control permit.

### Voice

- "Captain's log…" or "Log entry…" at the start of an utterance is a fast-path rule: the rest is saved verbatim with `log_entry`, without the model, and HOLT answers "Logged."
- Dictation passes the Phase 4 confidence gate and the existing `non_speech` filter, so Whisper hallucinations and TV or Discord speech are not logged.
- "Scratch that" within a minute deletes the last entry.
- On a `Shutdown` event, or when the Captain says "end of watch", HOLT offers a two-sentence spoken voyage report and writes a longer narrative with `log_entry(author: "holt")`. It mentions the avoidable-incident count every time, in character.
- Questions about past sessions go to the model, which calls `captains_log`; the persona prompt says the log is the source for past play, not conversation memory.

## Phase 9: Copilot planning

The copilot feeling comes from HOLT knowing what the Captain is working toward and keeping track of the current job. That needs three server-side pieces: standing objectives, a session-start briefing, and an active plan HOLT follows leg by leg.

### Objectives

Long-running goals in the Captain's words, stored in the captain's log store: "save 500M for a Python", "FSD to grade 5", "Elite in exploration".

- `objectives` tool: `list`, `set(text, measure?)`, `close(id)`. `set` is a knowledge write (add objectives to KNOWLEDGE\_WRITE), like `log_entry`.
- A `measure` ties a goal to journal data where one exists: credits (Status.json `Balance`), rank progress (`Rank`, `Progress`, `Promotion`), engineer unlocks (`EngineerProgress`), materials held (`Materials`, `MaterialCollected`, `EngineerCraft`). Goals without a measure are tracked by the Captain's say-so.
- Engineering goals need blueprint costs, which the journals do not contain. Start with a hand-maintained table for the blueprints the Captain cares about, alongside `guides/engineering.md`.

### Session briefing

On `LoadGame`, or when asked "what's the plan?", HOLT gives a 20-second briefing built in code from read-only data:

1. Ship, location, credits less the rebuy reserve, cargo.
2. Active missions from the `Missions` login event and `MissionAccepted` details, soonest expiry first.
3. Objective progress since the last session, from the captain's log.
4. Two or three options with payoff and data age, e.g. a `plan_trade_route` run from here, finishing a mission, or a step toward an objective.

The Captain picks one by voice; HOLT never picks for him.

### Active plan

The client already keeps a verified itinerary for 30 minutes. Move that to the server as a persistent plan:

- `plan` tool: `set`, `get`, `skip_leg`, `clear`. It is a knowledge write (`KNOWLEDGE_WRITE`) and never grants control.
- `set` takes a reference, not free-form legs: the id of a `plan_trade_route` result the Captain accepted by voice in this session, or an active mission id. Legs are derived from that source, so neither the model nor Spansh text can invent a destination. A leg is `{kind: travel | buy | sell | deliver, system, station?, commodity?, tonnes?}`.
- Legs complete from events, not from HOLT's say-so: `Docked` at the leg's station, then `MarketBuy` or `MarketSell` of the leg's commodity, or `MissionCompleted`.
- On each `Docked` and `Undocked`, HOLT names the next leg: "Next: sell 256 tonnes of gold at Ray Gateway, 14 light years."
- **"Plot the next leg" is a two-turn confirm.** HOLT says "Next leg is Ray Gateway in LP 546-48. Plot it?" It plots only if the Captain's next utterance, within 15 seconds, is an affirmative ("yes", "plot it", "do it"). This is a conversational flow, not a safety guarantee: plot\_route stays ungated by choice, so the model could still plot a leg's destination directly.
- Plans expire after seven days unused and are summarised into the captain's log when finished.

### Later, once the above works

Two cockpit touches are worth a follow-up: HOLT's state (listening, thinking, speaking, alert) sent over UDP to the existing ESP32 annunciator as an amber lamp, and the current plan and objectives on a read-only page sized for the 14-inch MFD screen. Both are out of scope for this plan, like other simpit lights. Mission-expiry callouts moved into the Phase 6 table.

## Prompt and persona updates

Three texts need changes, and two current rules conflict with ship actions as written. Update them in the same change as the code each one describes.

| Text | Where | Reaches |
| --- | --- | --- |
| HOLT persona and rules of engagement | `prompts/ship-computer.md` (or `ship_prompt` override) | Both clients; `setup-holt-tavern.py` builds the SillyTavern card and lorebook from it, so re-run setup after editing |
| Server instructions | `get_info` `with_instructions` in `src/server.rs` | Every MCP client, including Claude Desktop |
| Client contract | `RULES` in `client/engine.py` | HOLT model turns only |

### Rules of engagement (`ship-computer.md`)

- **Rule 5, "Stay on task".** Your Oct 27 rewrite already lets HOLT use configured controls for the current request, so no new exception is needed. Add one line: plain ship-system commands are handled by HOLT's command reflexes (the fast path), and HOLT never waits on a condition ("when we're hot") or acts on an alert by itself.
- **Rule 3, "one action, then check".** When a ship action reports "not confirmed", HOLT says so honestly rather than suggesting a retry.
- **New: cancellation.** On "stop" or "cancel", call `cancel_action` and report where it stopped from Status.json.
- **New: plans.** HOLT offers to plot the next leg and waits for a yes.
- **New: memory kinds.** Playbooks say how to do things; the captain's log says what happened. Look up past play in the log, procedures in playbooks.

### New guidance

- **Senses and limbs:** add `cancel_action`, `action_status`, `captains_log`, `voyage_report`, `log_entry`, `objectives` and `plan`, each in one line like the existing entries.
- **Truth discipline:** past play comes from `captains_log`, not conversation memory. HOLT's own narratives are its account; journal facts win when they disagree.
- **Plans:** a leg is done only when the log says so. HOLT names the next leg on docking and undocking, and repeats the destination before plotting it.
- **Callouts:** alert lines stay under eight words. Commentary is one sentence, never during danger.
- **Voyage report:** a short example in HOLT's voice, ending on the avoidable-incidents count, which gives his ambition a running joke grounded in real data.

### Client contract (`RULES`)

- Fast-path turns appear in history as ordinary turns. Never repeat an action they performed. Commenting on it in character is welcome; call `action_status` if unsure.
- `ship_action` is not among your tools by design; never describe it as missing or broken.

### Server instructions

Other MCP clients, such as Claude Desktop, still see `ship_action` on the server. Rewrite the ACTING paragraph in `get_info`'s instructions (`src/server.rs` \~L923) to say: call `ship_action` only for an action named in the Captain's current message, never conditionally or on your own initiative. Add one sentence each for `cancel_action`, `events_since`, and the log and plan tools. Also update the README's "Fair play" sentence, since ship systems are not UI chores; the rule it states (short, explicitly ordered, supervised) still holds.

Prompt size still matters for local Qwen, whose window is small; hosted Luna now uses its full window. Keep each new tool description to one line, and run `client.evaluate_conversation` after the prompt change to confirm nothing regressed.

## Testing and rollout

Ship it in three milestones, each usable on its own, with every new behaviour off by default so today's HOLT is unchanged until a setting is flipped.

| Milestone | Phases | Done when |
| --- | --- | --- |
| 1. Heat sink by voice | 1, 5a, 2, 4 (tier 1, no cancel) | "HOLT, heat sink" on Mouse5 fires in game, supervised, with zero false fires on the test set and every stage's timing logged |
| 2. Quiet and interruptible | 5b, 6, 3, 7 | PTT silences HOLT; interdiction interrupts banter; "stop" halts a plot mid-recipe with no stuck keys. Commentary stays off and is not part of this milestone |
| 3. Captain's log | 8 | "What did we do last night?" answered from session facts; a dictated entry saved verbatim |

Phase 5a comes before Phase 2 because PTT edges travel on the event stream; the rest of Phase 5 waits for Milestone 2.

A fourth milestone, **Copilot planning** (Phase 9), follows the captain's log, because objectives and finished plans are stored there.

### Before building

Milestone 1 is ready to start; each remaining unknown is settled by a test on SKIPPY, not by more design. Do these first, in this order:

1. **Spike: button capture.** Confirm Raw Input reads mouse button 5, the BBI-32 and the HOTAS while Elite has focus. If a HOTAS reads nothing, switch that device to the DirectInput fallback before building Phase 2. Settle what `Joy:<name>` matches: the HID product string, or the device name Elite writes in `.binds`.
2. **Spike: GPU Whisper.** Download and pin whisper.cpp's Vulkan build and `large-v3-turbo`; `start-holt-transcription.ps1` currently hard-codes the CPU binary, `small.en` and `-ng`. Then measure:
   - latency idle, and while Qwen is mid-turn, including image prefill from a control screenshot, the worst contention case on the shared iGPU;
   - whether the server's `verbose_json` returns per-segment `no_speech_prob` and `avg_logprob`, which the Phase 4 confidence gate needs;
   - whether a shortened `audio_ctx` causes repetition or hallucination on the command set before adopting it.

   That number becomes the Phase 4 latency target.
3. **Phase 1 in-game checks.** Confirm the binding names in the action table against the current `.binds`, whether silent running or any other toggle is in hold mode, the supercruise rule per action, and that gear stays put while landed. Record results in `docs/validated-ship-actions.md`.
4. **Captain's log work.** Resolved: the earlier work was the playbooks system, which stays separate (see Phase 8).

Implementation happens on SKIPPY, where the code, the game and the hardware for these spikes all are.

Defaults: `ptt_mode: off` until Milestone 1 passes, then hold, `endpoint_mode: fixed`, `commentary: off`. The fast path follows **Explicit game commands**, so unticking it disables it too.

### Rust tests (`cargo test`, any OS)

- `ship_action` with the `Recorder` sender and fixture Status.json files: `already` presses nothing; a toggle presses once; panel open, docked, landed and in-SRV refuse; unbound names the binding.
- `events.rs`: journal tailing across a partial last line and a file rotation; flag diffs; `Last-Event-ID` resume and `reset` in `tests/http.rs`.
- Cancellation: a recipe cancelled during `wait_for_gui` stops before its next step, and `HeldInputs` releases every key it recorded as down.
- `captains_log`: session boundaries, earnings totals and incidents from fixture journals, extending the existing `Journal.2026-01-0*.log` fixtures.

### Python tests (`unittest`, no game)

- `test_holt_fastpath.py`: about 80 cases, each asserting the exact actions or none. Must fire: plain commands, known garbles, compounds of commands. Must not fire, including every case the review measured at a fuzzy score of 100:
  - "did you drop a heat sink", "what's a heat sink", "should I heat sink?"
  - "the lights on that station", "keep the hardpoints in for now"
  - "don't deploy hardpoints", "drop a heat sink when we're hot"
  - COVAS lines: "Hardpoints deploying", "Heat sink launched", "Landing gear deployed", and real COVAS recordings for the voice pack in use
  - wing or Discord voices with PTT up: a bare "chaff!" or "heat sink!" must not fire under `toggles_only`
  - "gear up" while landed must be refused
  - "hardpoints in" must never match `out`, and "silent running off" must be off
- `test_holt_segmenter.py`: synthetic frames for `off`, `hold` and `hybrid`, including a release mid-word, a short held "chaff", a press during playback, and a hold past `[ptt] max_hold_s`.
- `test_holt_speech.py`: priority preemption, expiry and alert merging with a fake speaker.
- `check_endpointing.py` and `check_fastpath.py`, as described in Phases 3 and 4, on recorded audio.

### In-game validation

On SKIPPY, supervised, in a safe spot: each `ship_action` once from each state, recording the binding, hold time and how long the flag took to flip. Write it up as `docs/validated-ship-actions.md`, in the same style as `validated-odyssey-recipe.md`.

### Dependencies

- Python: `rapidfuzz` and `onnxruntime` (Smart Turn), added with hashes to `requirements/holt-windows.lock`.
- Rust: `tokio-util` as a direct dependency in `Cargo.toml` (it is only transitive today), plus the `windows` crate features for Raw Input and DirectInput.
- Models: the Smart Turn ONNX file, pinned by checksum like the other downloads.
- Client settings: add `ptt_mode`, `open_mic_fastpath`, `endpoint_mode` and its thresholds, fastpath\_flavor, and `commentary` to `DEFAULTS` in `client/state.py`, since `load_settings` drops unknown keys.

## Open questions

- [x] **Existing captain's log work.** It isn't on any pushed branch (`main`, `screenshot`, `windows-install`). Where does it live, and should Phase 8 extend it or replace it? Decided: it was the playbooks system, which is complementary, so Phase 8 builds the log as its own store.
- [x] **Tighten `plot_route` too?** In natural mode the model can plot to any system today. The same hard gate as `ship_action` would require the destination in the Captain's words or a confirmed plan leg. Worth doing in Phase 1? Decided: no. plot\_route stays as it is; the gate applies to ship\_action only.
- [x] **PTT button.** Is mouse button 5 on SKIPPY unbound in Elite, or should PTT use a BBI-32 or HOTAS button? Decided: any of them, set as an ordered fallback list in holtzman.toml.
- [x] **Default PTT mode** once validated: `hold` (quietest) or `hybrid` (hands-free chat kept)? Decided: hold, the default once Milestone 1 validates PTT.
- [x] **Faster command transcription:** GPU Whisper for everything, or a small CPU model for held-PTT clips only? Decided: GPU, for everything, with a larger model for quality; benchmark large-v3-turbo first.
- [x] **Hardpoints and flight assist** in the fast path: in, or kept to the model path? Decided: both in; flight assist added to the phrase table.

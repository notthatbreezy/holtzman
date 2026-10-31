# HOLT implementation and offline validation

The implementation plan is `holt-implementation-plan.md`. The source changes in
this checkout cover its offline software work. None of the milestones is declared
live-validated: their acceptance criteria include supervised Elite Dangerous use.
The running server, production configuration, installed Tavern data, audio devices,
and game were not changed or exercised during implementation.

## Implemented

| Area | Implementation |
| --- | --- |
| Ship actions | Ten typed actions, hold-mode parsing, missing-binding coverage, complete cockpit checks, physical-modifier refusal, idempotent on/off, and Status.json verification. The same state checks protect matching `press_binding` calls. |
| PTT | Server Raw Input reader for mouse/function keys/HID buttons, HID product-name identity, connected-device fallback, authenticated `/ptt` learn/configuration, comment-preserving edits, reload errors, snapshots and maximum-hold release latch. |
| Audio pipeline | Capture, transcription and conversational turns run separately in both clients. Held provenance and trace IDs travel with each utterance; stale audio and disconnected generations cannot execute commands. |
| Endpointing | Fixed mode remains default. Optional standalone checksum-pinned Smart Turn ONNX adapter runs checks in a worker; stale results are discarded, inference is timed, and failure uses fixed mode with five-minute retry. |
| Reflexes | Anchored whole-utterance grammar, exact state words, explicit garbles, consumable confidence/address/PTT gates, deduplication, one-shots first, no retries, receipt history and optional tool-free flavour. Cancellation and dictation bypass the model. |
| Events | Authenticated and Host-checked SSE plus long polling, 2,000-event ring, restart/old-cursor reset, current snapshots, partial-line/rotation-safe tailing, flag/status/journal/file/action events. Slow resync reads run outside the PTT consumer. |
| Speech/callouts | One expiring priority queue, urgent preemption, alert merging, PTT playback inhibition, journal-based hull alerts, interdiction/shields/attack/docking/fuel/mission watches, optional tool-free commentary. |
| Cancellation | Tracked controls, step events, cancellation through waits and verification by dropping the operation future, release-on-drop key/mouse guards, and cancellation before panel shutdown. A lost link is reported as unconfirmed. |
| Captain's log | Journal-derived sessions, earnings/spending/discoveries/incidents, separately stored verbatim entries and labelled narratives, session attachment, scratch-within-one-minute, voyage reports. |
| Planning | Persistent objectives with journal-backed measures, historical mission details reconciled with login missions, code-built briefings with bounded optional trade search, verified route/mission references, quantity/location-checked event progression, expiry and two-turn next-leg confirmation. |
| Interfaces | Desktop and Tavern settings/status/learn controls, Tavern command-line display without transcript generation, updated persona/server/client contracts. |

## Defaults and operation

New commands are **opt-in**, separately from the existing game-control permission.
`fastpath_enabled=false`, `ptt_mode=off`, `endpoint_mode=fixed`,
`callouts_enabled=false`, `commentary=off`, `session_reports=false`, and
`session_briefings=false`. `open_mic_fastpath=toggles_only` and
`fastpath_flavor=sometimes` apply only after enabling reflexes. Unticking game
controls blocks reflex input immediately; cancellation still works.

This deliberately resolves a conflict in the plan: existing `allow_actions=true`
does not by itself activate the new reflex grammar. Toggle verification retains the
input reservation until completion, rather than allowing another control to race
the first request's stale flag. One-shot acknowledgements say that input was sent;
they do not claim an ammunition launch was verified.

Server configuration, when ready for supervised testing:

```toml
[ptt]
inputs = ["Joy:<actual HID product name>:5", "Mouse5"]
tail_ms = 150
max_hold_s = 30

[ship_actions.hardpoints]
enabled = true
hold_ms = 80
# Optional overrides: allow_docked, allow_landed, supercruise.
```

Use learn mode for an actual joystick product name; `T16000M` in the design was an
example, not a discovered device ID. The server observes rather than consumes the
button, so choose an input that Elite does not use. Empty `ptt.inputs` registers no
input classes. An older server binary will reject the new configuration sections;
remove them before rolling back.

The captain's store defaults to `captains_log/` beside the server configuration;
`captains_log_dir` overrides it. Knowledge writes never grant game-control authority.
Session date ranges use the server's local calendar; timestamps are UTC. End balances
without a later LoadGame are computed from the listed credit events, not a shutdown
balance. Missing mission details or measures remain unknown.

Install the updated normal Windows lock file with the existing installer. The new
dependency wheel hashes are also isolated in `requirements/holt-reflex-windows.lock`
for `pip install --no-deps --require-hashes -r ...` integrity checks. Smart Turn is
downloaded only by explicitly running `python scripts/download-holt-turn.py`;
neither startup nor tests download it. The adapter follows the
[upstream feature-extraction contract](https://github.com/pipecat-ai/smart-turn/blob/main/inference.py),
with the [v3.0 model commit and checksum](https://huggingface.co/pipecat-ai/smart-turn-v3/commit/100f018c9e4302f7e0f39ea49f0fe7b075b918e6).

## Review and verification

Two independent reviewers examined server and client changes, with a separate
initial review of the design. Their findings led to regression coverage for toggle
races, stale snapshots, journal tails, log visibility, continuous trade routes,
PTT provenance, speech inhibition, confirmations and slow telemetry reads.

Validation uses `Recorder`, fake screenshots/speakers, scripted button edges,
synthetic PCM, temporary journals/SQLite and loopback mock HTTP servers. It does not
construct live speakers, capture devices, invoke SendInput, start inference services,
or request game tools from the production server. Some tests require execution
outside the sandbox because Windows denies its temporary-file/asyncio/loopback use.
That permission changes the test environment, not the fake-input implementation.

Run `cargo test --offline` and Python `unittest discover -s tests -p 'test_holt_*.py'`.
`client.check_fastpath` and `client.check_endpointing` are prepared for a recorded
audio corpus, without executing any game inputs. Conversation model-quality
evaluation remains separate from deterministic prompt/grammar tests.

Final offline result: **73 Rust tests and 99 Python tests passed**. `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, JavaScript/PowerShell syntax
checks and `git diff --check` passed. The final client review independently reran
the queued-confirmation-versus-stop regression and confirmed the fix. No live
acceptance criteria are implied by these results.

## Deferred validation and hardware-dependent work

Use [the supervised validation sheet](validated-ship-actions.md) after the Captain
is ready. No latency target or in-game pass is claimed yet.

1. Mouse5, BBI-32 and HOTAS capture while Elite has focus; unplug/replug fallback,
   maximum hold, LAN loss and reconnect. Raw HID is compiled and tested through
   scripted edges. A DirectInput fallback remains a follow-up **if that hardware
   spike establishes a need**; it is not represented as implemented.
2. Verify actual binding names, toggle/hold settings, supercruise behavior, physical
   modifiers, panels, landed gear refusal, and flag transition latency in Elite.
3. Pin/install the chosen Vulkan whisper.cpp runtime and large-v3-turbo assets on
   ARCHIE, then measure idle and Qwen-contention latency and verbose confidence.
   The transcription launcher accepts explicit CPU/Vulkan/runtime/model choices;
   its current CPU default was preserved. No runtime/model switch was made here.
4. Run recorded real COVAS/Discord/command audio through the two check scripts.
   Calibrate Smart Turn cut-offs/tail jitter and inference speed before changing
   defaults. Missing Whisper confidence fails closed for consumable reflexes.
5. Perform supervised cancel-during-plot, PTT barge-in and each milestone's full
   acceptance checks. Run the conversational evaluation on ARCHIE's model.
6. Populate the hand-maintained blueprint table only for selected blueprints with
   verified costs. No blueprint cost is inferred from journals or fabricated.

The optional tier-two router, simpit lamps and MFD screen remain out of scope as
specified in the plan. The production rollout and default-mode change follow live
validation; this change does not deploy or restart anything.

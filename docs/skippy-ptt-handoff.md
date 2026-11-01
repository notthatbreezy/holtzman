# Handoff to Codex on SKIPPY: Numpad Enter PTT

Paste the following into the chat on the Elite PC. The ARCHIE changes were pushed
to main beginning with `4b51fa3`; use the latest main, including follow-up fixes.

---

Please finish the SKIPPY side of HOLT's **Numpad Enter push-to-talk** setup.

Architecture: SKIPPY runs Elite Dangerous and Holtzman. ARCHIE runs the microphone,
Whisper transcription, standalone SillyTavern and Chatterbox speech. SKIPPY should
only observe the PTT key and publish button events; it does not need speech models,
microphone/audio routing changes or another HOLT client installed.

1. In `C:\Users\chris\holtzman`, inspect local changes and safely pull the latest
   `origin/main`. Do not discard local configuration or unrelated edits. Main must
   include `4b51fa3` or a descendant: that adds the distinct `Key_NumpadEnter` input.
   The previously running server was `d78c6bc7ae73`, which does not support it.

2. Build/update/restart the Holtzman server using the repository's existing script:

   ```powershell
   cd C:\Users\chris\holtzman
   git pull --ff-only
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/update-holtzman-server.ps1
   ```

   Keep the existing bearer token, listen address/port 7777, allowed hosts, journal
   paths and firewall configuration. The updater builds before stopping the server,
   preserves a backup binary and supplies the correct build commit. If a build
   fails, investigate it; do not replace the working server with a failed build.

3. After the new binary is running, configure SKIPPY's existing `holtzman.toml`:

   ```toml
   [ptt]
   inputs = ["Key_NumpadEnter"]
   tail_ms = 150
   max_hold_s = 30
   ```

   Update an existing `[ptt]` section rather than creating a duplicate, and preserve
   unrelated settings/comments. The running server reloads PTT configuration.
   Alternatively, use the authenticated `POST /ptt` with body
   `{"input":"Key_NumpadEnter"}` after updating the binary; ARCHIE's new ST panel
   also provides this action. Do not configure this value in the old binary.

4. Verify using the existing bearer credential read locally, without printing it:
   - `status` should report the new build commit.
   - Authenticated `GET /ptt` should report `input: "Key_NumpadEnter"`, `down: false`
     while released, and `error: null`.
   - Authenticated SSE `GET /events` should show a fresh `ptt` event with `down: true`
     when the Captain physically presses **keypad Enter**, and `down: false` on
     release. Main keyboard Enter must not generate that PTT event.
   - If connecting by loopback/IP fails Host validation, retain the configured
     authorized Host header, e.g. `Host: skippy:7777`; do not weaken Host validation.

5. Ask the Captain for the physical press/release test. Do not simulate game
   controls, remap Elite bindings or choose a different PTT key silently. Holtzman
   observes rather than consumes the key, so it still reaches Elite; choose a
   suitable in-game screen for that test.

Report back the running commit, `/ptt` input/error state and whether both physical
edges were observed. Never paste the bearer token or the full credential-bearing
configuration into chat.

ARCHIE already has the standalone ST **HOLT voice input** panel and its input-only
bridge on `127.0.0.1:8083`. Microphone capture is off pending your update. Once
SKIPPY is ready, the Captain can select HOLT, open **Extensions → HOLT voice input**,
then **Start listening**, and hold Numpad Enter on SKIPPY while speaking into the
Windows default ARCHIE microphone. Chatterbox continues handling output through ST's TTS
provider. See `docs/speech-flow-and-ptt.md` for the full block diagram and lifecycle.

---

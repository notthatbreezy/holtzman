# Ship-action supervised validation sheet

**Not yet validated in Elite Dangerous.** This is the handoff sheet for the tests
deferred at the Captain's request. Offline Recorder/fixture tests are recorded in
`holt-implementation-status.md`; they establish software behavior, not game behavior.

Before testing: safe location, supervised, keyboard Secondaries configured, no
unintended Elite binding on the PTT input. Record server/client build, controls
preset, PTT input, journal directory and transcription runtime/model.

| Action | Intended binding | Hold mode/key verified | Normal-space on/off or press | Docked/landed/panel refusal | Supercruise rule | Flag latency |
| --- | --- | --- | --- | --- | --- | --- |
| hardpoints | DeployHardpointToggle | pending | pending | pending | analysis-mode only; pending | pending |
| landing_gear | LandingGearToggle | pending | pending | landed must not launch; pending | refused; pending | pending |
| cargo_scoop | ToggleCargoScoop | pending | pending | pending | refused; pending | pending |
| lights | ShipSpotLightToggle | pending | pending | cosmetic exception; pending | pending | pending |
| silent_running | ToggleButtonUpInput | pending | pending | pending | pending | pending |
| night_vision | NightVisionToggle | pending | pending | cosmetic exception; pending | pending | pending |
| flight_assist | ToggleFlightAssist | pending | pending | pending | pending | pending |
| heat_sink | DeployHeatSink | pending | pending | pending | pending | unverified one-shot |
| chaff | FireChaffLauncher | pending | pending | pending | pending | unverified one-shot |
| shield_cell | UseShieldCell | pending | pending | pending | pending | unverified one-shot |

Also verify: no input for `already`; only one press for a toggle; no automatic retry;
busy during a plot; modifier refusal; stop during a key hold, GUI wait and route
verification; no stuck keys; final panel reported accurately.

PTT/audio: Mouse5, BBI-32 and HOTAS with game focused; ordered device fallback;
ten-second learn mode; hold/hybrid/off; short chaff/stop; release mid-word; 150 ms
tail; maximum hold; PTT during playback and a model turn; LAN disconnect/reconnect
without open-mic fallback or replay. Record release→Whisper→match→LAN→keypress→TTS
timing, including shared-iGPU contention. Do not set a latency target beforehand.

Reflex corpus: questions, negation, conditional requests, COVAS lines, wing audio
with PTT up, exact in/out and on/off, known garbles, compounds, low/missing Whisper
confidence, controls disabled. Required result: no unintended game inputs.

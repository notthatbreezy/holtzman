# SKIPPY audio handoff for Codex

Set up **only SKIPPY's audio side** of HOLT's two-way VBAN link. The user wants
their Elgato mic and headphones/speakers on SKIPPY, with Elite and Discord audio
still usable. Do not install OBS or change the MCP server for this task. Do not move
the language model, Whisper or Kokoro to SKIPPY. Do not start a second HOLT client.

## Counterpart and agreed network contract

These are LAN addresses observed on 2026-10-26; confirm them before configuring.
HOLT processing PC: **192.168.50.16**. SKIPPY on Ethernet: **192.168.50.41**
(replaces its earlier Wi-Fi address, 192.168.50.21). Both sides use
**UDP 6980**, **48,000 Hz, 16-bit PCM, 2 channels**, initial quality **Medium (2)**.
Stream names are case-sensitive. Prefer Ethernet where available. Reserve addresses
in the router or update both configurations if DHCP changes them. Do not create
router port forwards or expose this audio link to the Internet.

| Direction | Exact stream name | Sender source | Receiver destination |
|---|---|---|---|
| SKIPPY -> HOLT PC | `SKIPPY_MIC` | SKIPPY Banana **B2**, bus index **4** | HOLT Banana **Hardware Input 1**, strip index **0** |
| HOLT PC -> SKIPPY | `HOLT_VOICE` | HOLT Banana **A2**, bus index **1** | SKIPPY Banana **Hardware Input 3**, strip index **2** |

On the HOLT PC the incoming mic strip goes **only to B1**. HOLT will capture the
`Voicemeeter Out B1` recording device and play George to `Voicemeeter Input` (VAIO,
strip 3), which goes **only to A2**. No return voice is routed back to its mic.
The virtual devices may have legacy labels (`Voicemeeter Output` for B1).

## Work on SKIPPY

1. Ask the user to physically connect the **Elgato mic / Wave XLR** to SKIPPY;
   it was not connected there when this handoff was written. Inspect actual device
   names. Confirm which headphones/speakers they want; do not guess an output.
2. Back up current Windows default device names/roles and any existing Voicemeeter
   configuration. Install **Voicemeeter Banana** from the official publisher if
   absent. Standard lacks the separate buses used here. Current staged host version
   is **2.1.3.0**. Verify the installer signature. A driver install needs admin rights
   and a reboot; arrange that with the user, never restart an active game blindly.
3. In Banana set **A1** to the user's physical headphones/speakers. Prefer WDM;
   if device-sharing problems occur, test a shared interface. Keep sample rate 48 kHz.
4. Set **Hardware Input 1** to the Elgato microphone. Label it `Elgato microphone`.
   Enable **B1 and B2 only**; leave A1/A2/A3 off to avoid mic sidetone by default.
   B1 is the mic for Discord/other local apps; B2 is the independent mic send to HOLT.
5. Leave **Hardware Input 3** without a physical device. Label it `HOLT voice`.
   Enable **A1 only** on that strip. Never enable B1 or B2 there.
6. Route **Voicemeeter VAIO** (virtual strip 3, normal game/desktop sound) and,
   if used, **AUX** (strip 4) to **A1 only**, never to the mic buses. If routing
   Windows/Elite/Discord playback through Banana, use `Voicemeeter Input` as the
   playback device, then verify game audio still reaches A1. Preserve or deliberately
   migrate existing Wave Link/Discord routing rather than adding a second capture.
7. Enable VBAN. Outgoing stream 0: name `SKIPPY_MIC`, destination **192.168.50.16**,
   port **6980**, source **B2 (index 4)**, 48 kHz/16-bit/stereo, Medium quality.
   Incoming stream 0: name `HOLT_VOICE`, source **192.168.50.16**, port **6980**,
   destination **Hardware Input 3 (strip 2)**, Medium quality.
8. If required, add a Windows Firewall inbound rule for UDP 6980 scoped to
   **remote address 192.168.50.16** and the actual Banana executable. Inspect the
   active network profile; the counterpart currently uses a Public profile, so do
   not assume Private-only rules work. Do not disable the firewall or make a broad rule.
9. In Discord choose **Voicemeeter Out B1** (`Voicemeeter Output` on older drivers)
   as input. Discord playback can use the VAIO/AUX -> A1 path above. Do not use B2
   as the Discord mic: B2 is the independently controllable HOLT feed. Muting
   the **B2 bus** stops HOLT hearing the mic while leaving Discord's B1 available.
   Discord's own mute does not mute HOLT. The Elgato's physical mute affects both.
10. Save the tested profile and arrange normal user-session startup/loading. Avoid
    running Standard and Banana together. Do not mark this done solely because the
    VBAN switch is green: complete the checks below and report any pending counterpart.

## Checks and report back

- Elgato meter moves on strip 1 and both B1/B2; hardware mute silences both.
- `SKIPPY_MIC` packets reach the HOLT PC, and its B1 recording endpoint carries the mic.
- `HOLT_VOICE` is heard once in SKIPPY's selected headphones, with no local mic meter
  response to HOLT playback. No game/Discord output enters either mic bus.
- Elite audio and Discord mic/playback remain usable simultaneously. Muting SKIPPY
  B2 stops the HOLT feed without muting Discord B1. Avoid a real outgoing Discord
  call/message without the user's direction; use local device checks/mic preview.
- The HOLT PC still needs its saved microphone/speaker selections changed and its
  worker restarted after the peer is ready. Its operator can run:

  ```powershell
  .\.local\holt\voice-env\Scripts\python.exe scripts\setup-holt-audio-host.py --select-holt-devices
  ```

  Then fully restart HOLT Tavern (`HOLT-Tavern.cmd stop`, reopen the shortcut,
  click Start HOLT) to load the new WASAPI playback conversion code. Coordinate
  this with the user; SKIPPY's setup cannot verify
  HOLT's actual transcription or final voice playback by itself.
- Record device names, mixer version, firewall scope, saved-profile/startup paths,
  packet/level evidence, listening test results and any latency/dropouts. Return those
  details to the user to pass back. Do not send messages to another Codex chat without
  the user's explicit authorization. No API key or MCP token is needed for audio setup.

## Host status and recovery

The HOLT PC has Banana **2.1.3.0 installed and configured**, with a saved
`holt-host.xml` profile and minimized **HOLT Audio Bridge** user-startup shortcut.
A1 uses shared MME `Speakers (Senary Audio)` only as the stable audio clock;
no input strip is routed to A1, so HOLT is not monitored on this PC's speakers.
The firewall's active mixer exception is `HOLT-VBAN-SKIPPY`, UDP 6980, remote
192.168.50.41, scoped to `voicemeeterpro.exe`. Broad first-launch app allow rules
were disabled and recorded for rollback. The network profile was not changed.

Local hardware checks passed: 24 kHz HOLT-format playback was converted into the
48 kHz virtual mixer, sent through A2/VBAN to a temporary loopback relay, received
on the mic strip/B1, and captured in HOLT's 16 kHz mono format. **This does not prove
SKIPPY's network/audio end works.** The test restored both real peer addresses.
Idle mixer measurement: about 102 MiB RSS and 9.3% of one CPU core (about 0.3% of
total capacity across 32 logical CPUs), sampled over three seconds. Not a loaded
two-PC benchmark; actual Wi-Fi jitter/latency still needs testing.

The installer and original settings backup are under ignored
`.local/holt/audio-bridge/`. HOLT device selection is deliberately separate from
mixer setup so local conversation can continue until SKIPPY's mic exists.
`scripts/setup-holt-audio-host.py --apply` backs up the current mixer profile and
sets the counterpart routing above; it does not modify SKIPPY or Windows defaults.
The host's end-to-end checks remain pending until this SKIPPY setup is complete.
Voicemeeter's publisher requires reboot after installation even if endpoints appear.
The HOLT PC has not yet been rebooted after this install. Complete that before
declaring the final installed setup validated. No HOLT API/MCP credentials changed.

`scripts/check-holt-audio-host.py` can repeat the local format/routing check; it
temporarily redirects the two dedicated streams and must not run during a real
remote conversation. It restores stream addresses/ports in `finally`.

To back out, Stop HOLT, restore the host's backed-up `microphone`/`speaker` choices,
and restore previous mixer/Windows defaults on each affected PC. Do not delete the
original backups. Do not uninstall a virtual driver while apps are using it.

## Publisher references

- [Banana download and installation](https://vb-audio.com/Voicemeeter/banana.htm)
- [VBAN network audio](https://vb-audio.com/Voicemeeter/vban.htm)
- [Banana manual](https://vb-audio.com/Voicemeeter/VoicemeeterBanana_UserManual.pdf)
- [Remote API and bus/strip indices](https://download.vb-audio.com/Download_CABLE/VoicemeeterRemoteAPI.pdf)

Banana is donationware: it can be installed/tested before payment; the publisher
expects a license payment if useful or used professionally. The earlier $15 example
was **Standard**, not a fixed Banana price. Do not buy a license without the user.

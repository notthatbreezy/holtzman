# HOLT in SillyTavern

For the independent, researched **Elite Dangerous** universe lorebook, see
[installation and sources](../../docs/elite-dangerous-lorebook.md). It works with
standalone SillyTavern and is separate from HOLT's personal fictional canon.

To populate a standalone HOLT character and its human Captain persona through the
local API, see [character/persona setup](../../docs/holt-character-and-persona.md).

For **standalone ST using its MCP extension directly**, use the
[HOLT local MCP connection](../../docs/sillytavern-direct-mcp.md). It keeps inference
on Ollama and requires ST function calling **on**. The gateway instructions below
describe a different tool loop and keep ST function calling off.

For standalone ST speech, see [Chatterbox installation, start/stop and reference
voices](../../docs/chatterbox-sillytavern.md). Its separate speech service is
controlled by `HOLT-Chatterbox.cmd` and uses ST's built-in Chatterbox provider.

Start with the **HOLT Tavern** desktop shortcut or **HOLT-Tavern.cmd** in the repository root. The browser opens after the
local server becomes ready. Click **Start HOLT**: it selects the HOLT character,
connects SillyTavern to the gateway, starts the speech services and connects SKIPPY.
Wait for **Ready — listening** before speaking. PTT is optional; choose off, hold or
hybrid before Start. Command reflexes are separately opt-in and appear as command
lines without starting a SillyTavern generation.

**Stop HOLT** stops the ship/audio worker and managed inference services. It leaves
SillyTavern open for editing lore/chats. `HOLT-Tavern.cmd stop` closes the trial
servers as well. `HOLT-Tavern.cmd status` shows the worker state. Closing the browser
releases new microphone input through its event-stream lease; it does not shut down
the background services. Stop an active command before closing the interface.
Stop requests server-side cancellation before closing the worker. If the link is
lost, cancellation cannot be confirmed; the worker reports that uncertainty. A plain
disconnect is not itself a cancellation request. Held keys are released when a
cancelled server operation exits; open panels remain open.

The **Voice on Start** checkbox applies on the next Start and is disabled while
running; Stop first to change it. It controls local recognition and playback together.
Mic/speaker devices and the selected George/filter settings come from the existing
HOLT preferences; a blank device selection follows the Windows default when started.
The **Game controls** checkbox applies immediately. No per-command approval popup
is introduced. Use the model selector to change the next turn's reasoning backend.

| Model profile | Backend |
|---|---|
| `holt-hosted` | GPT-5.4 mini, low reasoning |
| `holt-hosted-fast` | GPT-5.4 mini, reasoning disabled |
| `holt-5.6-luna` | GPT-5.6 Luna, low reasoning |
| `holt-6-luna` | GPT-6 Luna, low reasoning; selected trial default |
| `holt-5.6-terra` | GPT-5.6 Terra, low reasoning |
| `holt-6.1-sol` | GPT-6.1 Sol, low reasoning, including auxiliary requests |
| `holt-local` | Existing Qwen3.5-35B-A3B Q4_K_M, thinking disabled |
| `holt-local-thinking` | Same Qwen with thinking enabled and a larger reply budget |

The local model starts on first use and can take roughly fourteen seconds to load
on this PC, before its first answer. It consumes GPU memory until Stop. These are
backend choices, not separate character definitions: HOLT's card/lore stay the same.
SillyTavern's connection-profile menu also contains these profiles. Ordinary tool
rounds happen inside the gateway, so leave ST's own function calling disabled.

## Edit HOLT

Use **Character Management → HOLT** for its description, personality and examples.
Use the character's globe button or **World Info → HOLT canon** for detailed lore.
Core identity remains in the character description; the eight detailed canon entries
activate by relevant keywords. ST's lore budget and scan depth determine what is
included. Neither a retrieved story nor an old chat is a current ship reading.

The seed files in this folder are portable exports. User edits live in the ignored
SillyTavern data directory. Setup refreshes our extension but preserves existing
character, lore and settings. Export edits from ST deliberately if you want them in
Git. The gateway does not overwrite ST's personality with the server biography.

Conversation text lives in ST chat files; its summaries can be inspected/edited in
Extensions → Summarize. Summaries can lose detail or hallucinate; keep fixed canon
in the authored card/lorebook. Structured ship history and verified itinerary context
remain separate in `.local/holt/tavern/memory.sqlite3`. Desktop history is retained
separately at its original location and is not silently merged into ST.

## Install or reproduce

Prerequisites: the existing HOLT Python/model/speech environment, Git and Node.js
20+. On this PC the setup records the bundled Node executable; another machine can
use its installed LTS Node. Use the upstream dependency lock, not unconstrained
dependency updates.

```powershell
git clone --branch release https://github.com/SillyTavern/SillyTavern .local/holt/sillytavern
git -C .local/holt/sillytavern checkout 06bde939fb1e9c4c8d8641d810f0a916b5bce127
Push-Location .local/holt/sillytavern
npm ci --ignore-scripts
Pop-Location
.\.local\holt\voice-env\Scripts\python.exe scripts/setup-holt-tavern.py
.\HOLT-Tavern.cmd
```

The trial used bundled pnpm to import the upstream npm lock and install locked
dependencies with scripts disabled. ST initialized its content and compiled its
frontend on first launch. No upstream source modification or third-party MCP
extension is required. Keep Node/SillyTavern running without elevation.

## Architecture and diagnostics

- Browser ST: `http://127.0.0.1:8000`.
- HOLT gateway: `http://127.0.0.1:8082/v1`; no browser API key required.
- Qwen on demand: `http://127.0.0.1:8080`; Whisper: `http://127.0.0.1:8081`.
- SKIPPY MCP and OpenAI keys stay in Windows Credential Manager. They are never
  copied into ST settings or sent to another endpoint.
- ST supplies character/lore/history once. The gateway adds operational rules,
  automatically refreshed instruments and the complete tool catalogue. It executes
  the existing engine/tool loop and returns the final reply. Quiet/summary calls have
  no tools or speech. Regeneration does not replay a game action.
- Game captures retain the existing explicit hosted-image permission. User-uploaded
  media is rejected at the gateway because it is not a verified game-window capture.
- Web-search links appear in the HOLT sources area; citations are not read aloud.
- Runtime and logs: `.local/holt/results/sillytavern.log`,
  `client-tools.jsonl`, and `tavern-gateway-errors.log` when launched with redirection.
  These contain chat/game data; keep them private. No auth headers are recorded by HOLT.
- Use only one HOLT microphone client at a time. The original **HOLT.cmd** desktop
  client remains available as a fallback. Stop one before starting the other.

The **HOLT diagnostics** disclosure contains **Capture evaluation prompts**. This
creates a separate test chat, runs the fixed fixtures through ST's real prompt/lore
assembly and writes captured prompts under `.local/holt/tavern/captures`. No model
or ship calls occur in capture mode. It is a developer test, not normal conversation.

See [the design](../../docs/sillytavern-design.md),
[the high-level comparison](../../docs/sillytavern-summary.md), and
[the detailed report](../../docs/sillytavern-report.md).

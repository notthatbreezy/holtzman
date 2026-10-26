# HOLT with SillyTavern

Date: 2026-10-26. Status: implemented trial; see the historical report for results and limitations. Selected default: GPT-6 Luna, low reasoning.

## Goal

Use SillyTavern to edit HOLT's character, select relevant lore and manage chats,
while preserving the working ship instruments, verified MCP recipes, local voice
and choice of local or hosted reasoning. Keep the desktop client as a fallback.

## Architecture decision

SillyTavern owns character/lore editing, prompt assembly, chat history and optional
chat summarization. A first-party HOLT gateway exposes an OpenAI-compatible
`/v1/models` and `/v1/chat/completions` endpoint on loopback. Its model IDs select
local Qwen or hosted GPT-5.4 mini, with thinking variants. SillyTavern sends its
assembled messages to this gateway. The gateway runs the existing HOLT tool loop
and returns the final spoken text, rather than nesting a second conversational
agent inside SillyTavern's tool loop. Exactly one component executes tools.

This deliberately differs from registering all tools through a third-party MCP
extension. It reuses the current verified game integration, Responses API adapter,
credential handling, tool/image limits and automatic instrument refresh. It also
avoids copying credentials into SillyTavern. MCP tools are still all visible to the
reasoning model. Tool events and source links can be displayed separately by our
SillyTavern extension, without reading citations aloud.

The gateway adds the client operational contract, live MCP instructions and fresh
instruments, but does **not** append the MCP personality/biography over SillyTavern's
character. ST is the personality source in this mode. Its assembled history is used
once, without also loading desktop conversation history. Gateway conversation and
instrument storage is isolated from the desktop database.

The initial character is exported from the existing authored HOLT canon. Core
identity/personality stays always included; separate keyed lore entries cover the
maker, origin, previous assignment, mug-manifest incident, recommissioning and
preferences. Lore never supplies current game state or evidence of executed actions.
Changes in ST are authoritative there; refreshing seed assets is an explicit action.

## Lifecycle and voice

`HOLT-Tavern.cmd` opens the local interface. A small first-party extension provides
Start/Stop, listening and game-control switches plus model selection. Start manages
the existing local services and the gateway's MCP/audio worker. Stop cancels work
and releases the microphone; shutdown manages only recorded owned processes. Avoid
running both desktop and Tavern microphone loops simultaneously.

The gateway retains George, the selected robot filter, urgency and pronunciation.
Microphone transcripts are delivered to the active ST chat, so voice and typed
messages take the same ST prompt/lore path. Half-duplex speech prevents feedback.
An active browser lease prevents an abandoned tab from accepting new microphone
turns. Server-sent events deliver transcripts and renew the owning lease so
background-tab timer throttling does not drive speech delivery. Explicit Start can
transfer ownership to a new tab. Intentional silence produces no spoken response or forced acknowledgement.

## Request and execution contract

- Loopback-only listener; reject foreign browser origins and unsupported methods.
- No API/MCP credentials in browser data, profiles, logs or committed files.
- Fixed model IDs and backend endpoints; no arbitrary upstream/proxy selection.
- Bound request size and context; normalize multiple system messages for Qwen.
- Reject ambiguous input/malformed data before tools; validate each tool schema.
- Read status automatically before every reasoning round; retain structured recent
  trends and verified task context independently of conversational summaries.
- Honour the live game-control toggle. Planning does not require execution enabled.
- No automatic replay of actions on retry, regenerate, reconnect or duplicate request.
- Stop disconnects/cancels; an already-running remote recipe may finish server-side.
- Cockpit screenshots retain the existing explicit hosted-image consent setting.
- Auxiliary ST summary requests cannot execute game tools and are not spoken.
- Preserve original transcripts. Do not silently truncate current instructions,
  pending tool chains or images to fit a context window.

## Evaluation plan

Compare a 2x2 matrix: existing desktop prompt versus actual ST-assembled prompt,
using local Qwen and hosted GPT-5.4 mini. Add thinking variants when supported.
Use the same scenarios, ship fixtures, authored canon and tool schemas. Include
facts, contradictory old readings, equipment search, trade-plan/follow-up route,
canonical biography, dry banter, unknown facts and intentional silence.

Run no real game controls in benchmarks. Record all tool proposals/results against
a simulated ship, complete replies, wall-clock latency, usage, estimated hosted cost,
errors and explicit scoring criteria. Warm the local model separately and report
cold startup, sample count, model/runtime versions, context sizes and reasoning
settings. Do not conflate backend speed, framework overhead and prompt-length effects.
Use at least repeated representative cases rather than one hand-picked response.

Integration checks cover the actual ST page, character/lore activation, model switch,
gateway HTTP routing, disabled controls, speech output and microphone recovery.
Measure synthetic typed/model performance separately from acoustic recognition and
physical game reliability. A simulated successful plot is not a real-game proof.

## Acceptance and rollback

Both model profiles must answer through ST with the authored personality and current
instruments; core tool and silence cases must remain covered. Start/Stop and the
voice path must work without extra terminals. Record any incomplete acceptance item
in the final historical report rather than claiming success. Preserve HOLT.cmd and
the desktop database. Tavern runtime/data stay git-ignored under `.local/holt/`;
commit bridge, seed assets, setup/launcher code, tests and the report. Pin the tested
SillyTavern revision. No upstream fork or silent background update is required.

## Proposed next layer: intent-aware retrieval

Requested during evaluation; this is a design proposal, not a measured feature in
the first integration. Current mandatory instrument snapshots are structured
context enrichment. ST lorebook keyword activation supplies conditional authored
context. Neither alone is a comprehensive domain retrieval system.

Use a small always-included ship snapshot plus a bounded topic context selected
from the current utterance and the last few turns. A router emits a constrained
intent (commodity buy/sell, trade planning, outfitting, mechanics, biography,
navigation, banter), resolved entities, required facts and confidence. Resolve
canonical commodity/module names and aliases against a local catalogue first.
Use semantic retrieval when helpful; reserve an extra classifier-model call for
ambiguous cases rather than taxing every utterance. References such as "this cargo"
and "that station" must use structured task/current-ship context.

Example: "Where can I sell this uranium?" selects current uranium quantity,
location, large-pad requirement, relevant buyers, reported demand, price, distance
and report timestamps. "Why is uranium valuable?" retrieves a mechanics/reference
entry instead of a large market table. "Find a scoop" routes to modules, never
commodities. Resolve Elite jargon and ASR errors before selecting records where
confidence permits; avoid treating one misleading word as a decisive intent.

Separate stable knowledge (indexed local guides/catalogue), authored roleplay canon,
historical flight events and fresh external market results. Every retrieved item
has provenance, observed/source time, scope and size budget. Expire market caches
and never allow an old retrieved message to override the current ship snapshot.
Retrieve a ranked shortlist, not every market/commodity entry. Retrieval is read-only;
it cannot authorize controls. Retrieved content remains data rather than instructions.

Instrument relevance and cost: record selected intent/entities, retrieved items,
token counts, fetch latency, cache hits and a failure reason when a source is absent.
An uncertain classification should supply a small useful context or let the main
model use its tools, rather than forcing irrelevant records or a canned refusal.
Budget network requests, parallelize independent reads, and tolerate unavailable
providers without fabricating stock or prices.

Evaluate on a separate corpus: buy versus sell, minerals versus modules, pronouns,
misspellings/ASR, lore using commodity words, multiple topics, stale markets and
conversation corrections. Compare relevant-record recall, distracting context,
task success, input-token cost and end-to-end latency against the current baseline.
ST vector retrieval may help static notes, but time-sensitive market selection and
ship constraints belong in the HOLT context service rather than a generic chat index.

## Sources

- [Character design](https://docs.sillytavern.app/usage/core-concepts/characterdesign/)
- [World Info](https://docs.sillytavern.app/usage/core-concepts/worldinfo/)
- [Connection profiles](https://docs.sillytavern.app/usage/core-concepts/connection-profiles/)
- [Extension hooks](https://docs.sillytavern.app/for-contributors/writing-extensions/)
- [Function calling](https://docs.sillytavern.app/for-contributors/function-calling/)
- [Summarization](https://docs.sillytavern.app/extensions/summarize/)
- [GPT-5.4 mini](https://developers.openai.com/api/docs/models/gpt-5.4-mini)
- [Responses reasoning continuity](https://developers.openai.com/api/docs/guides/reasoning)

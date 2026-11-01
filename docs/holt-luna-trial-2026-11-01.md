# HOLT factual-grounding investigation and Luna trial — 2026-11-01

The user reported invented navigation answers after the conversational changes.
The live chat confirmed the problem: HOLT supplied unsupported nearest-system,
distance and route claims, then repeated them after correction. This was not a
speech-rendering issue. The user authorised trying Luna and deferred adding the
missing general galaxy lookup tool.

## Findings

- The native MCP connection still advertised all 33 tools and function calling was
  enabled. Most offending replies contained no tool call. A later `status` call
  did not supply distant system coordinates or a nearest-Empire ranking, yet the
  model repeated its answer anyway.
- The 32.4 figure matched an earlier fuel reading, not a verified route distance.
  From the earlier recorded Crucis Sector AL-X b1-5 coordinates
  `[62.625, 36.125, 22.875]` to Achenar's EDSM coordinates
  `[67.5, -119.46875, 24.84375]`, Euclidean distance is **155.683 ly**.
  These are historical-origin coordinates: the commander had moved by the later
  status call. This is not a claim about the current distance or closest system.
  [EDSM system data](https://www.edsm.net/api-v1/system?systemName=Achenar&showCoordinates=1&showInformation=1)
  supplied the destination coordinates.
- The current direct ST tool set has ship telemetry, market lookups and route
  controls, but no general system/allegiance search or web-search function. A
  status receipt cannot fill that gap, and a commodity search is not an equivalent
  nearest-system query. This gap is deferred at the user's request.
- The existing chat contains repeated unsupported answers about navigation and
  game mechanics. It can reinforce the model's mistakes, but a fresh context alone
  did not solve Qwen's tool-selection failures.

## Intercepted comparison

Tests advertised the same 33 real tool schemas through the existing schema
normaliser. Status/guide results were synthetic fixtures. Every proposed control
or write was intercepted; no game keypress, route change, log write or real MCP
call was executed by these model checks. The contaminated context was an abbreviated
reproduction of the bad claims, not the user's full chat.

| Configuration | Result |
| --- | --- |
| Qwen, operational prompt, fresh nearest-system case | Proposed a dummy commodity query with an invalid numeric argument. Failed. |
| Qwen, revised capability guidance, fresh context | Proposed plotting to Sol as an allegiance-search method. Failed; intercepted. |
| Qwen, contaminated-history correction/audit | Admitted unsupported claims, but still offered an unavailable future search. |
| Luna low, Responses API | Audit, route proposal and banter useful; missing-lookup cases still tried unsuitable market/log tools. Not an all-pass result. |
| Luna none, Chat Completions | Fresh and contaminated cases eventually admitted no verified nearest-system result; audit retracted distances; correct `plot_route(Achenar)` proposal; short banter. |

Luna none took about .8–2.5 seconds per model request in these small tests. Its
missing-lookup scenarios each needed three requests, so those numbers are not
complete spoken-turn latency. Qwen requests with this tool inventory took around
10–14 seconds each. Cache and prompt shapes differed; this is a practical trial,
not a comprehensive speed or factuality benchmark. One run per case does not
establish that either model will always obey the instructions.

[Raw synthetic checks](evaluations/2026-11-01-grounding.json) retain failures as
well as the successful trials. No credential is included.

## Applied setup

- Added and selected **HOLT Luna MCP**, preserving **HOLT local MCP** and **Ollama**.
- Model `gpt-6-luna`, custom endpoint `https://api.openai.com/v1`, native ST tool loop
  and all 33 MCP tools retained. No older HOLT gateway or extra inference proxy is
  started. Speech remains local Chatterbox Nano; input/output routing is unchanged.
- Response allowance stays 2048. OpenAI supports multiple system messages, so the
  Qwen-only `semi_tools` workaround is disabled in the Luna preset.
- The existing authorised OpenAI key was copied from Windows Credential Manager
  through ST's normal secret API. ST stores it in its private `secrets.json` and
  the connection profile stores a secret reference. No key is in the preset,
  character, model messages, repository or report. The Windows credential remains.
- HOLT card **1.4-standalone** has an operational main-prompt override, replacing
  ST's default invitation to write a fictional chat. Authored personality/history
  remain fiction; live gameplay claims require evidence. The note requires actual
  action receipts and refuses to treat earlier assistant claims as verification.
  The character-main-prompt preference was verified enabled.

### Reasoning compatibility

Luna supports none, low, medium, high, xhigh and max. In this direct ST setup,
**Chat Completions with function tools requires none**. A low-plus-tools request
returned HTTP 400 explicitly directing the client to Responses or none.
[Official Luna documentation](https://developers.openai.com/api/docs/models/gpt-6-luna)
states this restriction. Low plus tools was accepted through the existing Responses
transport for the comparison, but native ST here does not implement that transport.
Higher effort with tools requires an adapter or Responses-capable integration;
changing the ST dropdown alone is insufficient.

The Luna preset's Custom Parameters use:

```yaml
# Additional body parameters
reasoning_effort: none
max_completion_tokens: {{maxResponseTokens}}
```

Excluded body parameters are `max_tokens`, `temperature`, `top_p`,
`presence_penalty`, `frequency_penalty`, `seed`, `n`, `stop` and `logit_bias`.
ST expands the response-token macro before dispatch. This avoids the older
client's missing GPT-6 Luna parameter mapping. The explicit body parameter is
authoritative; the generic Reasoning Effort dropdown alone does not change it.

A smoke request through ST's own backend and secret handling returned model
`gpt-6-luna`, `finish_reason: stop`, with all 33 tools advertised. This is stronger
than only checking a direct Python call, but it is not a new live route test.

## Use a fresh HOLT chat

**Recommended for this trial.** Open HOLT, then use the chat menu's **Start new
chat** action. Keep the old chat as history; do not delete it or carry its generated
summary into the new chat. The old thread was not rewritten during this work.

A new conversation still has HOLT's updated card, linked lorebooks, Captain
persona and connected MCP tools. Server-side journals, fleet records and saved
playbooks are not erased. Conversation-only plans will need to be restated;
copy only verified facts or the user's goals, not the old assistant's claims.

To return to local reasoning, select the **HOLT local MCP** connection profile.
Its Qwen model and tool-preserving prompt conversion remain available. Chatterbox
and the voice-input bridge need no model-specific restart.

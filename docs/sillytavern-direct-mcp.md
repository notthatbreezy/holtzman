# Standalone SillyTavern with Holtzman MCP

Checked on **2026-10-31**, against the user's separate SillyTavern installation
at `C:\Users\chris\SillyTavern`, served on `http://127.0.0.1:8000`.
This is distinct from the older trial installation under `.local/holt/sillytavern`.

The installed integrations are the `SillyTavern-MCP-Server` server plugin and
`SillyTavern-MCP-Client` browser extension. The MCP server entry is **holtzman**.
Its settings may contain credentials; do not publish `mcp_settings.json` or copy
its contents into chat.

## Why the connection screen worked but HOLT had no tools

The MCP plugin was connected and had all 23 tools enabled. A read-only `status`
request through `/api/plugins/mcp/servers/holtzman/call-tool` succeeded.

SillyTavern itself was using **Text Completion → Ollama**, with function calling
off. In the installed ST version, `ToolManager.isToolCallingSupported()` requires
the Chat Completion API (`main_api == 'openai'`, ST's internal name for that mode)
and `function_calling == true`. The extension registers its tools with this tool
manager; a successful MCP connection does not override the generation-mode check.

The selected local model, **qwen3-coder-next:q8_0**, advertised both `completion`
and `tools` capabilities through Ollama's `/api/show`. Its compatible `/v1/models`
endpoint also exposed that model ID. It was already loaded with an 8,192-token
context. No model download or hosted provider is needed to expose tool calls.

## Saved connection

The **HOLT local MCP** connection profile and Chat Completion preset use:

| Setting | Value |
| --- | --- |
| API mode | Chat Completion |
| Source | Custom (OpenAI-compatible) |
| Base URL | `http://localhost:11434/v1` |
| Model ID | `qwen3-coder-next:q8_0` |
| Enable function calling | On |
| Prompt post-processing | None |
| Context | 8,192 tokens, matching the running Ollama model |

The name OpenAI-compatible describes the protocol; inference stays on local
Ollama. The original Ollama connection profile is retained. The new profile was
saved through the running ST UI, avoiding the stale settings overwrite encountered
when API writes were made behind an open editor.

The 23 tool schemas alone occupy about 20,950 JSON characters before tokenisation.
They share the context window with HOLT's character, Captain persona, relevant
lore, chat and returned tool results. An 8k window therefore has limited room for
long sessions or large journal queries; the original 4,095-token ST Chat Completion
default was particularly tight. This check does not establish an optimal context
size or performance setting for the local model.

## Repeat a read-only connection check

Obtain a cookie and CSRF token from `GET /csrf-token`, then use the normal local
SillyTavern API:

- `GET /api/plugins/mcp/servers`: connection state and cached tools; avoid printing
  raw server command arguments because user-configured arguments can contain secrets.
- `GET /api/plugins/mcp/servers/holtzman/list-tools`: tool schemas and enabled states.
- `POST /api/plugins/mcp/servers/holtzman/call-tool` with
  `{"toolName":"status","arguments":{}}`: a real read-only tool request.

This tests the installed ST bridge rather than connecting a separate MCP client
directly to SKIPPY. Check `result.status`, `result.data.isError` and the returned
content. Cached tool discovery can succeed even when a later actual call fails.

Journal-derived status can be old while the MCP connection is healthy. The bridge
check returned a status timestamp of `2026-10-31T04:30:08Z`; it must not be treated
as a current live reading without checking freshness. Null fuel/flags or a stale
timestamp do not, by themselves, prove a broken MCP transport.

## Conversation test result

After changing the connection through the UI, the existing HOLT chat was asked:

> HOLT, check our recorded ship status and tell me the system, station and ship.
> Mention the data timestamp if it is not current. This is a read-only check; do
> not send any game controls.

The live ST conversation displayed **Tool calls: holtzman: status**, followed by
Qwen's response using the returned system, station and ship records. No game-control
tool was called. This verifies browser registration, model-generated tool selection,
the installed ST MCP bridge, the remote status response and the final local-model
reply together. The test ran locally, without hosted inference. It took approximately
two minutes, so tool access is functional but this check does not establish acceptable
conversational latency.

The answer was too verbose and described the location as current before displaying
the old timestamp. It quoted the record's `2026-10-31T04:30:08Z` correctly but did not
adequately qualify its freshness. That remains a model/prompt limitation of the
tested reply; it must not be presented as a fresh cockpit observation. The test does
not validate docking, route plotting, vision or long-session compaction. The selected
Qwen model advertises completion and tools, not image input.

Screenshot and the initial bridge result are retained privately in
`.local/holt/results/sillytavern-mcp-working-2026-10-31.png` and
`.local/holt/results/st-mcp-status-check.json`.

## Fresh-chat follow-up and preferred presentation

After adding the version 1.2 Character's Note, a new chat was created through ST's
**Start new chat** action, keeping the previous chat file. It began with only
HOLT's greeting. The Captain asked:

> HOLT, what station are we at? Check the ship records, please.

HOLT independently called **holtzman: status** and answered in prose, opening:

> We're currently docked at **The Pillar of Fortitude**, in the **SPF-LF 1** system.

It then included ship, capacity, jump range and credits. The Captain explicitly
liked this output and said the stale-data wording was not a concern. Keep this
presentation as the accepted baseline; no further freshness-focused tuning was
made. This is an output preference, not a verification that every interpretation
was accurate: the reply also described rank progress as maximum faction progress,
which this test did not establish.

The fresh-chat run again took approximately two minutes. A separately timed
read-only request through ST's MCP bridge took about **1.04 seconds**. Therefore
MCP connectivity alone does not explain the conversational delay; the remaining
model/prompt-generation path needs profiling before attributing it to a particular
cause. No hosted model was called, no game controls were sent, and no tools were
disabled. The selected model and connection remain **HOLT local MCP**.

Fresh-chat evidence is retained privately at
`.local/holt/results/sillytavern-mcp-fresh-chat-2026-10-31.png`.

## What a new chat remembers

A new ST chat has its own conversation history. The HOLT character, selected
Commander Brown persona, associated lorebooks and connection settings still apply,
because those are configuration rather than memories from the previous chat.
Keyword-triggered lore entries are included when ST's lore rules select them.

For this fresh-chat reply, ST's prompt itemization reported **zero tokens** from
Summarize, Smart Context, vector chat retrieval and vector Data Bank retrieval.
The Character's Note was character configuration, not a prior-chat summary.
This verifies that the successful tool call did not depend on the old conversation;
it does not demonstrate automatic cross-chat memory or long-session compaction.
The direct MCP setup also does not automatically use the older HOLT gateway's
SQLite memory store. Explicitly saved server playbooks are separate MCP data and
can be retrieved when needed, independently of a particular ST chat.

## Difference from the HOLT gateway profiles

The original `holt-*` gateway profiles execute tools inside our HOLT Python engine
and keep ST's own function calling disabled. This direct integration executes tools
through the installed MCP extension and **requires** ST function calling enabled.
Do not mix those two configurations or apply the old gateway advice to this profile.

The character and lorebooks supply behaviour and background. They do not register
tools, change API mode or grant a model capabilities absent from its connection.

# HOLT old-result compaction and Nemotron trial — 2026-11-02

HOLT now compacts selected large, historical tool results in the **outgoing ST
prompt**. The original conversation and receipts remain in the saved chat. This
is a deterministic extension, not an extra summarizing model call. Nemotron is
installed in Ollama and selected in a separate **HOLT Nemotron MCP** profile;
the existing Qwen and Luna profiles remain available.

## Behavior and design

The `integrations/sillytavern/context-compaction` extension uses ST's supported
generation interceptor at order 90, before Vector Storage at order 100 and before
the real generation's token budget is calculated. It applies only to the solo
HOLT character. Native dry-run prompt previews skip these interceptors; their
token totals can therefore still show the unmodified history.

Defaults: enabled, keep the newest **two user turns** in full, compact eligible
results larger than **5,000 characters**. Recognized successful JSON receipts are
`fleet`, `recent_events`, `status`, and `session_briefing` from `mcp_holtzman_*`.
Fleet summaries preserve identities and locations with bounded equipment samples;
event summaries preserve counts, returned time bounds and a bounded recent tail.
Omissions and the historical nature of the result are explicit. Refetch details
when a later question needs them. This is deliberately lossy context reduction,
not permanent memory or an assertion that omitted equipment does not exist.

Unknown schemas, malformed results, images, guides, action confirmations and
failures remain full. Nested error markers also prevent compaction. If a valid
summary cannot fit, the original remains. Matching uses unique invocation IDs
and the saved conversation's user turns, so retrieval reordering cannot make a
current result look old. IDs, arguments, ordering, reasoning and signatures are
preserved. Changed messages and their nested invocation arrays are cloned to
avoid ST's shallow-copy aliasing into the archive.

The retrieval patch additionally excludes entire tool-invocation messages from
vector indexing and retrieval candidates, as well as search queries. Otherwise
Vector Storage could put the full old payload back into the prompt or separate a
tool response from its assistant call. Ordinary conversation and lore retrieval
remain enabled. Existing excluded hashes are removed by ST's normal index sync;
no conversation is deleted.

## Controlled measurements on ARCHIE

Ryzen AI Max+ 395 / Radeon 8060S, 128 GB physical RAM, Windows 11; Ollama 0.40.13,
native ROCm with all model layers offloaded. The CPU-visible Windows memory is
about 32 GB, not the machine's physical total. Nemotron is
`nemotron-3.5-lightning:30b`, Q4_K_M, digest
`e6905fffe2e2d9bbe325a9a2d2e074e3bd32683721bc6dfba409dede2f17f4dc`.

The frozen HOLT history contained 119 messages. Four historical results changed:
**277,279 → 80,013 characters** of tool-result text (71.1% reduction).

| Warm weights, fresh prompt prefix | Original history | Compacted history |
|---|---:|---:|
| Prompt tokens, first paired run | 101,126 | 35,208 |
| Prompt processing | 141.834 s | 41.212 s |
| First visible token | 142.416 s | 41.556 s |
| Total response | 142.775 s | 41.874 s |
| Model load reported separately | 0.010 s | 0.005 s |
| Reverse-order repeat, first token | 142.644 s | 40.704 s |

This is about **65.2% fewer prompt tokens** and **71% less fresh-prefix wait**.
The script warms the weights before measurement, changes an equal-length nonce
at the beginning of each paired prompt to prevent a prefix-cache advantage, and
uses the same model, 131,072 context, thinking off, seed, temperature and final
synthetic status. A reverse-order repeat gives similar results. There is no tool
executor, so no game controls can be sent. This compares compaction, **not Qwen
against Nemotron**, and excludes ST lore/retrieval, tool schemas, MCP and speech.

All paired answers were manually checked and agreed: “We are docked at Galileo
in the Sol system with 37.5 tonnes of fuel aboard.” That status was a synthetic
fixture, not actual game telemetry. This is a narrow current-fact retention test,
not a general model quality benchmark.

A subsequent compacted-history follow-up, with the same prompt prefix cached,
answered “37.5” in **0.279 s to first token / 0.373 s total**, including 0.142 s
prompt processing. `/api/ps` showed Nemotron resident before and after, with
25,715,298,794 bytes on GPU, equal to its reported loaded size. This demonstrates
prefix reuse separately from keeping weights loaded; it is not an ST latency
guarantee. Changing early lore, vector insertion or historical summaries can
invalidate part of that cache.

The raw-history follow-up in that final run is **excluded from the warm-cache
comparison**: the residency snapshot became empty after the preceding request,
and the follow-up reported 45.321 s model load and 188.070 s to first token.
The cause of that unload is not established. It does not invalidate the earlier
paired fresh-prefix measurements, whose loads were 0.010 s or less. Qwen was
explicitly unloaded for these tests, as authorized; its absence from GPU memory
was expected.

## Installation and operation

Already installed on ARCHIE. From the repository root, a fresh ST 1.19.0 install
can be configured with:

```powershell
$stRoot = "$env:USERPROFILE\SillyTavern"
$target = "$stRoot\public\scripts\extensions\third-party\holt-context-compaction"
New-Item -ItemType Directory -Force -Path $target
Copy-Item integrations/sillytavern/context-compaction/* $target -Force
python scripts/patch-st-retrieval.py --st-dir $stRoot
python scripts/patch-st-retrieval.py --st-dir $stRoot --apply
# Restart ST if the server-side embedding patch changed, then refresh the browser.
```

The patch checks the version and all expected source fragments before writing,
backs up changed files with hashes, preserves newlines and is idempotent. A
future ST update may overwrite it; preview again before applying. This extension
and the local vectors patch are custom integration code that must be maintained
across ST upgrades.

In **Extensions → HOLT context control**, toggle compaction or change the recent
turn count/result limit. Its status shows the last actual generation's character
reduction and available tool count. No new chat is required; existing history
benefits on the next generation. The newest protected results can still be large.

Choose **API Connections → HOLT Nemotron MCP** to try Nemotron. Its separate
response preset uses local `http://localhost:11434/v1`, function calling on, and
Additional Parameters:

```yaml
reasoning_effort: "none"
```

This exact compatibility setting was checked against the installed local API:
`none` returned “Ready.” with zero reasoning characters. Despite the model's
advertised `false`/`true`/`medium` values, the string `false` produced a reasoning
trace and was replaced. The isolated benchmark uses native API `think:false`.
The existing four-hour Ollama keep-alive remains. Pocket TTS and the saved X56
button 5 PTT binding are independent of the conversational model.

To roll back prompt compaction, uncheck its toggle. To remove it, disable the
extension and refresh. To roll back the retrieval changes, stop ST and restore
only the corresponding files from `.local/holt/results/st-retrieval-*`, then
restart. The ARCHIE upgrade backup is
`st-retrieval-20261102T172129292743Z`; it contains only files changed in that run.

## Validation and independent review

The Node integration suite passed 33 checks, including ten compaction cases;
the Python retrieval patch suite passed three checks. Coverage includes archive
immutability, protocol fields, recent turns, mixed batches, nested failures,
unsupported schemas, malformed data, duplicate IDs, reordered input, HOLT scope,
idempotence and vector retrieval guards. Benchmark and patch scripts compile.

An independent read-only agent reviewed the implementation and reran those
checks. Review findings about unsupported fleet schemas, deep error markers,
malformed receipts, shallow-copy aliasing and vector reinjection were addressed.
The final review found no core implementation blockers. Private chats and full
benchmark inputs remain ignored under `.local/holt/results/`; only sanitized
measurements are published in `docs/benchmarks/st-compaction-2026-11-02.json`.

Live ST validation in the existing long chat compacted four results and reported
**35 available tools**. Nemotron returned the exact requested “Systems online,
Captain.” A second native generation with the corrected no-thinking setting
returned “Ready when you are, Captain.” without a reasoning trace. Neither test
requested tools or game controls. All **43 original tool-result batches**, including
their stored display text and complete invocation metadata, compared equal after
the tests. The new Nemotron profile and preset survived browser reload; the final
reasoning setting was separately saved to that preset. The saved X56 button 5 PTT
was rearmed after the reload. No MCP server restart is needed for this client change.
The final native ST generation took **57.004 seconds** end to end in this long
chat; its prompt included the live integration's extra content, so it is not
interchangeable with the 41-second isolated replay. The first live check also
reconfigured/reloaded the runner's context and is not a warm benchmark.

## Windows versus the Strix Halo vLLM setup

The [kyuz0 setup](https://kyuz0.github.io/amd-strix-halo-vllm-toolboxes/) uses
native Linux, GPU device access, tuned memory handling and gfx1151-specific vLLM
support. Its published peak numbers aggregate concurrent requests; they are not
the speed of one spoken HOLT reply. Current images and historical benchmark
images also differ, so a matching speed claim requires a matching workload.

ARCHIE is already using ROCm, full GPU offload and automatic Flash Attention.
The largest demonstrated Windows improvement here is reducing actual prompt
content. More keep-alive time does not avoid processing a changed 100k-token
prefix. Smaller context limits alone do not shorten a prompt; quantized KV cache
saves cache memory rather than quantizing model weights. Changing those or
comparing Vulkan against HIP needs a separate controlled experiment.

Qwen3.6-35B-A3B is also a different model from the installed Qwen3-Coder-Next Q8.
Testing that smaller quantized model on Windows would be a reasonable next
experiment before an OS migration; it was not downloaded as part of this task.

Sources: [Ollama FAQ and cache controls](https://docs.ollama.com/faq),
[GPU backends](https://docs.ollama.com/gpu),
[OpenAI compatibility](https://docs.ollama.com/api/openai-compatibility),
[ST extension interceptors](https://docs.sillytavern.app/for-contributors/writing-extensions/),
[ST vectorization](https://docs.sillytavern.app/extensions/chat-vectorization/).

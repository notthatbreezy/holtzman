# Context recovery (2026-10-27)

The repeated “Tool results exceeded context budget” failure came from the client,
not an OpenAI quota error. The client incorrectly imposed a 32,768-token working budget on GPT-6 Luna,
whose verified context window is 1,050,000 tokens. Model limits are now refreshed
per turn, including ST model selection. Verified Luna/Sol profiles use that actual
window; their optional compaction threshold is 131,072 estimated tokens. A failed
optional summary no longer aborts a request which still fits the model. Local
models retain their configured context size. Other hosted profiles retain the
existing conservative limit until their metadata is verified.

The window specification is from the [official GPT-6 Luna model page](https://developers.openai.com/api/docs/models/gpt-6-luna).
A real failure at an estimated 50,228 tokens was a client-side false limit, not a
Luna context exhaustion. The first compaction-only fix did not solve that root
cause and was superseded by the model-specific budget and graceful fallback.

## Causes and changes

1. The old estimate serialized `_response_items` as text, including encrypted
   reasoning, alongside mirrored tool calls and replies. These are two
   representations of the same response. The estimate now counts visible content
   once, plus actual reasoning-token usage. Legacy responses without usage get a
   bounded allowance. The provider's original opaque response items are preserved
   unchanged for subsequent API requests.
2. Desktop history had compaction but external SillyTavern history bypassed it.
   Both now receive request-local budget recovery. Older turns can be summarized,
   keeping the system/personality, current question and three recent exchanges.
   SillyTavern's saved transcript is never rewritten. Identical history summaries
   are cached within the running engine.
3. Tool outputs were sliced at 10,000 characters, potentially breaking JSON and
   deleting the end of a market list. JSON is now minified without cutting rows.
   Trade-route replies carry the full existing itinerary summary instead of
   duplicating it alongside raw provider data.
4. Identical commodity/reference searches are cached for at most 120 seconds,
   within one request only. A cache hit does not renew market timestamps. Status,
   screenshots, live journal readings, knowledge writes and game actions are never
   replayed from this cache. Playbook reads are also excluded to avoid stale
   revisions after updates.
5. When genuinely close to the budget, older completed read-only results can be
   condensed into a labeled checkpoint. Source queries and call IDs remain
   attached. The latest two tool results, all action/write/error receipts, and all
   provider reasoning items remain intact. Compaction never invokes a game tool.
   Older images may be omitted while the latest observation is retained.

Summary requests have bounded inputs and no tools. They preserve exact names,
prices, quantities, units, timestamps, constraints and source-query associations;
summaries remain historical, potentially partial evidence. They are not permission
to operate controls or proof a market is exhaustive. Full original tool receipts
remain in the existing local trace. A failed compaction stops the request without
replaying controls; it does not silently discard action outcomes or erase history.

This uses local, provider-neutral working-context summaries. It does **not** edit
encrypted OpenAI compaction items or migrate transport to the standalone Responses
compaction endpoint. See the [official OpenAI compaction guide](https://developers.openai.com/api/docs/guides/compaction)
for that separate mechanism.

## Diagnostics and validation

`context_estimate` trace events record per-round estimated tokens and budget.
`context_compaction` records before/after sizes; `tool_cache_hit` identifies reused
request-local reads. These events contain no API credential.

Regression tests cover large opaque reasoning, complete JSON market lists, external
history compaction, preserved tool-call IDs/action receipts, and full desktop/ST
loops with repeated market calls. `python -m client.check_context` is an opt-in paid
GPT-6 Luna smoke check comparing four simulated commodities, including a long ST
history. All game and market tools in that check are simulated, and game controls
are disabled. Its results are written under `.local/holt/results/`.

Deploy by restarting the local HOLT gateway. No MCP server or SKIPPY update is
required for this client-only change. Existing audio choices and saved chats remain.


### Recorded result

All 72 Python regressions passed locally. The two paid GPT-6 Luna checks passed
with simulated markets and no game controls. Both compared all four commodities
and correctly chose Coffee, with a full-load reported profit of 3,504,000 credits.
The synthetic long ST history was reduced from an estimated 82,325 to 25,177
working tokens and the comparison completed. These numbers describe a synthetic
regression, not live prices or a guarantee of model quality on every request.
Detailed outputs and usage: [hosted-check.json](evaluations/2026-10-27-context/hosted-check.json).

The live rollout also exposed a summary that exhausted its output allowance. The
summarizer now retries only its own bounded, tool-free request with a larger
allowance, never the game operation. Summaries are scoped to the current question;
the current request is explicit in the operational contract so unanswered earlier
trade questions are not treated as queued work during a microphone check.


Follow-up checks caught an unrelated batch limit: four-commodity research may
legitimately request more than eight reads at once. Read batches now allow up to
24 proposals, with a 64-proposal per-request ceiling. The existing single-control
per observation and bounded action permit remain enforced. Three follow-up hosted
checks passed across separate runs: desktop trading, long ST trading, and an
explicit microphone check after unresolved trade questions. The last initially
made unnecessary market calls; a targeted current-request rule removed those calls.
See [hosted-followup.json](evaluations/2026-10-27-context/hosted-followup.json), including
that failed intermediate check rather than concealing it.

During live rollout, an independent MCP ConnectTimeout also occurred. SKIPPY
advertised both link-local IPv6 and Ethernet IPv4 addresses. A direct status check
succeeded with an IPv4 socket to the original skippy hostname. The ship transport
now prefers IPv4 for skippy/skippy.local only. It keeps the same URL, Host validation
and bearer credential; other MCP hostnames retain their normal address selection.
Nested transport exceptions now report a credential-safe underlying error type.


Final combined synthetic checks all passed: desktop four-commodity comparison,
long-history ST comparison, ST audibility check with no market lookups, and the
observed docking procedure. The ST comparison completed at 36,599 actual provider
input tokens with no forced compaction. Docking used FocusLeftPanel,
CycleNextPanel, UI_Right and UI_Select, and verified a simulated pad assignment.
See [final hosted results](evaluations/2026-10-27-context/hosted-final.json) and
[simulated docking](evaluations/2026-10-27-context/docking-simulated.json).

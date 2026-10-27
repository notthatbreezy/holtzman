# Context recovery (2026-10-27)

The repeated “Tool results exceeded context budget” failure came from the client,
not an OpenAI quota error. Hosted requests have a conservative local 32,768-token
working budget. It includes instructions, schemas, conversation, live instruments,
tool results and reasoning. This budget remains in place.

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

All 66 Python regressions passed locally. The two paid GPT-6 Luna checks passed
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

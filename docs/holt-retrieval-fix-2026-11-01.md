# Recurring ST console notices — 2026-11-01

The recurring **Vector Storage: Generation interceptor aborted; check browser
console** notice was caused by oversized local embedding requests. This could
interrupt chat/lore retrieval while the main model still generated a reply.
It was a retrieval integration failure, not evidence that Luna refused a tool.

## Evidence

ST server logs recorded ONNX `OrtRun` error code 6 with input lengths of 120,354,
7,627 and 6,539 tokens. The first case was the screenshot-as-text problem fixed
earlier. Later failures began with HTML-formatted tool receipts, including fleet
and guide results. ST's `getQueryText` took the two latest messages without
excluding tool-invocation messages.

The configured embedding model is `Cohee/jina-embeddings-v2-base-en`. Its local
model config declares 8,192 positions, but its exported tokenizer advertises
2,147,483,648 tokens. The installed feature-extraction pipeline enables
truncation but relies on that tokenizer limit. Large inputs therefore still reach
the local ONNX runtime; their attention-memory demands can fail before HOLT's
conversational model receives a request.

## Repair

- Retrieval searches use conversation text and exclude tool-invocation messages.
  Full tool receipts remain available to the conversational model as evidence.
- Local feature-extraction tokenization is capped at **512 tokens**, preserving
  any smaller existing tokenizer limit. This bounds attention-memory use for
  embedding queries and indexed chunks. It does not truncate HOLT's chat input
  or change Luna's context/response limits.
- Long embedding inputs now use their first 512 tokens. This is a deliberate
  resource/recall tradeoff; keep long indexed documents split into useful chunks.
  The embedding model and vector dimensions remain unchanged. No lorebooks,
  chat histories, summaries or vector stores were deleted or rebuilt.

`scripts/patch-st-retrieval.py` applies both changes to ST 1.19.0, after checking
all expected source fragments. It previews by default, backs up original files
and hashes under `.local/holt/results/`, preserves newlines, and is idempotent.
Unknown versions/source layouts stop installation before either file changes.

```powershell
python scripts/patch-st-retrieval.py
python scripts/patch-st-retrieval.py --apply
# Restart SillyTavern, refresh its browser tab, reopen the existing HOLT chat.
node scripts/check-st-retrieval.cjs
python -m unittest discover -s tests -p test_st_retrieval_patch.py
```

Installation and restart were completed on ARCHIE. The current chat was reopened.
A future ST update may overwrite these local patches; rerun the preview first.
To undo, stop ST, restore the two files from the recorded backup, and restart.

## Verification and remaining distinctions

Two deployment regression tests passed. The installed-function harness confirmed
that tool payloads are excluded from search queries without changing chat records,
and that the embedding guard preserves smaller limits and vector output handling.

Through the restarted ST API, a short embedding query returned HTTP 200 in 0.39
seconds. An oversized query of 8,000 repeated words returned HTTP 200 in 1.53
seconds. These used empty collection lists to test embedding execution without
changing any index. A full browser test then asked HOLT for a read-only fleet
count. Its fleet receipt was **16,267 bytes**, and HOLT replied **“Four ships are
recorded, Captain.”** No browser console errors occurred in that test.

The inspected combat exchange had two other failures: an expired MCP session,
then a lookup for a nonexistent `combat` guide. `read_guide` can list available
guides when called without a name; `interdiction` is an available related guide.
After the ST restart/browser reconnection, `status` and `read_guide(interdiction)`
both returned HTTP 200 and successful MCP receipts. This confirms present access,
not a permanent repair of session expiry.

The MCP plugin also continues to log `Failed to parse SSE data: Unexpected end
of JSON input` server-side while successful calls complete. That transport-parser
warning is separate from the repaired vector-retrieval notices and is not claimed
fixed here. Private logs and browser proof remain in `.local/holt/results/`.

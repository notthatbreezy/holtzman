# HOLT screenshot repair — 2026-11-01

HOLT can now request a screenshot through native SillyTavern MCP and inspect it
with the **HOLT Luna MCP** connection. No SKIPPY server change or restart was
required. The current chat remains usable.

## Causes and repairs

1. **`Invalid URL` before capture.** The ST MCP server plugin's bundled
   `jsonschema@1.5.0` could not resolve local `$ref` pointers in anonymous schemas.
   Screenshot's explicit `format` and `rect` parameters follow these pointers.
   Omitting `format` happened to avoid the bug. Giving the validator an absolute
   local base fixes resolution without modifying the server schema or disabling
   argument validation. The base `https://mcp.invalid/schema/` is a resolution
   identifier, not a network service.
2. **Encoded images treated as chat text.** Native ST MCP JSON-stringifies tool
   results. This put hundreds of thousands of image characters into tool history.
   Before dispatch, ST's token budget dropped the relevant chat; vector lore
   retrieval also tried to embed the image text and failed with an ONNX error.
   Merely converting the image at the final request event was too late. A live
   intermediate test produced an unrelated reply about manifests, exposing this
   problem despite the separate transport/vision smoke test passing.
3. **Missing multimodal delivery.** JSON containing base64 is not image input.
   The compatibility extension now stores JPEG/PNG captures in ST's user image
   store *before* token accounting, summarization and vector retrieval. Tool
   history contains small image references. At request dispatch it reloads the
   two most recent captures and attaches genuine user-role `image_url` parts,
   after all pending tool receipts. This follows the
   [official OpenAI image-input format](https://developers.openai.com/api/docs/guides/images-vision).

The adapter preserves text metadata and tool-call IDs. It identifies only
`mcp_holtzman_` game-window tools: `screenshot`, `press_binding`,
`nav_target_start`, `nav_target_move` and `nav_target_confirm`. Image dispatch
currently targets the tested `gpt-6-luna` model only; this does not give the local
Qwen model vision. Game controls were not exercised by these tests.

Older captures retain their text receipts, with an explicit omission note in the
model request. Captures remain in `data/default-user/user/images/HOLT-MCP/` for
chat history; the two-image limit bounds prompt input, not disk retention.
Existing inline captures are migrated when opening or generating in that chat.
Conversation prose, persona, lore and memory summaries are preserved.

## Installation and recovery

The installed ST server plugin is `SillyTavern-MCP-Server` version 1.0.0. Its
TypeScript and bundled production JavaScript both need the validator patch.
From the Holtzman checkout:

```powershell
# Preview, then install. Defaults to $env:USERPROFILE\SillyTavern.
python scripts/patch-st-mcp-validation.py
python scripts/patch-st-mcp-validation.py --apply

Copy-Item integrations/sillytavern/tool-schema-compat/index.js `
  "$env:USERPROFILE/SillyTavern/public/scripts/extensions/third-party/holt-tool-schema-compat/index.js"
Copy-Item integrations/sillytavern/tool-schema-compat/manifest.json `
  "$env:USERPROFILE/SillyTavern/public/scripts/extensions/third-party/holt-tool-schema-compat/manifest.json"
```

The existing HOLT compatibility extension is version 1.1.0. Restart SillyTavern
after the server patch, refresh its browser tab, and reopen HOLT's chat. Both
were done during installation. Other HOLT services need no restart.

The patcher validates all known call sites before writing, preserves line
endings, backs up originals and hashes under `.local/holt/results/`, and is
idempotent. An unrecognized plugin version or bundle stops installation. A ST
plugin update/rebuild may overwrite the local patch; rerun its preview first.
Do not bypass validation to get past this error.

To undo, stop ST, restore `src/McpClient.ts` and `dist/index.js` from the recorded
backup, restore the earlier compatibility extension, and restart ST. Restoring
the earlier extension removes image interpretation; retained capture files can
stay on disk. The current chat was backed up before its inline image migration.

## Verification

- Reproduced `Invalid URL` using the exact installed validator dependency.
- Verified JPEG, PNG, null/default format and rectangle references after repair;
  unsupported formats and invalid rectangles still fail validation.
- Through the running ST bridge, explicit JPEG and PNG returned HTTP 200 with
  image blocks in approximately 0.14 and 0.16 seconds. `format: gif` remained
  rejected. The plugin's existing HTTP error mapping reports that rejection as
  HTTP 500, but it is a validation failure and no capture is requested.
- A smoke request through ST's backend and saved credentials sent a real image
  part to Luna. It correctly read “Select Mission Board”, “Mission Board” and
  “Passenger Lounge” in 1.66 seconds. This excludes capture and speech latency.
- A second, complete **browser chat test** made a model-selected JPEG screenshot
  call through native MCP and produced:

  > The station services screen is open at Salve De Terra, Captain, showing the
  > Mission Board and Passenger Lounge choices.

  The receipt was **781 bytes**, with a separately stored image. The full chat
  was approximately 48 KB, versus approximately 460 KB after the initial inline
  screenshot test. No browser errors appeared during the successful follow-up.
- Sixteen Node checks passed, covering voice compatibility, schema handling,
  image conversion, saved-image reload, early compaction, idempotence, parallel
  tool ordering, history bounds and unrelated-tool/model isolation. Two Python
  deployment checks passed, including fail-closed unknown-bundle handling.

Reproduce the checks with:

```powershell
npm install --prefix .local/holt/schema-validator jsonschema@1.5.0 --ignore-scripts --no-audit --no-fund
node --test tests/st_voice.test.cjs tests/st_tool_schema.test.cjs tests/st_mcp_images.test.cjs tests/st_mcp_validator.test.cjs
python -m unittest discover -s tests -p test_st_mcp_patch.py
```

Private evidence, including bridge results, the image-input smoke response,
pre-migration chat backup and browser proof, remains under `.local/holt/results/`.
Images, chats and credentials are not committed. These checks establish screenshot
capture and interpretation; they do not validate docking or other game controls.

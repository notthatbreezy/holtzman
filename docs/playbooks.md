# Persistent agent playbooks

Playbooks are operational memory shared by MCP clients connected to the same server.
They survive new chats, SillyTavern compaction and server restarts. Authored field
guides remain read-only; chat summaries and HOLT's fictional biography stay separate.

## Using them

Tell HOLT: “Remember this docking workflow: inspect the cockpit and current distance
before using Contacts; verify the docking grant before saying it worked.” HOLT can
save a draft or revise an existing record. It must report success only after the
server returns a save receipt. Later, ask about or request a similar task: the
desktop and SillyTavern gateway search a compact index automatically once before
each normal model turn, when the server advertises the feature. The agent reads
the full record before following it. Exact named route orders still use the
existing deterministic server recipe directly.

This is bounded lexical retrieval over names, titles, tags, summaries and purpose,
with common stopwords ignored and short prefixes for inflections. It is not a
semantic embedding index; an agent can search again using alternative terms or list
records. Discovery failures do not block conversation. A matching record is a
candidate, not proof it applies. There is no automatic extraction of every chat
message into a playbook.

## MCP tools

| Tool | Arguments | Result |
| --- | --- | --- |
| `search_playbooks` | Optional `query`, `limit` (default 5, max 20), `offset`, `include_archived` | Compact latest-revision entries and total matches; empty query lists records |
| `read_playbook` | `name`, optional `revision` | Full latest record, or an immutable numbered historical revision |
| `save_playbook` | `name`, `expected_revision`, `procedure`, `change_note` | Saved full record with server timestamp and new revision |

Create with `expected_revision: 0`. Before updating, read the latest record and use
its revision; submit the **complete** revised procedure. A stale revision conflicts
instead of overwriting another client. Previous revisions are numbered consecutively
from 1 and remain readable. Archive obsolete records with `archived: true`; ordinary
search excludes them. Read them by name or search with `include_archived: true`.
Restore by saving a new revision with `archived: false`.

Example save arguments (a draft, not a claim of a successful live test):

```json
{
  "name": "request-docking",
  "expected_revision": 0,
  "change_note": "Captain requested a reusable docking workflow",
  "procedure": {
    "title": "Request docking from observed Contacts",
    "summary": "Inspect current cockpit state before requesting docking.",
    "when_to_use": "The Captain asks for an immediate docking request.",
    "tags": ["docking", "station", "contacts"],
    "required_tools": ["screenshot", "press_binding", "recent_events"],
    "steps": [
      "Inspect current cockpit, target and distance; verify docking range.",
      "Open Contacts and select the observed station and Request Docking, observing each menu step."
    ],
    "success_checks": ["Observe a docking grant, pad and timer, or fresh DockingGranted event."],
    "stop_conditions": ["Stop if the target, range or highlighted menu option cannot be verified, or a control fails."],
    "validation": "draft",
    "evidence": null,
    "archived": false
  }
}
```

`reported_working` requires an evidence string identifying the observation or user
report and preferably its time and conditions. This label records supplied evidence;
the server does not independently verify gameplay. An agent must not fabricate
evidence. Saved workflows never grant permissions, execute scripts, alter control
recipes or establish current ship state. Verify current conditions, tools and
authority before acting. Knowledge saves work with game controls off; their MCP
annotation correctly marks them as writes.

## Storage and deployment

Default storage is `playbooks/` beside the server's `holtzman.toml`. Override it with
top-level `playbooks_dir = "my-playbooks"`; relative paths resolve beside the config.
Absolute paths are supported. Embedded servers without a config path must explicitly
set an absolute storage directory to save. Reading/listing an absent store does not
create files. No new service or database is required.

Records live at `<directory>/<slug>/<20-digit-revision>.json`, with schema version 1.
Each record is at most 64 KiB with bounded fields and arrays. Lowercase slugs cannot
contain paths or reserved Windows device names. Linked child directories and linked
revision files are rejected. A synced temporary file is published with an atomic,
create-only hard link: competing writers cannot replace a committed revision or
expose a partially written document. Use a local filesystem supporting hard links
(NTFS on the Windows server); publication failures are returned as tool errors.
Corrupt records produce errors rather than silently reverting to older knowledge.

Back up the entire directory. Agent knowledge is runtime data and the default root
directory is git-ignored. Treat it as trusted local application data with the same
account/filesystem protection as other server files. All holders of the MCP bearer
token can read and write the shared store; there is no per-commander isolation or
per-agent identity. Distinguish commander-specific workflows in names and purpose
when needed. The feature does not send playbooks to a new external service, but
recalled content enters the currently selected reasoning model's context.

Deploy the updated server on SKIPPY, restart its MCP service, then restart HOLT from
its panel so discovery refreshes. Restart the local gateway process to load changed
Python code when updating it. No speech model, audio routing or game recipe changes
are required. Existing saved playbooks are retained through updates.

## Validation

Rust tests cover persistence across reopening, historical reads, stale/concurrent
writes, archive/restore, lexical ranking and pagination, size/name/evidence checks,
corruption and linked-directory escapes. HTTP tests exercise authenticated discovery
and actual tool calls. Python tests check automatic discovery and ensure a knowledge
write with controls off does not trigger game preflight, screenshot or keypresses.

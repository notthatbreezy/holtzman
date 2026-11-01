# HOLT character and Captain persona

Installed through the local SillyTavern API on **2026-10-31**.

In SillyTavern, the **character** is the AI's role. The **persona** is the user's
role. HOLT is therefore the character, and Commander Brown is the human Captain.
Putting HOLT's identity in both fields would tell the model that both participants
are HOLT.

## What is installed

- **HOLT**, version `1.4-standalone`, in the existing `HOLT.png` character slot.
  Its image and chat history are retained. The description, personality, scenario,
  opening, examples and author notes were populated from the established HOLT canon
  and conversational preferences.
- **HOLT canon**, the existing repository's eight authored personal-history entries,
  as HOLT's primary character lorebook. This describes the fictional manufacturer,
  Leesti assembly, prior freighter, recovered core and small personal preferences.
- **Elite Dangerous**, the 84-entry researched universe book, as an additional
  character lorebook. The association is stored in
  `world_info_settings.world_info.charLore`, keyed by the character filename stem.
- A separate **Commander Brown** persona, titled **HOLT's Captain — Elite Dangerous**,
  associated with `HOLT.png`. It reuses a copy of the existing persona artwork and
  describes the Captain's relationship with HOLT and conversational preferences.
  The Captain's user-authored background is a veteran of the Thargoid War now
  seeking a quiet living through trading, exploration and passenger transport.
  No military rank, unit, battles, decorations, service dates, injuries or faction
  allegiance are invented. This background does not establish real gameplay events.

The existing general persona, global persona default, global lore selections and
unrelated characters are preserved. The character installer does not change the
model connection, inference service, speech device, game control setting or MCP
credential. The separate 2026-11-01 response-limit and prompt-compatibility settings
adjustments are documented below. A character card cannot provide capabilities
by itself.

## Use it

Close all SillyTavern browser tabs before an API update; an open editor can autosave
its stale blank card or persona over the update. Reopen SillyTavern after setup,
then select **HOLT**. The persona connected to that character
will be selected by SillyTavern when the character/chat loads. If an old chat has a
different persona explicitly locked to it, select **Commander Brown** with the
**HOLT's Captain — Elite Dangerous** subtitle in Persona Management and update that
chat's persona lock. The installer does not rewrite old chat metadata or messages.

The populated card applies to future replies in an existing chat. Its new opening
and alternate greetings apply when starting a new chat; old openings are retained.
The card favours one to three useful sentences, measured dry humour, more urgency
when needed, and less compulsory last-word chatter. Requested research and game
tasks should use the tools actually connected to the active session, with observed
results determining what HOLT can truthfully report.

Version 1.2 adds a short **Character's Note**, inserted as a system instruction at
depth 0. It tells HOLT to turn tool results into one or two natural spoken sentences,
omit unrequested fields and closing offers, and qualify old records as "last recorded"
rather than "currently". This supplements the personality instructions close to the
active request. It is character configuration, not an old-chat summary.

Version 1.3 (2026-11-01) strengthens spoken prose: answer first, interpret the
few relevant readings, and connect them to the Captain's decision. Routine
answers omit headings, bullet lists, raw field names, internal entry IDs, emoji
and closing menus. Requested briefings/checklists can still be structured.
Additional examples cover fuel, natural acknowledgements, confirmed log writes
and in-character banter. All example readings/actions are hypothetical.

Version 1.4 adds an operational main-prompt override: fictional personality does
not authorise fictional game facts. Current facts need relevant evidence, nearest
claims need a verified comparison, and action completion needs a real receipt.
Earlier assistant assertions do not become evidence through repetition. This is
grounding guidance, not an extra approval gate or a binding restriction.
The [Luna trial and factual-grounding report](holt-luna-trial-2026-11-01.md)
records the observed failures, fresh/contaminated-context checks, the selected
hosted preset and the recommendation to begin a fresh HOLT conversation.

### Local Qwen connection compatibility and response allowance

ST's HOLT local MCP preset and active settings now use **2,048 response tokens**,
up from 300. This is a ceiling, not a desired reply length; the card governs
ordinary brevity. The saved chat contained mid-sentence endings and Ollama logged
several generations ending at exactly 300 tokens before the change.

A separate compatibility issue explained why card instructions had little effect:
the local `qwen3-coder` renderer keeps the first system message and filters out
the others. [Ollama's renderer source](https://github.com/ollama/ollama/blob/main/model/renderers/qwen3coder.go)
implements this explicitly. A local two-system-message code-word check failed,
while the combined-message equivalent succeeded. The initial synthetic conversation
checks had identical prompt counts despite different later system content, also
consistent with the missing instructions.

For this local connection, **API Connections → Prompt Post-Processing → Semi-strict
(alternating roles; with tools)** is enabled and saved in the same preset. ST
merges consecutive system messages, keeps the first system role, converts later
system messages into user messages, and preserves tool definitions/call metadata.
Thus initial character/persona/lore instructions reach Qwen together, and later
notes reach it as user-role context rather than disappearing. Their role is changed;
this is not equivalent to a backend supporting multiple system messages natively.
The mode with **no tools** must not be used for HOLT's MCP connection.

To reproduce, set Max Response Length to 2048 in AI Response Configuration,
select the tool-preserving compatibility option in API Connections, then update
the **HOLT local MCP** preset with its save button. The card installer deliberately
does not replace connection presets. Other model connections should use their
own appropriate prompt format rather than inheriting a Qwen workaround blindly.

Both limits/compatibility values were read back from active settings and the saved
preset; the character fields and legacy editor mirrors matched the portable card.
Changes were applied through the open ST editors so their in-memory state was
updated too. No old chat messages or the Commander persona were rewritten.

Four isolated local text checks through ST's backend exercised fuel readings,
banter, a confirmed log entry and a synthetic function-call round trip. Replies
ended with `finish_reason: stop`, and the function call remained usable. Examples:
“An outcome I remain strongly in favour of, Captain.” and “Yes, Captain—the
pirate-hunt summary is logged.” The tool-result reply used a connected sentence
about having 32 tonnes for an eight-tonne jump. These are synthetic fixtures,
not observations of the real ship. No actual MCP call, game control or TTS was
triggered by the checks.

[Raw checks](evaluations/2026-11-01-conversation.json) preserve the results and
limitations. The separate readiness case still embellished drive/navigation
state not supplied by its fixture, so this demonstrates better delivery and
instruction delivery, not complete factual reliability. Existing long chat history
may also continue to influence style; these checks used isolated conversations.

Standalone ST does not automatically support HOLT's gateway-specific delivery JSON
or silence marker. The card therefore follows a client's supported conventions
without printing invented transport markers into normal conversation. Talkativeness
is also lowered for ST group chats; ordinary reply length comes from the character
instructions, not that group-chat setting.

## Reproduce or edit

Portable character card: `integrations/sillytavern/HOLT.standalone.character.json`.
Portable persona definition: `integrations/sillytavern/HOLT-Captain.persona.json`.
Personal canon: `integrations/sillytavern/HOLT.lorebook.json`.
The original gateway trial card remains separately available as `HOLT.character.json`.

```powershell
cd C:\Users\chris\holtzman
.\.local\holt\voice-env\Scripts\python.exe scripts\setup-holt-character.py
.\.local\holt\voice-env\Scripts\python.exe scripts\setup-holt-character.py --install --editor-closed
```

Installation requires the existing HOLT character and the Elite Dangerous book.
It reads and saves through the ordinary cookie/CSRF-protected APIs. It backs up the
previous character and targeted persona/lore settings under
`.local/holt/lore-research/st-backups`, excluding provider settings and credentials.
The character merge endpoint performs ST's card validation. The installer checks
every authored card field and the legacy mirrors used by editors, the entire settings
read-back in memory, the avatar's existence and unchanged unrelated character records.
It removes competing HOLT associations from older personas, selects the populated
Captain persona and synchronises ST's active persona fields. The global default
persona remains unchanged. No hosted model call or game
action is used for verification. Differing pre-existing personal lore is preserved
and reported for reconciliation rather than silently replaced.

Rerunning `--install` deliberately reapplies the portable card and HOLT-specific
persona definition, so export any later UI edits before doing so. A browser holding
stale settings must be closed before an API update and reopened afterward. The
`--editor-closed` flag makes that requirement explicit on future installations.

The restoration on 2026-10-31 was also checked through a freshly opened ST page:
HOLT's description and greeting were visibly populated, the selected Captain
persona visibly contained the war-veteran/civilian background, and an API read-back
after page load confirmed the character fields, active persona cache, sole HOLT
persona connection and both lorebooks remained intact.

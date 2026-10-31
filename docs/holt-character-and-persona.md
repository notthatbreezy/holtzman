# HOLT character and Captain persona

Installed through the local SillyTavern API on **2026-10-31**.

In SillyTavern, the **character** is the AI's role. The **persona** is the user's
role. HOLT is therefore the character, and Commander Brown is the human Captain.
Putting HOLT's identity in both fields would tell the model that both participants
are HOLT.

## What is installed

- **HOLT**, version `1.2-standalone`, in the existing `HOLT.png` character slot.
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
unrelated characters are preserved. No model connection, inference service, speech
device, game control setting or MCP credential is changed. A character card cannot
provide those capabilities by itself.

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
The card favours one or two useful sentences, measured dry humour, more urgency
when needed, and less compulsory last-word chatter. Requested research and game
tasks should use the tools actually connected to the active session, with observed
results determining what HOLT can truthfully report.

Version 1.2 adds a short **Character's Note**, inserted as a system instruction at
depth 0. It tells HOLT to turn tool results into one or two natural spoken sentences,
omit unrequested fields and closing offers, and qualify old records as "last recorded"
rather than "currently". This supplements the personality instructions close to the
active request. It is character configuration, not an old-chat summary.

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

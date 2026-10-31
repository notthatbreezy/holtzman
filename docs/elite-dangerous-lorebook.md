# Elite Dangerous SillyTavern lorebook

Prepared and researched **2026-10-31**. The book contains **84 focused entries**.
It is an opt-in setting reference usable with any character or model connection;
it does not require the HOLT gateway or access to the game.

## Use it

After installation, refresh SillyTavern and open **World Info / Lorebooks**. Select
**Elite Dangerous** to browse or edit it. Enable that book as global World Info for
an Elite session, or associate it with the intended character/chat through ST's
lorebook controls. A global selection affects other conversations too; character
or chat associations are more appropriate if you also use unrelated settings.

The installer only fills the library book. It does not change global selections,
characters, model connections, chats, personas, lore budgets or HOLT services.
In the instance inspected for this installation, the existing Elite Dangerous
book was empty and the existing character used Eldoria. That association was
preserved.

## Contents and retrieval

- Setting, the Bubble, commanders, ranks and the Codex.
- Federation, Empire, Alliance, independent systems, factions, Powerplay and BGS.
- Human expansion, the Duvals, Marlinism, INRA, Jameson, Aegis and Azimuth.
- The Proteus Wave and the dated Titan campaign, including Cocijo's destruction.
- Thargoids, Guardians, Constructs, archaeology, Ram Tah and Seo Jin-ae.
- Sol, the political capitals, Lave, Shinrarta, Colonia, Jaques, Sagittarius A*,
  Beagle Point, Hutton, the Formidine Rift, Raxxla and the Dark Wheel.
- Shipbuilders, travel technology, starports, carriers, Engineers and careers.
- Remlok, Holo-Me, law, community rescuers, reconstruction and colonisation.

One short entry is constant when the book is enabled. The other 83 entries use
literal aliases, case-insensitive matching, ST's whole-word setting and a two-message
scan depth. Character descriptions and scenarios are not additional trigger inputs.
Both recursion flags prevent the encyclopedia from activating itself through chains
of names inside other entries. No entry ignores ST's token budget. Existing
global budget settings remain authoritative; roughly **3,500 lore tokens** is a
reasonable starting cap to choose in ST, not a setting silently applied by setup.
Large multi-topic questions can still exceed a configured budget.

These are lexical triggers, not semantic RAG. A paraphrase without a configured
alias may miss a topic, and the previous reply can keep a topic relevant for the
next exchange. The actual ST keyword matcher passed 14 offline cases, including
unrelated fantasy, Raxxla, a fuel-scoop question, political comparisons, Guardian
AI, abbreviations, punctuation, ship/task combinations and the two-message limit.
Those cases selected at most four entries, including scope. Approximate sizes for
the sample matches were 166–672 tokens using characters/4, not provider billing.
The checks do not evaluate a model response or the browser's final token-budget
placement. No hosted generation was needed for installation or verification.

## Accuracy and sources

The user ran the included crawler against the public MediaWiki API, obtaining
**88 articles from 89 seed titles**, with article and revision URLs, raw wikitext,
rendered article HTML, extracted text and integrity hashes. The requested title
`The Zurara` did not exist. The book instead uses the archived Formidine Rift
article and [EDAstro's Zurara record](https://edastro.com/gec/view/35).

The principal source is the **Elite Dangerous Wiki**, especially its transcriptions
of in-game descriptions, Codex material and dated GalNet. Selected facts were also
checked through web-indexed wiki pages, the GalNet material hosted by EDSM,
EDAstro, the Anti-Xeno Initiative, Xeno Strike Force and the organisations' own
sites for the Fuel Rats, Hull Seals and Operation Ida. Sources and access type are
recorded per entry under `extensions.holtzman.sources`, with archived revisions
when present. External/index-only sources do not pretend to have archived revision
IDs. The separate source catalog makes these links easier to audit.

This is a curated paraphrase, not a wholesale wiki import. Some pages mix current
and obsolete mechanics, planned features, legacy continuity, opinion and source
transcripts. The book therefore:

- Separates Pilots Federation, Federation, local government and Powerplay.
- Uses the Alliance's **3230** foundation date.
- Treats Raxxla and claims about the legendary Dark Wheel as unresolved.
- Avoids importing Jaques's explicitly legacy-labelled biography as current canon.
- Dates Cocijo's destruction to **18 December 3310**, without declaring the entire
  Thargoid species gone or historic war zones still active.
- Treats the Proteus Wave's aftermath as history rather than freezing HIP 22460's
  old hazards as current conditions.
- Avoids hard-coding present leaders, territories, market quotes, station stock,
  build costs, ship prices, commander unlocks or ownership.
- Marks mechanics as version-sensitive. The archived Interstellar Factors page
  discusses a future legal-system change; the book does not present that planned
  change as already shipped, or promise the old contact will exist forever.
- Keeps HOLT's invented biography separate from Frontier's setting.

The research date is not a guarantee of perfect or exhaustive canon. For current
missions, prices, politics and access, use current instruments or fresh research.
Source links remain metadata rather than being appended to every response or read
aloud. A user can inspect them deliberately in the exported JSON/source catalog.

## Files and repeatable setup

Authored text: `integrations/sillytavern/elite-dangerous-lore.md`.
Portable import: `integrations/sillytavern/Elite-Dangerous.lorebook.json`.
Attribution catalog: `integrations/sillytavern/elite-dangerous-sources.json`.
Downloaded corpus and API backups remain private under `.local/holt/lore-research`.

```powershell
cd C:\Users\chris\holtzman

# Optional: fetch the curated source corpus; completed pages are reused.
.\.local\holt\voice-env\Scripts\python.exe scripts\crawl-elite-wiki.py

# Build the portable book from the authored text.
.\.local\holt\voice-env\Scripts\python.exe scripts\build-elite-lorebook.py

# Verify aliases using the installed ST keyword matcher, without inference.
$loreNode = (Get-Content .local\holt\tavern\installation.json | ConvertFrom-Json).node
& $loreNode scripts\check-elite-lorebook.cjs

# Install through CSRF-protected local API, then read back and compare every field.
.\.local\holt\voice-env\Scripts\python.exe scripts\build-elite-lorebook.py --install
```

The installer uses `/csrf-token`, `/api/worldinfo/list`, `/api/worldinfo/get` and
`/api/worldinfo/edit`. It uses the normal cookie/CSRF flow and only accepts a
loopback HTTP base URL. No MCP token or OpenAI key is required. Existing nonempty
books are protected: review/export changes before explicitly adding `--replace`.
An existing book is backed up before replacement; an identical book needs no write.

The crawler waits at least 1.5 seconds between requests, checks robots.txt, respects
server delays and stops on access blocks. Default mode downloads only the seed
list. A bounded link crawl is available with `--crawl --max-pages 200 --max-depth 1`;
it is not needed for this edition. It does not scrape accounts, comments or private
data and makes no attempt to bypass access controls.

Source articles and game material belong to their respective authors and rights
holders. Attribution is retained, and no long article passages or wiki images are
included in the portable lorebook.

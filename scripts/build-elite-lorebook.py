"""Build the researched Elite Dangerous lorebook; optionally install via local ST API."""

import argparse
import hashlib
import json
import re
from datetime import datetime, timezone
from http.cookiejar import CookieJar
from pathlib import Path
from urllib.parse import quote, urlparse
from urllib.request import HTTPCookieProcessor, Request, build_opener


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "integrations/sillytavern/elite-dangerous-lore.md"
EXPORT = ROOT / "integrations/sillytavern/Elite-Dangerous.lorebook.json"
CATALOG = ROOT / "integrations/sillytavern/elite-dangerous-sources.json"
CORPUS = ROOT / ".local/holt/lore-research/elite-wiki"
BOOK_NAME = "Elite Dangerous"
REVIEW_DATE = "2026-10-31"


def load_entries(source):
    entries = []
    seen = set()
    for block in re.split(r"(?m)^## ", source)[1:]:
        header, rest = block.split("\n", 1)
        slug, title = header.split(" | ", 1)
        metadata, content = rest.split("\n\n", 1)
        fields = dict(line.split(": ", 1) for line in metadata.splitlines())
        content = content.strip()
        keys = [x.strip() for x in fields["Keys"].split(";")]
        sources = [x.strip() for x in fields["Sources"].split(";")]
        if slug in seen or not re.fullmatch(r"[a-z0-9-]+", slug):
            raise ValueError(f"Duplicate/invalid entry identifier: {slug}")
        if not keys or not all(keys) or not sources or not content:
            raise ValueError(f"Incomplete entry: {slug}")
        if len(content.split()) > 180:
            raise ValueError(f"Split long entry into focused topics: {slug}")
        if fields["Type"] not in {"canon", "historical", "legend", "mechanics", "community", "editorial"}:
            raise ValueError(f"Invalid source classification: {slug}")
        seen.add(slug)
        entries.append({"id": slug, "title": title, "keys": keys, "sources": sources,
                        "type": fields["Type"], "priority": int(fields["Priority"]),
                        "constant": fields.get("Always") == "true", "content": content})
    if not entries or sum(x["constant"] for x in entries) != 1:
        raise ValueError("Expected exactly one short, always-on reference-scope entry")
    return entries


def source_record(source, manifest):
    if source.startswith("wiki:"):
        title = source[5:]
        record = {"title": title, "url": "https://elite-dangerous.fandom.com/wiki/" +
                  quote(title.replace(" ", "_"), safe=""),
                  "publisher": "Elite Dangerous Wiki contributors", "reviewed": REVIEW_DATE,
                  "access": "web search index; no archived revision"}
        page = manifest.get("pages", {}).get(title)
        if page:
            article = json.loads((CORPUS / page["file"]).read_text(encoding="utf-8"))
            # Hash the corpus before using its revision metadata.
            raw = (CORPUS / page["file"]).read_bytes()
            raw_hash = hashlib.sha256(raw).hexdigest()
            # Crawler v1 on Windows wrote CRLF but hashed its pre-write LF string.
            # Accept that documented representation as well; record the actual bytes.
            lf_hash = hashlib.sha256(raw.replace(b"\r\n", b"\n")).hexdigest()
            if page["sha256"] not in {raw_hash, lf_hash}:
                raise ValueError(f"Archived source hash mismatch: {title}")
            record.update(url=article["source_url"], revision_id=article["revision_id"],
                          revision_url=article["revision_url"], retrieved_utc=article["retrieved_utc"],
                          archive_sha256=raw_hash, access="archived MediaWiki article")
        return record
    if not source.startswith("https://"):
        raise ValueError(f"Invalid source URL: {source}")
    return {"title": urlparse(source).netloc + urlparse(source).path,
            "url": source, "reviewed": REVIEW_DATE, "access": "web page or indexed excerpt",
            "note": "See research notes for source scope; not an archived MediaWiki revision."}


def build():
    source = SOURCE.read_text(encoding="utf-8")
    records = load_entries(source)
    manifest_path = CORPUS / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else {}
    sources = {s: source_record(s, manifest) for r in records for s in r["sources"]}
    entries = {}
    for uid, row in enumerate(records):
        label = {"historical": "Dated history", "legend": "Legend / unresolved",
                 "mechanics": "Game systems — version-sensitive", "community": "Player community",
                 "editorial": "Reference scope", "canon": "Setting background"}[row["type"]]
        entries[str(uid)] = {
            "uid": uid, "key": row["keys"], "keysecondary": [], "comment": row["title"],
            "content": f"[{label}: {row['title'].split(' — ', 1)[-1]}]\n{row['content']}",
            "constant": row["constant"], "vectorized": False, "selective": False,
            "selectiveLogic": 0, "addMemo": True, "order": row["priority"], "position": 0,
            "disable": False, "ignoreBudget": False, "excludeRecursion": True,
            "preventRecursion": True, "delayUntilRecursion": False,
            "probability": 100, "useProbability": True, "displayIndex": uid,
            "scanDepth": 2, "caseSensitive": False, "matchWholeWords": True,
            "matchPersonaDescription": False, "matchCharacterDescription": False,
            "matchCharacterPersonality": False, "matchCharacterDepthPrompt": False,
            "matchScenario": False, "matchCreatorNotes": False,
            "group": "", "groupOverride": False, "groupWeight": 100, "useGroupScoring": False,
            "sticky": 0, "cooldown": 0, "delay": 0, "depth": 4, "role": 0,
            "automationId": "", "outletName": "", "triggers": [],
            "extensions": {"holtzman": {"id": row["id"], "classification": row["type"],
                                         "reviewed": REVIEW_DATE,
                                         "sources": [sources[s] for s in row["sources"]]}},
        }
    book = {"name": BOOK_NAME, "description": "Researched Elite Dangerous setting reference. "
            "Keyword entries; sources in entry metadata; no live market or commander state.",
            "entries": entries, "extensions": {"holtzman": {
                "version": "1.0", "reviewed": REVIEW_DATE,
                "authoring_source": SOURCE.relative_to(ROOT).as_posix(),
                "authoring_sha256": hashlib.sha256(source.encode("utf-8")).hexdigest(),
                "attribution": "Original summaries of facts from credited sources. "
                               "Elite Dangerous and its universe belong to Frontier Developments. "
                               "Source articles have their own authors and licences.",
                "activation": "Opt-in book. One constant scope entry; other entries scan two messages. "
                              "Case insensitive, whole-word, no recursive cascade. Existing budgets apply.",
                "recommended_lore_budget_tokens": 3500,
            }}}
    EXPORT.write_text(json.dumps(book, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    CATALOG.write_text(json.dumps({"reviewed": REVIEW_DATE, "sources": sources}, indent=2,
                                  ensure_ascii=False) + "\n", encoding="utf-8")
    return book


class LocalTavern:
    def __init__(self, base):
        parsed = urlparse(base)
        if (parsed.scheme != "http" or parsed.hostname not in {"127.0.0.1", "localhost", "::1"}
                or parsed.username or parsed.password or parsed.query or parsed.fragment
                or parsed.path not in {"", "/"}):
            raise ValueError("Use a loopback HTTP SillyTavern URL without credentials or a path")
        self.base = base.rstrip("/")
        self.opener = build_opener(HTTPCookieProcessor(CookieJar()))
        self.token = self.request("/csrf-token")["token"]

    def request(self, path, data=None):
        headers = {"Content-Type": "application/json", "Origin": self.base}
        if hasattr(self, "token"):
            headers["X-CSRF-Token"] = self.token
        payload = json.dumps(data).encode("utf-8") if data is not None else None
        with self.opener.open(Request(self.base + path, data=payload, headers=headers), timeout=30) as r:
            return json.load(r)

    def install(self, book, replace=False):
        worlds = self.request("/api/worldinfo/list", {})
        exists = any(x["file_id"] == BOOK_NAME for x in worlds)
        old = self.request("/api/worldinfo/get", {"name": BOOK_NAME}) if exists else None
        if old == book:
            return {"name": BOOK_NAME, "entries": len(book["entries"]), "already_current": True}
        if old and old.get("entries") and not replace:
            raise RuntimeError("The book already contains entries. Export/review them before using --replace.")
        backup = None
        if old is not None:
            folder = ROOT / ".local/holt/lore-research/st-backups"
            folder.mkdir(parents=True, exist_ok=True)
            stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
            backup = folder / f"Elite-Dangerous-before-{stamp}.json"
            backup.write_text(json.dumps(old, ensure_ascii=False, indent=2), encoding="utf-8")
        result = self.request("/api/worldinfo/edit", {"name": BOOK_NAME, "data": book})
        if not result.get("ok"):
            raise RuntimeError("SillyTavern did not acknowledge the write")
        actual = self.request("/api/worldinfo/get", {"name": BOOK_NAME})
        if actual != book:
            raise RuntimeError("SillyTavern read-back differed from the requested book")
        return {"name": BOOK_NAME, "entries": len(actual["entries"]), "read_back_equal": True,
                "backup": str(backup) if backup else None,
                "activation": "Library only; character associations and global selections unchanged."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true", help="Write book through the local SillyTavern API")
    parser.add_argument("--url", default="http://127.0.0.1:8000")
    parser.add_argument("--replace", action="store_true", help="Back up and replace a populated book")
    args = parser.parse_args()
    book = build()
    print(f"Built {len(book['entries'])} entries: {EXPORT}")
    if args.install:
        print(json.dumps(LocalTavern(args.url).install(book, args.replace), indent=2))


if __name__ == "__main__":
    main()

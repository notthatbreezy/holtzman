"""Download a bounded, attributed Elite Dangerous reference corpus (Python 3.12+).

Uses the public MediaWiki API, obeys robots.txt, and stops on access blocks.
No login, browser cookies, API keys, proxy rotation, or challenge bypasses.
Downloaded text is private reference material, not automatically trusted lore.
"""

import argparse
import hashlib
import json
import time
from collections import deque
from datetime import datetime, timezone
from html.parser import HTMLParser
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen
from urllib.robotparser import RobotFileParser


ROOT = Path(__file__).resolve().parents[1]
ORIGIN = "https://elite-dangerous.fandom.com"
AGENT = "HoltzmanLoreResearch/1.0 (personal Elite Dangerous reference archive)"


class ArticleText(HTMLParser):
    """Keep article prose; discard scripts/styles and collapse layout whitespace."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.hidden = 0
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag in {"script", "style"}:
            self.hidden += 1
        elif tag in {"p", "div", "br", "li", "tr", "h1", "h2", "h3", "h4"}:
            self.parts.append("\n")

    def handle_endtag(self, tag):
        if tag in {"script", "style"}:
            self.hidden = max(0, self.hidden - 1)
        elif tag in {"p", "div", "li", "tr", "h1", "h2", "h3", "h4"}:
            self.parts.append("\n")
        elif tag in {"td", "th"}:
            self.parts.append(" | ")

    def handle_data(self, data):
        if not self.hidden:
            self.parts.append(data)

    def text(self):
        return "\n".join(line for raw in "".join(self.parts).splitlines()
                         if (line := " ".join(raw.split())))


class AccessBlocked(RuntimeError):
    pass


class Wiki:
    def __init__(self, delay):
        self.delay = max(delay, 1.5)
        self.last_request = 0.0
        self.robots = RobotFileParser()
        self.robots.set_url(ORIGIN + "/robots.txt")
        try:
            self.robots.parse(self.fetch(ORIGIN + "/robots.txt").decode("utf-8").splitlines())
        except HTTPError as exc:
            if exc.code != 404:
                raise AccessBlocked(f"Cannot check robots.txt (HTTP {exc.code}); stopped.") from exc
            self.robots.parse([])
        crawl_delay = self.robots.crawl_delay(AGENT) or self.robots.crawl_delay("*") or 0
        self.delay = max(self.delay, crawl_delay)

    def fetch(self, url):
        for attempt in range(4):
            time.sleep(max(0, self.delay - (time.monotonic() - self.last_request)))
            self.last_request = time.monotonic()
            try:
                with urlopen(Request(url, headers={"User-Agent": AGENT,
                                                  "Accept": "application/json,text/plain,*/*"}),
                             timeout=45) as response:
                    final = response.geturl()
                    if not final.startswith(ORIGIN + "/"):
                        raise AccessBlocked("Server redirected outside the wiki; stopped.")
                    body = response.read(12 * 1024 * 1024 + 1)
                    if len(body) > 12 * 1024 * 1024:
                        raise RuntimeError("Article response exceeded 12 MiB; skipped.")
                    return body
            except HTTPError as exc:
                if exc.code in {401, 403, 402}:
                    raise AccessBlocked(f"Wiki denied access (HTTP {exc.code}); no bypass attempted.") from exc
                if exc.code not in {429, 500, 502, 503, 504} or attempt == 3:
                    raise
                retry = exc.headers.get("Retry-After", "")
                if retry.isdigit() and int(retry) > 300:
                    raise AccessBlocked("Server requested a long pause; rerun later.") from exc
                time.sleep(max(5 * 2 ** attempt, int(retry) if retry.isdigit() else 0))
        raise RuntimeError("Request failed")

    def parse(self, title):
        page_url = ORIGIN + "/wiki/" + quote(title.replace(" ", "_"), safe="")
        if not self.robots.can_fetch(AGENT, page_url):
            raise AccessBlocked(f"robots.txt excludes {title}; stopped.")
        url = ORIGIN + "/api.php?" + urlencode({
            "action": "parse", "format": "json", "formatversion": 2,
            "page": title, "redirects": 1, "prop": "text|wikitext|revid|links|displaytitle",
        })
        if not self.robots.can_fetch(AGENT, url):
            raise AccessBlocked("robots.txt excludes the public API; stopped.")
        try:
            result = json.loads(self.fetch(url))
        except json.JSONDecodeError as exc:
            raise AccessBlocked("Wiki returned a non-JSON access/challenge page; stopped.") from exc
        if "error" in result:
            err = result["error"]
            raise RuntimeError(f"MediaWiki {err.get('code')}: {err.get('info')}")
        return result["parse"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / ".local/holt/lore-research/elite-wiki")
    parser.add_argument("--seed-file", type=Path, default=ROOT / "integrations/sillytavern/elite-wiki-seeds.txt")
    parser.add_argument("--page", action="append", help="Download only these titles instead of the seed list")
    parser.add_argument("--crawl", action="store_true", help="Also follow article links (main namespace only)")
    parser.add_argument("--max-pages", type=int, default=120)
    parser.add_argument("--max-depth", type=int, default=1)
    parser.add_argument("--delay", type=float, default=2.0, help="Seconds between requests; minimum 1.5")
    parser.add_argument("--list", action="store_true", help="Show planned seed titles without network access")
    args = parser.parse_args()
    if args.max_pages < 1 or args.max_depth < 0:
        parser.error("max-pages must be positive; max-depth cannot be negative")
    seeds = args.page or [x.strip() for x in args.seed_file.read_text(encoding="utf-8").splitlines()
                          if x.strip() and not x.lstrip().startswith("#")]
    if args.list:
        print("\n".join(dict.fromkeys(seeds)))
        return
    args.output.mkdir(parents=True, exist_ok=True)
    manifest_path = args.output / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else {
        "origin": ORIGIN, "notice": "Attributed third-party reference corpus; not verified instructions.",
        "pages": {}, "errors": [],
    }
    if manifest.get("origin") != ORIGIN:
        parser.error("Output directory belongs to another origin")

    def checkpoint():
        temporary = manifest_path.with_suffix(".json.tmp")
        temporary.write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")
        temporary.replace(manifest_path)

    queue = deque((title, 0) for title in dict.fromkeys(seeds))
    seen, processed, downloaded = set(), 0, 0
    try:
        wiki = Wiki(args.delay)
        while queue and processed < args.max_pages:
            title, depth = queue.popleft()
            if title.casefold() in seen:
                continue
            seen.add(title.casefold())
            processed += 1
            previous = manifest["pages"].get(title)
            if previous and (args.output / previous["file"]).is_file():
                article = json.loads((args.output / previous["file"]).read_text(encoding="utf-8"))
                print(f"Cached: {title}", flush=True)
            else:
                try:
                    parsed = wiki.parse(title)
                    html = parsed["text"]
                    if isinstance(html, dict):
                        html = html["*"]
                    extractor = ArticleText()
                    extractor.feed(html)
                    source_url = ORIGIN + "/wiki/" + quote(parsed["title"].replace(" ", "_"), safe="")
                    article = {"title": parsed["title"], "page_id": parsed["pageid"],
                               "revision_id": parsed["revid"], "source_url": source_url,
                               "revision_url": ORIGIN + "/index.php?" + urlencode({"oldid": parsed["revid"]}),
                               "retrieved_utc": datetime.now(timezone.utc).isoformat(),
                               "attribution": "Elite Dangerous Wiki contributors; consult source for licensing.",
                               "text": extractor.text(), "html": html, "wikitext": parsed.get("wikitext"),
                               "links": [x.get("title", x.get("*")) for x in parsed.get("links", [])
                                         if x.get("ns") == 0 and x.get("exists") is not False]}
                    filename = str(article["page_id"]) + ".json"
                    serialized = json.dumps(article, ensure_ascii=False, indent=2)
                    (args.output / filename).write_bytes(serialized.encode("utf-8"))
                    manifest["pages"][title] = {"title": article["title"], "file": filename,
                                                "revision_id": article["revision_id"], "url": source_url,
                                                "sha256": hashlib.sha256(serialized.encode("utf-8")).hexdigest()}
                    downloaded += 1
                    checkpoint()
                    print(f"Saved: {article['title']} (revision {article['revision_id']})", flush=True)
                except AccessBlocked:
                    raise
                except (RuntimeError, HTTPError, URLError, KeyError, ValueError) as exc:
                    manifest["errors"].append({"title": title, "error": str(exc)[:300]})
                    checkpoint()
                    print(f"Skipped: {title}: {exc}", flush=True)
                    continue
            if args.crawl and depth < args.max_depth:
                queue.extend((link, depth + 1) for link in article.get("links", []) if isinstance(link, str))
    except (AccessBlocked, URLError) as exc:
        manifest["last_stop"] = str(exc)
        checkpoint()
        print(f"Stopped: {exc}\nAvailable corpus: {args.output}", flush=True)
        raise SystemExit(2) from None
    except KeyboardInterrupt:
        checkpoint()
        print("Stopped by user; completed pages retained.", flush=True)
        raise SystemExit(130) from None
    checkpoint()
    print(f"Done: {downloaded} new pages; {len(manifest['pages'])} total. {args.output}")


if __name__ == "__main__":
    main()

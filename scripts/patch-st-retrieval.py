"""Back up and repair ST 1.19.0 tool-result retrieval and embedding limits."""
import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path


QUERY_BEFORE = "    let hashedMessages = chat\n        .map(x => ({ text: substituteParams(getTextWithoutAttachments(x)),"
QUERY_AFTER = "    let hashedMessages = chat\n        // Tool receipts are evidence, not semantic search queries.\n        .filter(x => !x.extra?.tool_invocations?.length)\n        .map(x => ({ text: substituteParams(getTextWithoutAttachments(x)),"
EMBED_BEFORE = "    const pipe = await getPipeline(TASK);\n    const result = await pipe(text, { pooling: 'mean', normalize: true });"
EMBED_AFTER = """    const pipe = await getPipeline(TASK);
    // Bound local embedding attention memory. Some exported tokenizers advertise
    // billions of tokens despite the model's much smaller supported context.
    const declaredLimit = Number(pipe.tokenizer.model_max_length);
    pipe.tokenizer.model_max_length = Number.isFinite(declaredLimit) && declaredLimit > 0
        ? Math.min(declaredLimit, 512) : 512;
    const result = await pipe(text, { pooling: 'mean', normalize: true });"""
PATCHES = {"public/scripts/extensions/vectors/index.js": (QUERY_BEFORE, QUERY_AFTER),
           "src/vectors/embedding.js": (EMBED_BEFORE, EMBED_AFTER)}
INDEX_BEFORE = "context.chat.filter(x => settings.keep_hidden || !x.is_system).map(x =>"
INDEX_AFTER = "context.chat.filter(x => !x.extra?.tool_invocations?.length && (settings.keep_hidden || !x.is_system)).map(x =>"
RETRIEVE_BEFORE = "            if (retainMessages.includes(message) || !message.mes) {"
RETRIEVE_AFTER = "            if (retainMessages.includes(message) || !message.mes || message.extra?.tool_invocations?.length) {"
PATCH_GROUPS = {relative: [pair] for relative, pair in PATCHES.items()}
PATCH_GROUPS["public/scripts/extensions/vectors/index.js"].extend([
    (INDEX_BEFORE, INDEX_AFTER), (RETRIEVE_BEFORE, RETRIEVE_AFTER)])


def prepare(st_dir):
    package = json.loads((st_dir / "package.json").read_text(encoding="utf-8"))
    if package.get("version") != "1.19.0":
        raise ValueError("Unsupported ST version; inspect before patching")
    changes = []
    for relative, pairs in PATCH_GROUPS.items():
        path = st_dir / relative
        original = path.read_bytes()
        newline = "\r\n" if b"\r\n" in original else "\n"
        updated = original
        for before, after in pairs:
            before = before.replace("\n", newline).encode()
            after = after.replace("\n", newline).encode()
            if updated.count(after) == 1 and before not in updated:
                continue
            if updated.count(before) != 1 or after in updated:
                raise ValueError(f"Unrecognized retrieval code in {relative}; no changes made")
            updated = updated.replace(before, after)
        if updated != original:
            changes.append((relative, path, original, updated))
    return changes


def apply(st_dir, backup_root):
    changes = prepare(st_dir)
    if not changes:
        return None
    backup = backup_root / ("st-retrieval-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ"))
    backup.mkdir(parents=True, exist_ok=False)
    manifest = {}
    for relative, _, original, updated in changes:
        target = backup / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(original)
        manifest[relative] = {"before": hashlib.sha256(original).hexdigest(),
                              "after": hashlib.sha256(updated).hexdigest()}
    (backup / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    for _, path, _, updated in changes:
        path.write_bytes(updated)
    return backup


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--st-dir", type=Path, default=Path.home() / "SillyTavern")
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    changes = prepare(args.st_dir)
    print("Retrieval:", ", ".join(c[0] for c in changes) if changes else "already patched")
    if args.apply and changes:
        print("Backups:", apply(args.st_dir, Path(__file__).resolve().parents[1] / ".local/holt/results"))
        print("Restart ST and refresh its browser tab.")
    elif changes:
        print("Dry run. Use --apply to install.")


if __name__ == "__main__":
    main()

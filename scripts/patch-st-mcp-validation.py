"""Repair anonymous-schema $ref resolution in ST MCP's bundled jsonschema validator.

Dry run by default. Patches both TypeScript and the installed production bundle;
backs up originals locally before applying. Does not disable argument validation.
"""
import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path


BASE = "https://mcp.invalid/schema/"
PATCHES = {
    "src/McpClient.ts": (
        "new Validator().validate(params.arguments, schema, { throwError: true });",
        f"new Validator().validate(params.arguments, schema, {{ throwError: true, base: '{BASE}' }});",
    ),
    "dist/index.js": (
        'validate(e.arguments,t,{throwError:!0}),this.sendRequest("tools/call",e)',
        f'validate(e.arguments,t,{{throwError:!0,base:"{BASE}"}}),this.sendRequest("tools/call",e)',
    ),
}


def prepare(plugin):
    package = json.loads((plugin / "package.json").read_text(encoding="utf-8"))
    if (package.get("name"), package.get("version")) != ("sillytavern-mcp-server", "1.0.0"):
        raise ValueError("Unsupported MCP plugin version; inspect before patching")
    changes = []
    # Validate ALL files before writing either one. Preserve bytes/newlines.
    for relative, (old, new) in PATCHES.items():
        path = plugin / relative
        original = path.read_bytes()
        old, new = old.encode(), new.encode()
        if original.count(new) == 1 and old not in original:
            continue
        if original.count(old) != 1 or new in original:
            raise ValueError(f"Unrecognized validator call in {relative}; no changes made")
        changes.append((relative, path, original, original.replace(old, new)))
    return changes


def apply(plugin, backup_root):
    changes = prepare(plugin)
    if not changes:
        return None
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    backup = backup_root / f"st-mcp-validation-{stamp}"
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
    parser.add_argument("--backup-dir", type=Path,
                        default=Path(__file__).resolve().parents[1] / ".local/holt/results")
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    plugin = args.st_dir / "plugins/SillyTavern-MCP-Server"
    changes = prepare(plugin)
    print("Validator:", "already patched" if not changes else ", ".join(c[0] for c in changes))
    if args.apply and changes:
        print("Backups:", apply(plugin, args.backup_dir))
        print("Restart SillyTavern to load the repaired server plugin.")
    elif changes:
        print("Dry run. Use --apply to install.")


if __name__ == "__main__":
    main()

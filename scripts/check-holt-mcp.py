"""Read-only MCP discovery/status check; credentials never enter model context."""
import argparse
import asyncio
import getpass
import json
import logging
import warnings
from pathlib import Path

import httpx2
from keyring.backends.Windows import WinVaultKeyring
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client
from holt_discovery import discover, save_discovery


async def check(output, discovery_directory=None):
    token = WinVaultKeyring().get_password("HoltzmanVoice", "holtzman-bearer")
    if not token:
        print("No HOLT credential saved. Run this script with --save-credential in your own terminal.")
        return 2
    async with asyncio.timeout(45):
        async with httpx2.AsyncClient(
            headers={"Authorization": f"Bearer {token}"}, timeout=15,
            follow_redirects=False, trust_env=False,
        ) as http:
            async with streamable_http_client(
                "http://skippy:7777/mcp", http_client=http
            ) as (read, write):
                async with ClientSession(read, write) as session:
                    initialized = await session.initialize()
                    bundle = await discover(session, initialized)
                    tools = bundle["tools"]
                    print("Tools:", ", ".join(sorted(t["name"] for t in tools)))
                    print("Prompts:", ", ".join(p["name"] for p in bundle["prompts"]))
                    print("Resources:", ", ".join(r["uri"] for r in bundle["resources"]))
                    if discovery_directory:
                        digest = save_discovery(bundle, discovery_directory)
                        print("Server instructions and discovery snapshot saved.")
                        print("Ship-computer prompt SHA-256:", digest or "not advertised")
                    if output:
                        output.parent.mkdir(parents=True, exist_ok=True)
                        output.write_text(json.dumps(tools, indent=2), encoding="utf-8")
                    result = await session.call_tool("status", {})
                    if result.model_dump(by_alias=True).get("isError"):
                        print("Status returned a tool error.")
                        return 1
                    for block in result.content:
                        if block.type == "text":
                            print(block.text)
                    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--save-credential", action="store_true")
    parser.add_argument("--schemas", type=Path)
    parser.add_argument("--discovery", type=Path, help="Save instructions, prompt and resource index")
    args = parser.parse_args()
    logging.disable(logging.CRITICAL)
    if args.save_credential:
        warnings.simplefilter("error", getpass.GetPassWarning)
        token = getpass.getpass("Holtzman token (hidden; never paste into chat): ").strip()
        if not token or token.startswith("Bearer ") or "\n" in token or "\r" in token:
            raise SystemExit("Enter only the token value on one line.")
        vault = WinVaultKeyring()
        vault.persist = "local machine"
        vault.set_password("HoltzmanVoice", "holtzman-bearer", token)
        print("Credential saved in Windows Credential Manager.")
        return 0
    try:
        return asyncio.run(check(args.schemas, args.discovery))
    except Exception as exc:
        # Do not print SDK exception messages/tracebacks: they may expose headers.
        print("MCP check failed:", type(exc).__name__)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

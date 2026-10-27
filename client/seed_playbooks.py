"""Install bundled draft playbooks into the updated MCP server; never overwrite edits."""
import asyncio
import json

import httpx2
from keyring.backends.Windows import WinVaultKeyring
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client

from client.ship_link import ship_transport
from client.state import REPO, load_settings


async def main():
    settings = load_settings()
    key = WinVaultKeyring().get_password("HoltzmanVoice", "holtzman-bearer")
    if not key:
        raise RuntimeError("Save the existing MCP credential first")
    async with httpx2.AsyncClient(transport=ship_transport(settings["mcp_url"]),
                                 headers={"Authorization": "Bearer " + key}, timeout=30, trust_env=False) as http:
        async with streamable_http_client(settings["mcp_url"], http_client=http) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                names = {t.name for t in (await session.list_tools()).tools}
                if not {"save_playbook", "read_playbook"} <= names:
                    raise RuntimeError("The running SKIPPY server needs updating; no playbook was saved")
                for path in sorted((REPO / "examples/playbooks").glob("*.json")):
                    args = json.loads(path.read_text(encoding="utf-8"))
                    existing = await session.call_tool("read_playbook", {"name": args["name"]})
                    if not existing.is_error:
                        print(args["name"] + ": already exists; retained without changes")
                        continue
                    result = await session.call_tool("save_playbook", args)
                    if result.is_error:
                        raise RuntimeError("Could not save " + args["name"] + "; existing revisions were not replaced")
                    print(args["name"] + ": saved as draft revision 1")


if __name__ == "__main__":
    asyncio.run(main())

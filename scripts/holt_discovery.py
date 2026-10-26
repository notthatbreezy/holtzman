"""Discover the deployed server contract without invoking action tools."""
import hashlib
from pathlib import Path
import json

from mcp import types


async def collect_pages(method, attribute):
    cursor = None
    seen = set()
    items = []
    for _ in range(100):
        page = await method(params=types.PaginatedRequestParams(cursor=cursor))
        items.extend(getattr(page, attribute))
        cursor = page.next_cursor
        if not cursor:
            return items
        if cursor in seen:
            raise RuntimeError("Repeated discovery cursor")
        seen.add(cursor)
    raise RuntimeError("Discovery pagination limit exceeded")


async def discover(session, initialized):
    tools = await collect_pages(session.list_tools, "tools")
    prompts = []
    resources = []
    ship_prompt = None
    if initialized.capabilities.prompts is not None:
        prompts = await collect_pages(session.list_prompts, "prompts")
        if any(prompt.name == "ship_computer" for prompt in prompts):
            ship_prompt = await session.get_prompt("ship_computer")
    if initialized.capabilities.resources is not None:
        resources = await collect_pages(session.list_resources, "resources")
    encode = lambda item: item.model_dump(mode="json", by_alias=True)
    return {"initialize": encode(initialized), "tools": [encode(t) for t in tools],
            "prompts": [encode(p) for p in prompts],
            "resources": [encode(r) for r in resources],
            "ship_computer": encode(ship_prompt) if ship_prompt is not None else None}


def prompt_text(bundle):
    prompt = bundle.get("ship_computer") or {}
    return "\n\n".join(message["content"]["text"] for message in prompt.get("messages", [])
                         if message.get("content", {}).get("type") == "text")


def save_discovery(bundle, directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "mcp-discovery.json").write_text(json.dumps(bundle, indent=2), encoding="utf-8")
    text = prompt_text(bundle)
    if text:
        (directory / "ship-computer.md").write_text(text, encoding="utf-8", newline="\n")
    return hashlib.sha256(text.encode()).hexdigest() if text else None

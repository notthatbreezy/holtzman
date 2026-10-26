"""Local token accounting. Prices are estimates, not provider billing receipts."""
import json
from datetime import datetime, timezone

from client.state import ROOT


def estimated_cost(model, usage):
    if model != "gpt-5.4-mini":
        return None
    prompt = usage.get("prompt_tokens", 0)
    cached = min(prompt, max(0, usage.get("prompt_tokens_details", {}).get("cached_tokens", 0)))
    completion = usage.get("completion_tokens", 0)
    # completion_tokens already includes billed reasoning output; don't add it twice.
    return ((prompt - cached) * .75 + cached * .075 + completion * 4.5) / 1_000_000 + usage.get("web_search_calls", 0) * .01


def record_usage(model, usage, root=ROOT):
    directory = root / "results"
    directory.mkdir(parents=True, exist_ok=True)
    cost = estimated_cost(model, usage)
    entry = {"time": datetime.now(timezone.utc).isoformat(), "model": model, "usage": usage,
             "estimated_usd": cost, "pricing_date": "2026-10-26"}
    with (directory / "openai-usage.jsonl").open("a", encoding="utf-8") as output:
        output.write(json.dumps(entry) + "\n")
    return cost

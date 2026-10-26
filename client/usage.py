"""Local token accounting. Prices are estimates, not provider billing receipts."""
import json
from datetime import datetime, timezone

from client.state import ROOT


def estimated_cost(model, usage):
    # USD per million tokens; official model pages verified 2026-10-26.
    rates = {"gpt-5.4-mini": (.75, .075, 4.5), "gpt-5.6-luna": (.20, .02, 1.2),
             "gpt-6-luna": (.10, .01, .50), "gpt-5.6-terra": (2., .20, 12.),
             "gpt-6.1-sol": (2., .10, 10.)}
    if model not in rates:
        return None
    input_rate, cached_rate, output_rate = rates[model]
    prompt = usage.get("prompt_tokens", 0)
    cached = min(prompt, max(0, usage.get("prompt_tokens_details", {}).get("cached_tokens", 0)))
    completion = usage.get("completion_tokens", 0)
    # completion_tokens already includes billed reasoning output; don't add it twice.
    cache_write = min(prompt - cached, max(0, usage.get("prompt_tokens_details", {}).get("cache_write_tokens", 0)))
    return ((prompt - cached - cache_write) * input_rate + cached * cached_rate
            + cache_write * input_rate * 1.25 + completion * output_rate) / 1_000_000 + usage.get("web_search_calls", 0) * .01


def record_usage(model, usage, root=ROOT):
    directory = root / "results"
    directory.mkdir(parents=True, exist_ok=True)
    cost = estimated_cost(model, usage)
    entry = {"time": datetime.now(timezone.utc).isoformat(), "model": model, "usage": usage,
             "estimated_usd": cost, "pricing_date": "2026-10-26"}
    with (directory / "openai-usage.jsonl").open("a", encoding="utf-8") as output:
        output.write(json.dumps(entry) + "\n")
    return cost

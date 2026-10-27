"""Request-local context handling; action receipts and provider state stay intact."""
import copy
import hashlib
import json


CACHEABLE = frozenset({"trade_commodities", "find_commodity_market", "plan_trade_route",
                       "find_module", "read_guide"})
SUMMARIZABLE = CACHEABLE | {"market", "fleet", "cargo", "status", "recent_events", "nav_route"}


def context_limits(settings):
    if settings.get("model_provider") != "openai":
        budget = settings["context_size"]
    else:
        # Verified official specifications; the old 32K client cap was not a Luna limit.
        budget = {"gpt-6-luna": 1050000, "gpt-6.1-sol": 1050000}.get(settings.get("openai_model"), 32768)
    return budget, min(int(budget * .82), 131072)


def dumps(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def query_key(name, arguments):
    return name + ":" + json.dumps(arguments, sort_keys=True, ensure_ascii=False)


def estimate_tokens(messages, tools):
    """Estimate visible input once; opaque ciphertext is not ordinary text tokens.

    Keep the real _response_items untouched for the provider. Count the mirrored
    content/calls once and add measured reasoning usage, not base64 string length.
    """
    visible, images, reasoning = [], 0, 0
    for message in messages:
        item = {k: copy.deepcopy(message[k]) for k in ("role", "content", "tool_calls", "tool_call_id") if k in message}
        if isinstance(item.get("content"), list):
            for part in item["content"]:
                if part.get("type") == "image_url":
                    images += 1
                    part.clear()
                    part.update(type="text", text="[cockpit image]")
        reasoning += max(0, message.get("_reasoning_tokens", 0))
        # Old converted responses may lack usage; reserve a bounded allowance per item.
        if "_reasoning_tokens" not in message:
            reasoning += 512 * sum(i.get("type") == "reasoning" for i in message.get("_response_items", []))
        visible.append(item)
    return int(len(dumps({"messages": visible, "tools": tools})) / 2.5) + 512 + images * 2048 + reasoning


def tool_content(name, texts):
    """Preserve complete JSON instead of slicing through a row or price."""
    normalized = []
    for text in texts:
        try:
            data = json.loads(text)
            if name == "plan_trade_route" and "itinerary_summary" in data:
                # The complete itinerary already contains each leg and its cargo.
                data = {"itinerary_summary": data["itinerary_summary"],
                        "warning": data.get("provider_result", {}).get("warning")}
            normalized.append(dumps(data))
        except (ValueError, TypeError):
            normalized.append(text)
    return "\n".join(normalized)


def history_source(messages, turn_start):
    """Compact only older external history; retain the last three exchanges."""
    end = max(1, turn_start - 6)
    return end, messages[1:end]


def summary_key(source):
    return hashlib.sha256(dumps(source).encode()).hexdigest()


def read_receipts(messages, schemas):
    """Find completed older reads; never summarize a game action or knowledge write."""
    calls = {}
    for message in messages:
        for call in message.get("tool_calls", []):
            calls[call["id"]] = call["function"]
    candidates = []
    tool_indexes = [i for i, m in enumerate(messages) if m["role"] == "tool"]
    for index in tool_indexes[:-2]:
        message = messages[index]
        fn = calls.get(message["tool_call_id"], {})
        name = fn.get("name")
        if name not in SUMMARIZABLE or not (schemas.get(name, {}).get("annotations") or {}).get("readOnlyHint", True):
            continue
        if len(message["content"]) < 800:
            continue
        try:
            result = json.loads(message["content"])
        except ValueError:
            continue
        if result.get("isError") or result.get("context_checkpoint"):
            continue
        candidates.append((index, {"tool_call_id": message["tool_call_id"], **fn, "result": result}))
    return candidates

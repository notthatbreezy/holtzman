"""Small, dependency-free contract shared by the Tavern gateway and its tests."""
import copy
import hashlib
import json
import time

MODELS = {
    "holt-hosted": {"model_provider": "openai", "openai_model": "gpt-5.4-mini", "openai_reasoning": "low"},
    "holt-hosted-fast": {"model_provider": "openai", "openai_model": "gpt-5.4-mini", "openai_reasoning": "none"},
    "holt-5.6-luna": {"model_provider": "openai", "openai_model": "gpt-5.6-luna", "openai_reasoning": "low"},
    "holt-6-luna": {"model_provider": "openai", "openai_model": "gpt-6-luna", "openai_reasoning": "low"},
    "holt-5.6-terra": {"model_provider": "openai", "openai_model": "gpt-5.6-terra", "openai_reasoning": "low"},
    "holt-6.1-sol": {"model_provider": "openai", "openai_model": "gpt-6.1-sol", "openai_reasoning": "low"},
    "holt-local": {"model_provider": "local", "local_thinking": False},
    "holt-local-thinking": {"model_provider": "local", "local_thinking": True},
}
ORIGINS = {"http://127.0.0.1:8000", "http://localhost:8000"}


def normalize_messages(messages):
    """Keep ST history once; Qwen requires a single leading system message."""
    if not isinstance(messages, list) or not messages or len(messages) > 256:
        raise ValueError("Expected 1–256 text chat messages")
    systems, turns = [], []
    for message in messages:
        if not isinstance(message, dict) or message.get("role") not in {"system", "user", "assistant"}:
            raise ValueError("External chat may contain only system/user/assistant text")
        content = message.get("content")
        if not isinstance(content, str):
            raise ValueError("External media is not a game-window capture; only text input is accepted")
        if len(content) > 120000:
            raise ValueError("Chat message exceeds input limit")
        if message["role"] == "system":
            systems.append(content)
        else:
            turns.append({"role": message["role"], "content": content})
    if not turns:
        raise ValueError("A conversation message is required")
    return [{"role": "system", "content": "\n\n".join(systems)}, *turns]


def request_key(messages, chat):
    return hashlib.sha256(json.dumps([chat, messages], sort_keys=True).encode()).hexdigest()


class Receipts:
    """Cache completed or uncertain requests; never replay their game actions."""
    def __init__(self, ttl=600):
        self.ttl, self.items = ttl, {}

    def get(self, key):
        self.items = {k: v for k, v in self.items.items() if time.monotonic() - v[0] < self.ttl}
        value = self.items.get(key)
        return copy.deepcopy(value[1]) if value else None

    def put(self, key, reply):
        self.items[key] = (time.monotonic(), copy.deepcopy(reply))

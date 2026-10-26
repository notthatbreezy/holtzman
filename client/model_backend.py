"""Scoped model transport; provider credentials never reach MCP or speech servers."""
from urllib.parse import urlparse


def openai_payload(payload, model="gpt-5.4-mini", effort="low"):
    allowed = ("messages", "tools", "tool_choice", "response_format", "parallel_tool_calls")
    body = {k: payload[k] for k in allowed if k in payload}
    body.update(model=model, store=False,
                reasoning_effort=effort if payload.get("tools") else "none",
                max_completion_tokens=max(1536 if payload.get("tools") else 512, payload.get("max_tokens", 512)))
    return body


class ModelHTTP:
    def __init__(self, settings, http, key=None, report=lambda *_: None):
        self.settings, self.http, self.report = settings, http, report
        self.provider = settings.get("model_provider", "local")
        if self.provider not in ("local", "openai"):
            raise ValueError("Unknown model provider")
        self.key = None
        self.session_cost = 0
        if self.provider == "openai":
            if key is None:
                from client.credentials import openai_key
                key = openai_key()
            if not key:
                raise RuntimeError("Save an OpenAI API key using the panel's Set OpenAI key button.")
            self.key = key

    async def post(self, url, *, json):
        if self.provider == "local":
            return await self.http.post(url, json=json)
        if urlparse(url).path != "/v1/chat/completions":
            raise ValueError("Only model completions may use the OpenAI credential")
        # A fixed HTTPS destination and per-request header avoid credential leakage
        # through shared HTTP clients, provider URL settings or redirects.
        response = await self.http.post("https://api.openai.com/v1/chat/completions",
            headers={"Authorization": "Bearer " + self.key}, follow_redirects=False,
            json=openai_payload(json, self.settings.get("openai_model", "gpt-5.4-mini"),
                                self.settings.get("openai_reasoning", "low")))
        if response.status_code >= 300:
            try:
                code = response.json().get("error", {}).get("code") or "request_failed"
            except ValueError:
                code = "request_failed"
            safe_codes = {"invalid_api_key", "insufficient_quota", "credit_balance_exhausted", "model_not_found", "rate_limit_exceeded"}
            code = code if code in safe_codes else "request_failed"
            raise RuntimeError(f"OpenAI request failed (HTTP {response.status_code}, {code}). No game controls replayed.")
        usage = response.json().get("usage", {})
        try:
            from client.usage import record_usage
            cost = record_usage(self.settings.get("openai_model", "gpt-5.4-mini"), usage)
            if cost is not None:
                self.session_cost += cost
                self.report("service", f"Estimated OpenAI cost: ${cost:.4f} this call; ${self.session_cost:.4f} this client session")
        except OSError:
            self.report("service", "Could not write local API usage accounting")
        self.report("service", f"OpenAI tokens: {usage.get('prompt_tokens', '?')} input, {usage.get('completion_tokens', '?')} output")
        return response

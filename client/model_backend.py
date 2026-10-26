"""Scoped model transport; provider credentials never reach MCP or speech servers."""
from urllib.parse import urlparse
import re
import httpx2


def openai_payload(payload, model="gpt-5.4-mini", effort="low", web_calls=0, allow_images=False):
    inputs = []
    for message in payload["messages"]:
        if message.get("_response_items"):
            inputs.extend(message["_response_items"])
        elif message["role"] == "tool":
            inputs.append({"type": "function_call_output", "call_id": message["tool_call_id"], "output": message["content"]})
        elif message.get("tool_calls"):
            for call in message["tool_calls"]:
                inputs.append({"type": "function_call", "call_id": call["id"], **call["function"]})
        else:
            content = message.get("content") or ""
            if isinstance(content, list):
                if not allow_images and any(p["type"] == "image_url" for p in content):
                    raise ValueError("Hosted cockpit images require explicit authorization")
                content = [{"type": "input_text", "text": p["text"]} if p["type"] == "text"
                           else {"type": "input_image", "image_url": p["image_url"]["url"]} for p in content]
            inputs.append({"role": message["role"], "content": content})
    body = {"model": model, "input": inputs, "store": False,
            "reasoning": {"effort": effort if payload.get("tools") else "none"},
            "include": ["reasoning.encrypted_content"],
            "max_output_tokens": max(1536 if payload.get("tools") else 512, payload.get("max_tokens", 512))}
    if payload.get("tools"):
        body["tools"] = [{"type": "function", **t["function"], "strict": False} for t in payload["tools"]]
        if web_calls:
            body["tools"].append({"type": "web_search"})
            body["max_tool_calls"] = web_calls
        body["tool_choice"] = payload.get("tool_choice", "auto")
    formatting = payload.get("response_format", {})
    if formatting.get("type") == "json_schema":
        body["text"] = {"format": {"type": "json_schema", **formatting["json_schema"]}}
    return body


def completion_result(data):
    output = data.get("output", [])
    text, calls, sources = [], [], []
    for item in output:
        if item["type"] == "function_call":
            calls.append({"id": item["call_id"], "type": "function", "function": {
                "name": item["name"], "arguments": item["arguments"]}})
        if item["type"] == "message":
            for part in item.get("content", []):
                if part["type"] == "output_text":
                    text.append(part["text"])
                    for annotation in part.get("annotations", []):
                        if annotation.get("type") == "url_citation" and urlparse(annotation.get("url", "")).scheme in ("https", "http"):
                            sources.append({"title": annotation.get("title", "Source"), "url": annotation["url"]})
    usage = data.get("usage", {})
    usage = {"prompt_tokens": usage.get("input_tokens", 0), "completion_tokens": usage.get("output_tokens", 0),
             "prompt_tokens_details": usage.get("input_tokens_details", {}),
             "completion_tokens_details": usage.get("output_tokens_details", {}),
             "web_search_calls": sum(i["type"] == "web_search_call" for i in output)}
    message = {"role": "assistant", "content": "\n".join(text), "_response_items": output, "_sources": sources}
    if calls:
        message["tool_calls"] = calls
    finish = "length" if data.get("status") == "incomplete" else "tool_calls" if calls else "stop"
    return {"choices": [{"finish_reason": finish, "message": message}], "usage": usage}


class ModelHTTP:
    def __init__(self, settings, http, key=None, report=lambda *_: None):
        self.settings, self.http, self.report = settings, http, report
        self.provider = settings.get("model_provider", "local")
        if self.provider not in ("local", "openai"):
            raise ValueError("Unknown model provider")
        self.key = None
        self.session_cost = 0
        self.web_calls = 0
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
        response = await self.http.post("https://api.openai.com/v1/responses",
            headers={"Authorization": "Bearer " + self.key}, follow_redirects=False,
            json=openai_payload(json, self.settings.get("openai_model", "gpt-5.4-mini"),
                                self.settings.get("openai_reasoning", "low"),
                                max(0, 2 - self.web_calls) if self.settings.get("openai_web_search", False) else 0,
                                self.settings.get("allow_hosted_images", False)))
        if response.status_code >= 300:
            try:
                error = response.json().get("error", {})
                code = error.get("code") or "request_failed"
                detail = str(error.get("message", "")).replace(self.key, "[redacted]")
                detail = re.sub(r"sk-[A-Za-z0-9_-]+", "[redacted]", detail)
                detail = " ".join(detail.split())[:350]
            except ValueError:
                code = "request_failed"
                detail = ""
            safe_codes = {"invalid_api_key", "insufficient_quota", "credit_balance_exhausted", "model_not_found", "rate_limit_exceeded"}
            code = code if code in safe_codes else "request_failed"
            raise RuntimeError(f"OpenAI request failed (HTTP {response.status_code}, {code}): {detail}. No game controls replayed.")
        converted = completion_result(response.json())
        usage = converted["usage"]
        self.web_calls += usage["web_search_calls"]
        try:
            from client.usage import record_usage
            cost = record_usage(self.settings.get("openai_model", "gpt-5.4-mini"), usage)
            if cost is not None:
                self.session_cost += cost
                self.report("service", f"Estimated OpenAI cost: ${cost:.4f} this call; ${self.session_cost:.4f} this client session")
        except OSError:
            self.report("service", "Could not write local API usage accounting")
        self.report("service", f"OpenAI tokens: {usage.get('prompt_tokens', '?')} input, {usage.get('completion_tokens', '?')} output")
        return httpx2.Response(200, json=converted, request=httpx2.Request("POST", "https://api.openai.com/v1/responses"))

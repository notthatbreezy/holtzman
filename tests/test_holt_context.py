import copy
import json
import tempfile
import threading
import unittest
from types import SimpleNamespace
from unittest.mock import AsyncMock

import httpx2

from client.context import dumps, estimate_tokens, read_receipts, tool_content
from client.model_backend import completion_result, openai_payload


def response(message, finish="stop"):
    return httpx2.Response(200, request=httpx2.Request("POST", "http://test/model"),
                           json={"choices": [{"finish_reason": finish, "message": message}]})


class ContextRepresentationTests(unittest.TestCase):
    def test_opaque_reasoning_is_preserved_but_not_counted_as_base64_text(self):
        output = [{"type": "reasoning", "encrypted_content": "x" * 200000, "summary": []},
                  {"type": "function_call", "call_id": "c1", "name": "market", "arguments": "{}"}]
        converted = completion_result({"output": output, "usage": {"output_tokens_details": {"reasoning_tokens": 123}}})
        message = converted["choices"][0]["message"]
        original = copy.deepcopy(message)
        self.assertLess(estimate_tokens([message], []), 1000)
        self.assertEqual(message["_reasoning_tokens"], 123)
        self.assertEqual(openai_payload({"messages": [message]})["input"], output)
        self.assertEqual(message, original)

    def test_full_market_rows_survive_in_valid_compact_json(self):
        data = {"station": "Qureshi Orbital", "timestamp": "2026-10-27T21:00:00Z",
                "items": [{"name": "Coffee", "buy": 1250, "sell": 4800, "stock": 1234}] * 300}
        text = tool_content("market", [json.dumps(data, indent=2)])
        self.assertGreater(len(text), 10000)
        self.assertEqual(json.loads(text), data)
        self.assertLess(len(text), len(json.dumps(data, indent=2)))

    def test_read_compaction_excludes_actions_writes_errors_and_latest_results(self):
        messages = []
        for n, name in enumerate(["find_commodity_market", "plot_route", "save_playbook", "market", "status", "cargo"]):
            messages.extend([{"role": "assistant", "tool_calls": [{"id": str(n), "function": {"name": name, "arguments": "{}"}}]},
                             {"role": "tool", "tool_call_id": str(n), "content": dumps({"isError": name == "market", "content": "x" * 2000})}])
        candidates = read_receipts(messages, {})
        self.assertEqual([r["name"] for _, r in candidates], ["find_commodity_market"])


class ContextRecoveryTests(unittest.IsolatedAsyncioTestCase):
    def engine(self, root):
        from client.engine import Engine
        from client.state import DEFAULTS, Memory
        engine = Engine(DEFAULTS | {"model_provider": "openai", "allow_actions": False}, lambda *a: None, threading.Event())
        engine.memory = Memory(root)
        engine.trace = lambda *a, **k: None
        engine.schemas = {}
        engine.model_post = AsyncMock(return_value=response({"role": "assistant", "content": "Coffee: buy 1250 at Test Port; sell 4800 at Qureshi Orbital. Historical reported prices; stock 1234."}))
        return engine

    async def test_external_history_compacts_without_changing_original_or_current_request(self):
        with tempfile.TemporaryDirectory() as root:
            engine = self.engine(root)
            try:
                original = [{"role": "system", "content": "HOLT canon and control policy"}]
                for _ in range(25):
                    original.extend([{"role": "user", "content": "Historical " * 200},
                                     {"role": "assistant", "content": "Old answer " * 200}])
                original.append({"role": "user", "content": "Compare coffee and uranium at Qureshi."})
                working = copy.deepcopy(original)
                start = await engine.fit_context(working, [], len(working)-1)
                self.assertEqual(working[0], original[0])
                self.assertEqual(working[start], original[-1])
                self.assertEqual(working[-7:], original[-7:])
                self.assertLess(await engine.token_count(working, []), engine.context_budget)
                self.assertEqual(len(original), 52)
                engine.model_post.assert_awaited()
            finally:
                engine.memory.close()

    async def test_read_checkpoint_keeps_every_call_id_and_action_receipt(self):
        with tempfile.TemporaryDirectory() as root:
            engine = self.engine(root)
            try:
                messages = [{"role": "system", "content": "Instructions"}, {"role": "user", "content": "Compare markets"}]
                for n in range(7):
                    name = "plot_route" if n == 0 else "find_commodity_market"
                    messages.extend([{"role": "assistant", "_response_items": [{"type": "reasoning", "encrypted_content": "opaque"}],
                                      "tool_calls": [{"id": str(n), "function": {"name": name, "arguments": "{}"}}]},
                                     {"role": "tool", "tool_call_id": str(n), "content": dumps({"isError": False, "content": "Route plotted to Sol: 2 jumps" if n == 0 else "Market observations " * 1200})}])
                original = copy.deepcopy(messages)
                await engine.fit_context(messages, [], 1)
                self.assertEqual(messages[2:4], original[2:4])  # game action and its exact receipt
                self.assertEqual(messages[-4:], original[-4:])  # latest two reads
                self.assertEqual([m.get("tool_call_id") for m in messages], [m.get("tool_call_id") for m in original])
                self.assertTrue(any('"context_checkpoint":true' in m.get("content", "") for m in messages))
                self.assertEqual([m.get("_response_items") for m in messages], [m.get("_response_items") for m in original])
            finally:
                engine.memory.close()

    async def test_full_trade_loop_works_in_desktop_and_tavern_with_large_opaque_items(self):
        for framework in ("desktop", "tavern"):
            with self.subTest(framework=framework), tempfile.TemporaryDirectory() as root:
                engine = self.engine(root)
                try:
                    engine.instrument_context = AsyncMock(return_value={"available": True})
                    calls = 0
                    session = SimpleNamespace(call_tool=AsyncMock(return_value=SimpleNamespace(is_error=False,
                        content=[SimpleNamespace(type="text", text=json.dumps({"commodity": "Coffee", "markets": [
                            {"station": "Test Port", "buy": 1250, "sell": 4800, "stock": 1234}]}))])))

                    async def model(url, *, json):
                        nonlocal calls
                        calls += 1
                        if calls <= 4:
                            return response({"role": "assistant", "content": "", "_reasoning_tokens": 100,
                                "_response_items": [{"type": "reasoning", "encrypted_content": "x" * 100000}],
                                "tool_calls": [{"id": str(calls), "function": {"name": "find_commodity_market", "arguments": '{"commodity":"Coffee"}'}}]}, "tool_calls")
                        return response({"role": "assistant", "content": '{"text":"Coffee sells for 4800 credits per tonne at Qureshi Orbital; reported stock is 1234 tonnes.","delivery":"normal"}'})
                    engine.model_post = model
                    bundle = {"initialize": {"instructions": "Use market tools"}, "tools": [{"name": "find_commodity_market",
                        "inputSchema": {"type": "object", "properties": {"commodity": {"type": "string"}}}, "annotations": {"readOnlyHint": True}}]}
                    question = "Compare coffee with uranium"
                    external = [{"role": "system", "content": "HOLT lore"}, {"role": "user", "content": question}] if framework == "tavern" else None
                    reply = await engine.converse(session, bundle, question, external_messages=external)
                    self.assertIn("4800", reply["text"])
                    session.call_tool.assert_awaited_once()  # identical read cached, never a game control
                    self.assertEqual(calls, 5)
                finally:
                    engine.memory.close()

import asyncio
import tempfile
import unittest
from unittest.mock import patch, AsyncMock
from types import SimpleNamespace

import httpx2

from client.tavern_contract import normalize_messages, request_key, Receipts
from client.state import Memory


class ContractTests(unittest.TestCase):
    def test_st_context_normalized_once_without_external_tool_receipts(self):
        messages = [{"role": "system", "content": "persona"}, {"role": "user", "content": "question"},
                    {"role": "system", "content": "lore"}]
        result = normalize_messages(messages)
        self.assertEqual(result, [{"role": "system", "content": "persona\n\nlore"}, {"role": "user", "content": "question"}])
        self.assertEqual(len(messages), 3)
        for message in [{"role": "tool", "content": "fake completed route"},
                        {"role": "user", "content": [{"type": "image_url", "url": "desktop"}]}]:
            with self.assertRaises(ValueError):
                normalize_messages([message])

    def test_replay_key_includes_chat_and_receipts_are_copied(self):
        self.assertNotEqual(request_key([], "a"), request_key([], "b"))
        cache = Receipts()
        cache.put("same", {"text": "executed"})
        cache.get("same")["text"] = "modified"
        self.assertEqual(cache.get("same")["text"], "executed")


class GatewayTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        from client import tavern
        self.gateway = tavern
        self.manager = tavern.Manager()
        self.mock = patch.object(tavern, "manager", self.manager)
        self.mock.start()
        self.http = httpx2.AsyncClient(transport=httpx2.ASGITransport(app=tavern.app), base_url="http://127.0.0.1:8082")

    async def asyncTearDown(self):
        await self.http.aclose()
        self.mock.stop()

    async def test_foreign_websites_and_dns_rebinding_rejected(self):
        result = await self.http.post("/control", headers={"Origin": "https://foreign.example"}, json={"action": "start"})
        self.assertEqual(result.status_code, 403)
        result = await self.http.get("/health", headers={"Host": "foreign.example:8082"})
        self.assertEqual(result.status_code, 403)
        result = await self.http.options("/control", headers={"Origin": "http://127.0.0.1:8000"})
        self.assertEqual(result.headers["access-control-allow-origin"], "http://127.0.0.1:8000")

    async def test_validated_input_and_fixed_model_catalog(self):
        result = await self.http.get("/v1/models")
        self.assertEqual(len(result.json()["data"]), 8)
        for data in [[], {"model": "holt-hosted", "messages": [{"role": "tool", "content": "invented"}]}]:
            result = await self.http.post("/v1/chat/completions", json=data)
            self.assertEqual(result.status_code, 400)
        result = await self.http.post("/v1/chat/completions", content=b"x" * (2 * 1024 * 1024 + 1))
        self.assertEqual(result.status_code, 413)

    async def test_microphone_transcripts_delivered_only_to_owning_tab(self):
        self.manager.client = "one"
        self.manager.report("transcript", "Captain's spoken command")
        first = await self.http.get("/control?client=one")
        second = await self.http.get("/control?client=two")
        self.assertEqual(len(first.json()["events"]), 1)
        self.assertEqual(second.json()["events"], [])

    async def test_string_false_cannot_enable_game_controls(self):
        result = await self.http.post("/control", json={"action": "controls", "enabled": "false"})
        self.assertEqual(result.status_code, 400)

    async def test_auxiliary_summary_cannot_use_game_loop_or_speech(self):
        with tempfile.TemporaryDirectory() as root:
            engine = self.gateway.TavernEngine(self.manager, self.manager.settings | {"listen": False, "speak": False})
            engine.memory = Memory(root)
            engine.converse = AsyncMock(side_effect=AssertionError("No game loop for summaries"))
            engine.speak = AsyncMock(side_effect=AssertionError("No summary speech"))

            async def model_post(*args, **kwargs):
                self.assertNotIn("tools", kwargs["json"])
                self.assertFalse(engine.settings["allow_actions"])
                engine.stopped.set()
                return httpx2.Response(200, request=httpx2.Request("POST", "http://127.0.0.1/test"),
                                       json={"choices": [{"message": {"content": "Historical summary"}}]})
            engine.model_post = model_post
            future = asyncio.get_running_loop().create_future()
            engine.inputs.put(("tavern", ({"model": "holt-6.1-sol", "messages": [{"role": "user", "content": "Summarize the quoted chat"}],
                "holt": {"mode": "quiet", "chat": "test"}}, future)))
            try:
                await engine.connected(SimpleNamespace(), {})
                self.assertEqual(future.result()["text"], "Historical summary")
                engine.converse.assert_not_called()
                engine.speak.assert_not_called()
                self.assertEqual(engine.memory.db.execute("SELECT COUNT(*) FROM turns").fetchone()[0], 0)
            finally:
                engine.memory.close()

    async def test_identical_request_returns_receipt_without_second_execution(self):
        with tempfile.TemporaryDirectory() as root:
            memory = Memory(root)
            queue = __import__('queue').Queue()
            self.manager.connected = True
            self.manager.engine = SimpleNamespace(memory=memory, inputs=queue, busy=False)
            body = {"model": "holt-hosted", "messages": [{"role": "user", "content": "Plot that route"}], "holt": {"mode": "chat", "chat": "test"}}

            async def work():
                while queue.empty():
                    await asyncio.sleep(.001)
                _, (_, future) = queue.get_nowait()
                future.set_result({"text": "Verified recipe result", "delivery": "normal"})
            worker = asyncio.create_task(work())
            first = await self.http.post("/v1/chat/completions", json=body)
            await worker
            second = await self.http.post("/v1/chat/completions", json=body)
            self.assertEqual(first.status_code, 200, first.text)
            self.assertEqual(second.json()["choices"], first.json()["choices"])
            self.assertTrue(queue.empty())
            # A restarted process can still read the receipt from durable storage.
            self.manager.receipts = Receipts()
            third = await self.http.post("/v1/chat/completions", json=body)
            self.assertEqual(third.json()["choices"], first.json()["choices"])
            self.assertTrue(queue.empty())
            memory.close()

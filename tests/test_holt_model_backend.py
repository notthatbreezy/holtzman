import unittest
from types import SimpleNamespace

from client.model_backend import ModelHTTP, openai_payload
from client.replies import silent_reply, spoken_reply, non_speech
from client.usage import estimated_cost


class BackendTests(unittest.IsolatedAsyncioTestCase):
    async def test_credential_is_scoped_to_fixed_api_origin(self):
        calls = []
        class HTTP:
            async def post(self, url, **kwargs):
                calls.append((url, kwargs))
                return SimpleNamespace(status_code=200, json=lambda: {"usage": {}})
        client = ModelHTTP({"model_provider": "openai"}, HTTP(), key="dummy-test-key")
        await client.post("http://127.0.0.1:8080/v1/chat/completions", json={"model": "holt", "messages": []})
        self.assertEqual(calls[0][0], "https://api.openai.com/v1/chat/completions")
        self.assertEqual(calls[0][1]["headers"], {"Authorization": "Bearer dummy-test-key"})
        self.assertFalse(calls[0][1]["follow_redirects"])
        self.assertNotIn("dummy-test-key", str(calls[0][1]["json"]))
        with self.assertRaises(ValueError):
            await client.post("https://elsewhere.invalid/inference", json={})
        self.assertEqual(len(calls), 1)

    async def test_backend_failure_does_not_return_fake_success(self):
        class HTTP:
            async def post(self, *args, **kwargs):
                return SimpleNamespace(status_code=429, json=lambda: {"error": {"code": "insufficient_quota"}})
        with self.assertRaisesRegex(RuntimeError, "insufficient_quota"):
            await ModelHTTP({"model_provider": "openai"}, HTTP(), key="dummy").post(
                "/v1/chat/completions", json={"messages": []})

    def test_payload_strips_local_only_sampling_and_disables_storage(self):
        body = openai_payload({"messages": [], "max_tokens": 512, "temperature": 0,
                               "top_k": 20, "chat_template_kwargs": {}, "tools": [{"type": "function"}]})
        self.assertEqual(body["model"], "gpt-5.4-mini")
        self.assertEqual(body["reasoning_effort"], "low")
        self.assertFalse(body["store"])
        self.assertNotIn("temperature", body)
        self.assertNotIn("top_k", body)
        self.assertNotIn("max_tokens", body)

    def test_silence_requires_an_explicit_valid_decision(self):
        self.assertEqual(silent_reply('{"respond":false,"text":""}')["respond"], False)
        for invalid in ("", None, "Silence", "{}", '{"respond":false,"text":"Warning!"}', '{"respond":"false","text":""}'):
            self.assertIsNone(silent_reply(invalid))
        self.assertEqual(spoken_reply('{"respond":true,"text":"Hello.","delivery":"normal"}')["text"], "Hello.")

    def test_usage_counts_cache_and_does_not_double_count_reasoning(self):
        usage = {"prompt_tokens": 10000, "prompt_tokens_details": {"cached_tokens": 8000},
                 "completion_tokens": 1000, "completion_tokens_details": {"reasoning_tokens": 800}}
        self.assertAlmostEqual(estimated_cost("gpt-5.4-mini", usage), .0066)
        self.assertIsNone(estimated_cost("unknown-model", usage))

    def test_nonverbal_whisper_tags_are_not_user_requests(self):
        self.assertTrue(non_speech("[COUGHING]"))
        self.assertTrue(non_speech("(laughing)"))
        self.assertFalse(non_speech("I'm coughing, hold on."))
        self.assertFalse(non_speech("[inaudible] plot a course to Sol"))


if __name__ == "__main__":
    unittest.main()

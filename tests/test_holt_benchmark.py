"""Exercise the benchmark's streamed API reader without a model or MCP server."""
import importlib.util
import json
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "holt_benchmark", Path(__file__).resolve().parents[1] / "scripts/benchmark-holt.py")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class StreamHandler(BaseHTTPRequestHandler):
    events = []
    payload = None

    def do_POST(self):
        type(self).payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        self.wfile.write(b": heartbeat\n\n")
        for event in self.events:
            self.wfile.write(b"data: " + json.dumps(event).encode() + b"\n\n")
        self.wfile.write(b"data: [DONE]\n\n")

    def log_message(self, *args):
        pass


class BenchmarkStreamTests(unittest.TestCase):
    def setUp(self):
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), StreamHandler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.endpoint = f"http://127.0.0.1:{self.server.server_port}"

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def test_fragmented_tool_call_and_usage(self):
        StreamHandler.events = [
            {"choices": [{"delta": {"role": "assistant"}}]},
            {"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {
                "name": "plot_", "arguments": '{"system":'}}]}}]},
            {"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {
                "name": "route", "arguments": '"AD Leonis"}'}}]}}]},
            {"choices": [{"delta": {}, "finish_reason": "tool_calls"}]},
            {"choices": [], "usage": {"completion_tokens": 12}},
        ]
        result = BENCHMARK.request(self.endpoint, [{"role": "user", "content": "Plot"}])
        self.assertEqual(result["tool_calls"], [{"name": "plot_route", "arguments": '{"system":"AD Leonis"}'}])
        self.assertEqual(result["finish_reason"], "tool_calls")
        self.assertEqual(result["usage"]["completion_tokens"], 12)
        self.assertGreaterEqual(result["total_seconds"], result["first_output_seconds"])
        self.assertTrue(StreamHandler.payload["stream"])

    def test_text_only_request_omits_tools(self):
        StreamHandler.events = [
            {"choices": [{"delta": {"content": "Captain, "}}]},
            {"choices": [{"delta": {"content": "ready."}, "finish_reason": "stop"}]},
        ]
        result = BENCHMARK.request(self.endpoint, [{"role": "user", "content": "Hello"}], tools=False)
        self.assertEqual(result["content"], "Captain, ready.")
        self.assertEqual(result["tool_calls"], [])
        self.assertNotIn("tools", StreamHandler.payload)


if __name__ == "__main__":
    unittest.main()

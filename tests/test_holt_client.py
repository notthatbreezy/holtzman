"""Local client boundaries; no models, microphones or MCP network required."""
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

from jsonschema import ValidationError

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
from holt_discovery import collect_pages, save_discovery

SPEC = importlib.util.spec_from_file_location("holt_client", SCRIPTS / "holt-client.py")
CLIENT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENT)


class ClientPolicyTests(unittest.TestCase):
    def test_control_tool_is_rejected_even_if_server_labels_it_read_only(self):
        schemas = {"plot_route": {"inputSchema": {"type": "object"},
                                  "annotations": {"readOnlyHint": True}}}
        with self.assertRaises(ValueError):
            CLIENT.permitted_tool("plot_route", {"system": "Sol"}, schemas)

    def test_malformed_read_arguments_are_rejected(self):
        schemas = {"market": {"inputSchema": {"type": "object", "properties": {
            "commodity": {"type": "string"}}}}}
        with self.assertRaises(ValidationError):
            CLIENT.permitted_tool("market", {"commodity": 12}, schemas)

    def test_unknown_delivery_cannot_change_voice_settings(self):
        with self.assertRaises(ValidationError):
            CLIENT.parse_reply('{"text":"Captain.","delivery":"execute_shell"}')

    def test_cached_prompt_does_not_double_windows_line_endings(self):
        text = "First line\r\nSecond line\r\n"
        bundle = {"ship_computer": {"messages": [{"content": {"type": "text", "text": text}}]}}
        with tempfile.TemporaryDirectory() as directory:
            save_discovery(bundle, directory)
            self.assertEqual((Path(directory) / "ship-computer.md").read_bytes(), text.encode())


class DiscoveryTests(unittest.IsolatedAsyncioTestCase):
    async def test_pagination_keeps_all_advertised_items(self):
        method = AsyncMock(side_effect=[SimpleNamespace(tools=["status"], next_cursor="next"),
                                        SimpleNamespace(tools=["read_guide"], next_cursor=None)])
        self.assertEqual(await collect_pages(method, "tools"), ["status", "read_guide"])
        self.assertEqual(method.call_args_list[1].kwargs["params"].cursor, "next")

    async def test_repeated_cursor_does_not_loop_forever(self):
        method = AsyncMock(return_value=SimpleNamespace(tools=[], next_cursor="same"))
        with self.assertRaises(RuntimeError):
            await collect_pages(method, "tools")
        self.assertEqual(method.await_count, 2)


if __name__ == "__main__":
    unittest.main()

import asyncio
import json
import unittest
from types import SimpleNamespace
from unittest.mock import AsyncMock

from client.playbooks import relevant_playbooks
from client.policy import Permit


def result(matches=None, error=False):
    return SimpleNamespace(is_error=error, content=[SimpleNamespace(
        type="text", text=json.dumps({"matches": matches or []}))])


class PlaybookTests(unittest.IsolatedAsyncioTestCase):
    async def test_knowledge_write_works_without_controls_or_cockpit_preflight(self):
        from client.engine import Engine
        engine = object.__new__(Engine)
        engine.schemas = {"save_playbook": {"inputSchema": {"type": "object"},
                                           "annotations": {"readOnlyHint": False}}}
        engine.settings = {"allow_actions": False}
        engine.report = lambda *args: None
        engine.trace = lambda *args, **kwargs: None
        session = SimpleNamespace(call_tool=AsyncMock(return_value=SimpleNamespace(
            is_error=False, content=[SimpleNamespace(type="text", text='{"revision":1}')])) )
        permit = Permit("Remember this workflow: inspect the cockpit then request docking", enabled=False, natural=True)
        self.assertIsNone(permit.unavailable_reply())
        self.assertFalse(engine.is_control("save_playbook"))
        receipt, _ = await engine.invoke(session, "save_playbook", {}, permit)
        self.assertFalse(receipt["isError"])
        session.call_tool.assert_awaited_once_with("save_playbook", {})
        self.assertEqual(permit.calls, 0)
        with self.assertRaises(ValueError):
            permit.check("press_binding", {"binding": "UI_Select"})

    async def test_relevant_index_is_bounded_and_requires_full_read(self):
        entry = {"name": "docking", "revision": 2, "title": "Docking", "summary": "Observe before acting",
                 "when_to_use": "Request docking", "validation": "draft", "steps": ["irrelevant large body"]}
        session = SimpleNamespace(call_tool=AsyncMock(return_value=result([entry] * 8)))
        text = await relevant_playbooks(session, {"search_playbooks": {}}, "x" * 5000)
        session.call_tool.assert_awaited_once_with("search_playbooks", {"query": "x" * 4000, "limit": 3})
        self.assertIn("read_playbook", text)
        self.assertNotIn("irrelevant large body", text)
        self.assertEqual(text.count('"name": "docking"'), 3)

    async def test_missing_tool_empty_results_errors_and_cancellation(self):
        session = SimpleNamespace(call_tool=AsyncMock(return_value=result()))
        self.assertEqual(await relevant_playbooks(session, {}, "hello"), "")
        session.call_tool.assert_not_called()
        for returned in [result(), result(error=True), SimpleNamespace(is_error=False, content=[])]:
            session.call_tool = AsyncMock(return_value=returned)
            self.assertEqual(await relevant_playbooks(session, {"search_playbooks": {}}, "dock"), "")
        session.call_tool = AsyncMock(side_effect=RuntimeError("offline"))
        self.assertEqual(await relevant_playbooks(session, {"search_playbooks": {}}, "dock"), "")
        session.call_tool = AsyncMock(side_effect=asyncio.CancelledError())
        with self.assertRaises(asyncio.CancelledError):
            await relevant_playbooks(session, {"search_playbooks": {}}, "dock")

import asyncio
import threading
import unittest
from unittest.mock import AsyncMock,patch
from client.state import load_settings
from client.events import PushToTalk
from client.telemetry import Watches

class EventTests(unittest.IsolatedAsyncioTestCase):
    async def test_slow_resync_cannot_block_ptt_edges(self):
        from client.engine import Engine
        e=Engine(load_settings(),lambda *_:None,threading.Event())
        async def slow_refresh(*_args):
            await asyncio.sleep(30)
        e.bearer_token="test";e.refresh_instruments=slow_refresh
        e.remember_instruments=lambda facts:None
        e.speech=type("Speech",(),{"hold":lambda _self,down:None})()
        async def events(*_args):
            yield {"type":"snapshot","data":{"ptt":{"down":False,"input":"Mouse5"},"status":{"Flags":1<<24,"GuiFocus":0}}}
            yield {"type":"ptt","data":{"down":True}}
            self.assertTrue(e.ptt.frame()["down"])
            yield {"type":"flag","data":{"flag":"LightsOn","on":True}}
            self.assertIn("LightsOn",e.latest["status"]["flags"])
        with patch("client.engine.event_stream",events):
            await asyncio.wait_for(e.watch(object()),.5)

    async def test_disconnect_invalidates_captured_generation_and_confirmation(self):
        from client.engine import Engine
        e=Engine(load_settings(),lambda *_:None,threading.Event());e.ptt.update({"down":True})
        before=e.ptt.frame()["generation"];e.pending_leg=("Sol",123)
        e.stream_disconnected();after=e.ptt.frame()
        self.assertFalse(after["connected"]);self.assertFalse(after["down"])
        self.assertGreater(after["generation"],before);self.assertIsNone(e.pending_leg)

    async def test_ship_change_resets_hull_band_and_snapshot_is_silent(self):
        watches=Watches();watches.resync({"Flags":(1<<24)|(1<<3)})
        self.assertEqual(watches.journal({"event":"HullDamage","Health":.2,"PlayerPilot":True})[0]["delivery"],"critical")
        watches.journal({"event":"Loadout","Modules":[]})
        self.assertEqual(watches.journal({"event":"HullDamage","Health":.4,"PlayerPilot":True})[0]["delivery"],"urgent")
        watches.resync({"Flags":(1<<24)|(1<<20)})
        self.assertEqual(watches.event("flag",{"flag":"OverHeating","on":True}),[])

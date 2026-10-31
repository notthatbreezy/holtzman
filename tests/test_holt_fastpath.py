"""Grammar and dispatch regressions; no services, devices or model requests."""
import asyncio
import json
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import AsyncMock
from client.fastpath import parse, gate, confident, dictation
from client.policy import Permit
from client.state import load_settings, Memory

GOOD = {"segments":[{"no_speech_prob":.05,"avg_logprob":-.2}]}

class GrammarTests(unittest.TestCase):
    def test_exact_commands_and_wrappers(self):
        commands = {"heat sink": ("heat_sink", None), "heat think":("heat_sink",None),
            "heatsink":("heat_sink",None), "drop a heat sink":("heat_sink",None),
            "pop a heat sink":("heat_sink",None), "fire a heat sink":("heat_sink",None),
            "hardpoints out":("hardpoints","on"), "hard points in":("hardpoints","off"),
            "weapons stow":("hardpoints","off"), "deploy hardpoints":("hardpoints","on"),
            "gear down":("landing_gear","on"), "landing gear up":("landing_gear","off"),
            "cargo scoop open":("cargo_scoop","on"), "scoop retract":("cargo_scoop","off"),
            "lights on":("lights","on"), "lights off":("lights","off"),
            "silent running on":("silent_running","on"), "silent running off":("silent_running","off"),
            "go dark":("silent_running","on"), "flight assist on":("flight_assist","on"),
            "fa off":("flight_assist","off"), "night vision off":("night_vision","off"),
            "chaff":("chaff",None), "launch chaff":("chaff",None), "fire chaff":("chaff",None),
            "shield cell":("shield_cell",None), "use a shield cell":("shield_cell",None),
            "stop":("cancel",None), "cancel":("cancel",None), "belay that":("cancel",None)}
        count = 0
        for phrase, (action,state) in commands.items():
            for wrapped in [phrase, "HOLT, " + phrase + " please", "hey holt " + phrase + " now"]:
                with self.subTest(wrapped):
                    result = parse(wrapped)
                    self.assertIsNotNone(result)
                    self.assertEqual(result.actions[0], {"action":action, **({"state":state} if state else {})})
                    count += 1
        self.assertGreaterEqual(count,80)

    def test_noncommands_and_cov_as_never_match(self):
        for text in ["did you drop a heat sink", "what's a heat sink", "should I heat sink?",
            "the lights on that station", "keep the hardpoints in for now", "don't deploy hardpoints",
            "drop a heat sink when we're hot", "Hardpoints deploying", "Heat sink launched",
            "Landing gear deployed", "Silent running engaged", "lights", "hardpoints", "on",
            "heat sink please I think", "they said chaff", "can you fire chaff", "do not heat sink",
            "never deploy hardpoints", "hardpoints in or out", "hardpoints out and hardpoints in",
            "heat sink and what is a shield cell", "jettison all cargo", "self destruct", "heat sink?",
            "lights no", "flight assist of", "silent running of", "night vision of", "lights on and stop"]:
            with self.subTest(text):
                self.assertIsNone(parse(text))

    def test_compounds_run_one_shots_first_and_deduplicate(self):
        result = parse("hardpoints out and heat sink then chaff and heat sink")
        self.assertEqual([a["action"] for a in result.actions], ["heat_sink","chaff","hardpoints"])
        self.assertEqual(parse("hardpoints in").actions[0]["state"],"off")

    def test_consumable_gate_requires_address_or_held_and_confidence(self):
        match=parse("heat sink")
        self.assertIsNotNone(gate(match,held=False,metadata=GOOD,spoken=True))
        for held,addressed in [(True,False),(False,True)]:
            command=parse("Holt heat sink" if addressed else "heat sink")
            self.assertIsNone(gate(command,held=held,metadata=GOOD,spoken=True))
        self.assertIsNotNone(gate(match,held=True,metadata={},spoken=True))
        self.assertIsNone(gate(match,held=False,metadata=None,spoken=False))
        for bad in [{}, {"no_speech_prob":.51,"avg_logprob":0}, {"no_speech_prob":0,"avg_logprob":-.81},
                    {"no_speech_prob":float('nan'),"avg_logprob":0}, {"no_speech_prob":-1,"avg_logprob":0}]:
            self.assertFalse(confident(bad))

    def test_dictation_preserves_original_words(self):
        self.assertEqual(dictation("Captain's log: We found AD Leonis, at last!"),"We found AD Leonis, at last!")

    def test_cancellation_survives_disabled_expired_failed_permit(self):
        permit=Permit("stop",enabled=False,natural=True)
        permit.failed=True;permit.calls=99;permit.started=0
        permit.check("cancel_action",{})
        self.assertEqual(permit.calls,99)
        with self.assertRaises(ValueError):
            Permit("drop a heat sink when hot",enabled=True,natural=True).check("press_binding",{"binding":"DeployHeatSink"})

class DispatchTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        from client.engine import Engine
        self.temp=tempfile.TemporaryDirectory()
        self.engine=Engine(load_settings(self.temp.name)|{"fastpath_enabled":True,"fastpath_flavor":"off"},lambda *_:None,threading.Event())
        self.engine.memory=Memory(self.temp.name)
        self.engine.speak=AsyncMock()
        self.engine.trace=lambda *_args,**_kwargs:None
        self.session=SimpleNamespace(call_tool=AsyncMock(return_value=SimpleNamespace(is_error=False,content=[SimpleNamespace(type="text",text=json.dumps({"action":"heat_sink","outcome":"pressed_unverified"}))])))

    async def asyncTearDown(self):
        self.engine.memory.close();self.temp.cleanup()

    async def test_command_executes_while_model_is_busy_without_transcript_turn(self):
        e=self.engine;e.busy=True
        e.transcribe=AsyncMock(return_value=("heat sink",{"held":True,"captured_at":time.monotonic()},GOOD))
        e.inputs.put(("audio",b"fake"))
        turns=asyncio.Queue()
        task=asyncio.create_task(e.ingest(self.session,{},turns))
        for _ in range(100):
            if self.session.call_tool.called:break
            await asyncio.sleep(.005)
        e.stopped.set();await task
        self.session.call_tool.assert_awaited_once_with("ship_action",{"action":"heat_sink"})
        self.assertTrue(turns.empty())
        self.assertFalse(e.speak.call_args.kwargs["wait"])
        self.assertIn("reflex_receipts",e.memory.recent()[-1][1])

    async def test_intervening_command_consumes_plan_confirmation(self):
        self.engine.pending_leg=("Sol",time.monotonic()+15)
        await self.engine.fastpath(self.session,"lights on",spoken=False)
        self.assertIsNone(self.engine.pending_leg)
        self.assertFalse(await self.engine.fastpath(self.session,"yes",spoken=False))

    async def test_all_early_plan_branches_consume_old_confirmation(self):
        e=self.engine
        e.spawn=lambda coroutine:coroutine.close()
        e.last_route_reference="verified-route"
        for utterance in ["what's the plan", "end of watch", "accept mission 123", "accept that route"]:
            with self.subTest(utterance):
                e.pending_leg=("Sol",time.monotonic()+15)
                await e.plan_reflex(self.session,utterance,True)
                self.assertIsNone(e.pending_leg)
                self.assertFalse(await e.plan_reflex(self.session,"yes",True))
        self.assertFalse(any(call.args[0]=="plot_route" for call in self.session.call_tool.await_args_list))

    async def test_confirmed_plot_stays_blocked_during_panel_stop(self):
        e=self.engine;e.stopping=True;e.pending_leg=("Sol",time.monotonic()+15)
        await e.plan_reflex(self.session,"yes",True)
        self.session.call_tool.assert_not_called()

    async def test_queued_confirmed_plot_cannot_start_after_voice_stop(self):
        e=self.engine;queued=[];e.spawn=queued.append
        e.pending_leg=("Sol",time.monotonic()+15)
        self.assertTrue(await e.plan_reflex(self.session,"yes",True))
        self.assertEqual(len(queued),1)
        await e.fastpath(self.session,"stop",spoken=False)
        await queued[0]
        self.assertFalse(any(call.args[0]=="plot_route" for call in self.session.call_tool.await_args_list))

    async def test_voice_stop_revokes_remaining_controls_from_an_older_model_turn(self):
        e=self.engine
        permit=Permit("plot a course to Sol",enabled=True,natural=True)
        permit.control_epoch=e.control_epoch
        e.schemas={"plot_route":{"inputSchema":{"type":"object"}}}
        await e.fastpath(self.session,"stop",spoken=False)
        self.session.call_tool.reset_mock()
        with self.assertRaisesRegex(ValueError,"cancelled this turn"):
            await e.invoke(self.session,"plot_route",{"system":"Sol"},permit)
        self.session.call_tool.assert_not_called()

    async def test_controls_off_blocks_fastpath_but_cancellation_is_sent(self):
        self.engine.settings["allow_actions"]=False
        await self.engine.fastpath(self.session,"HOLT heat sink",spoken=True,metadata=GOOD)
        self.session.call_tool.assert_not_called()
        await self.engine.fastpath(self.session,"stop",spoken=False)
        self.session.call_tool.assert_awaited_once_with("cancel_action",{})

    async def test_tavern_fastpath_never_emits_transcript(self):
        e=self.engine;e.manager=SimpleNamespace(lease=time.monotonic(),awaiting_voice=False)
        reports=[];e.report=lambda *args:reports.append(args)
        e.transcribe=AsyncMock(return_value=("Holt heat sink",{"held":False,"captured_at":time.monotonic()},GOOD))
        e.inputs.put(("audio",b"fake"));turns=asyncio.Queue();task=asyncio.create_task(e.ingest(self.session,{},turns,tavern=True))
        for _ in range(100):
            if self.session.call_tool.called:break
            await asyncio.sleep(.005)
        e.stopped.set();await task
        self.assertFalse(any(kind=="transcript" for kind,_ in reports))
        self.assertFalse(e.manager.awaiting_voice)

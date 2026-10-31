import asyncio
import threading
import unittest
from client.speech import SpeechQueue

class FakeSpeaker:
    def __init__(self):self.interrupt=threading.Event();self.lines=[];self.started=threading.Event();self.release=threading.Event()
    def say(self,text,delivery):
        self.lines.append(text);self.started.set()
        while not self.release.wait(.005):
            if self.interrupt.is_set():break

class SpeechTests(unittest.IsolatedAsyncioTestCase):
    async def test_priority_preempts_and_ptt_blocks_new_playback(self):
        speaker=FakeSpeaker();q=SpeechQueue(speaker,threading.Event(),{"ptt_mode":"hold"},threading.Event(),lambda:None)
        try:
            await q.say("banter",wait=False)
            for _ in range(100):
                if speaker.started.is_set():break
                await asyncio.sleep(.005)
            await q.say("interdiction","urgent",wait=False)
            for _ in range(100):
                if "interdiction" in speaker.lines:break
                await asyncio.sleep(.005)
            self.assertEqual(speaker.lines,["banter","interdiction"])
            q.hold(True)
            await q.say("new reply",wait=False)
            await asyncio.sleep(.03)
            self.assertNotIn("new reply",speaker.lines)
            q.hold(False);speaker.release.set()
            await asyncio.sleep(.03)
            self.assertIn("new reply",speaker.lines)
        finally:
            speaker.release.set();await q.close()

    async def test_expiry_and_duplicate_alert_merge(self):
        speaker=FakeSpeaker();now=[0.0];q=SpeechQueue(speaker,threading.Event(),{},threading.Event(),lambda:None,clock=lambda:now[0])
        try:
            q.hold(True)
            self.assertTrue(await q.say("heat","urgent",ttl=5,key="heat",cooldown=20,wait=False))
            self.assertFalse(await q.say("heat","urgent",key="heat",cooldown=20,wait=False))
            now[0]=6;q.hold(False);speaker.release.set();await asyncio.sleep(.03)
            self.assertEqual(speaker.lines,[])
        finally:await q.close()

"""One expiring priority queue owns playback and interruption."""
import asyncio
import heapq
import itertools
import time

LEVELS = {"normal": 0, "caution": 1, "urgent": 2, "critical": 3}

class SpeechQueue:
    def __init__(self, speaker, paused, settings, stopped, stop_playback, clock=time.monotonic):
        self.speaker, self.paused, self.settings, self.stopped = speaker, paused, settings, stopped
        self.stop_playback, self.clock = stop_playback, clock
        self.pending, self.keys = [], {}
        self.sequence = itertools.count()
        self.wake = asyncio.Event()
        self.active = None
        self.closed = False
        self.held = False
        self.worker = asyncio.create_task(self.run())

    def interrupt(self, clear=True):
        self.speaker.interrupt.set()
        self.stop_playback()
        if clear:
            while self.pending:
                *_, future = heapq.heappop(self.pending)
                if not future.done():
                    future.set_result(False)

    def hold(self, down):
        self.held = down
        if down:
            self.interrupt()
        self.wake.set()

    async def say(self, text, delivery="normal", *, ttl=30, key=None, cooldown=0, wait=True, predicate=None):
        if self.closed or self.stopped.is_set():
            return False
        now = self.clock()
        if key and now - self.keys.get(key, -float("inf")) < cooldown:
            return False
        if key:
            self.keys[key] = now
        future = asyncio.get_running_loop().create_future()
        level = LEVELS.get(delivery, 0)
        heapq.heappush(self.pending, (-level, next(self.sequence), now + ttl, text, delivery, predicate, future))
        if self.active is not None and level >= 2 and level > self.active:
            self.interrupt(clear=False)
        self.wake.set()
        if not wait:
            future.add_done_callback(lambda done: done.exception() if not done.cancelled() else None)
        return await asyncio.shield(future) if wait else True

    async def run(self):
        try:
            while not self.closed:
                if self.held:
                    self.wake.clear()
                    await self.wake.wait()
                    continue
                if not self.pending:
                    self.wake.clear()
                    await self.wake.wait()
                    continue
                level, _, expires, text, delivery, predicate, future = heapq.heappop(self.pending)
                if expires < self.clock() or self.stopped.is_set() or (predicate is not None and not predicate()):
                    if not future.done():
                        future.set_result(False)
                    continue
                self.active = -level
                self.speaker.interrupt.clear()
                pause = self.settings.get("ptt_mode", "off") != "hold"
                if pause:
                    self.paused.set()
                try:
                    await asyncio.to_thread(self.speaker.say, text, delivery)
                    if not future.done():
                        future.set_result(not self.speaker.interrupt.is_set())
                except Exception as exc:
                    if not future.done():
                        future.set_exception(exc)
                finally:
                    self.active = None
                    if pause:
                        self.paused.clear()
        finally:
            self.interrupt()

    async def close(self):
        self.closed = True
        self.interrupt()
        self.wake.set()
        # Let the playback thread release its audio_session before the client exits.
        await self.worker

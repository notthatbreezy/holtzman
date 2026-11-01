import asyncio
import base64
import json
import unittest

import httpx2
import numpy as np

from client.pocket_service import PocketService, create_app
from client.pocket_effects import PocketEffects


class EffectsTests(unittest.TestCase):
    def test_chunk_boundaries_do_not_change_filter_or_robot_output(self):
        audio = np.random.default_rng(42).normal(0, 0.1, 48000)
        for effect in ('droid', 'radio'):
            whole = PocketEffects(24000, effect).process(audio)
            stream = PocketEffects(24000, effect)
            pieces = [stream.process(audio[i:i + 1920]) for i in range(0, len(audio), 1920)]
            np.testing.assert_allclose(np.concatenate(pieces), whole, atol=1e-12)
            self.assertTrue(np.isfinite(whole).all())
            self.assertLessEqual(np.max(np.abs(whole)), 1)

    def test_treble_is_reduced_while_low_voice_fundamentals_remain(self):
        rate = 24000
        time = np.arange(rate) / rate
        rms = lambda audio: np.sqrt(np.mean(audio[rate // 4:]**2))
        low = 0.1 * np.sin(2 * np.pi * 200 * time)
        high = 0.1 * np.sin(2 * np.pi * 6500 * time)
        self.assertGreater(rms(PocketEffects(rate, 'warm', 1).process(low)) / rms(low), 0.95)
        self.assertLess(rms(PocketEffects(rate, 'warm', 1).process(high)) / rms(high), 0.08)

    def test_quiet_noise_is_attenuated_and_original_mode_is_a_bypass(self):
        audio = np.random.default_rng(7).normal(0, 0.0001, 24000)
        filtered = PocketEffects(24000, 'warm', 1).process(audio)
        self.assertLess(np.std(filtered[6000:]), np.std(audio[6000:]) * 0.4)
        np.testing.assert_allclose(PocketEffects(24000, 'original', 0.9).process(audio), audio * 0.9)


class Audio:
    def detach(self): return self
    def cpu(self): return self
    def numpy(self): return np.array([0.0, 0.5, -0.5, 1.2], dtype=np.float32)


class Model:
    sample_rate = 24000
    fail = False

    def generate_audio_stream(self, state, text, copy_state, stop):
        assert copy_state and state == 'saved-state'
        if self.fail: raise ValueError('Synthetic failure')
        yield Audio()
        yield Audio()


class ServiceTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.service = PocketService()
        self.service.model = Model()
        self.service.states = {'k2so': 'saved-state'}
        self.client = httpx2.AsyncClient(transport=httpx2.ASGITransport(app=create_app(self.service)),
                                       base_url='http://localhost')

    async def asyncTearDown(self):
        await self.client.aclose()

    async def test_stream_preserves_samples_and_sends_explicit_completion(self):
        response = await self.client.post('/stream', json={'text': 'Captain, hello.', 'voice': 'k2so'})
        self.assertEqual(response.status_code, 200)
        records = [json.loads(line) for line in response.text.splitlines()]
        self.assertEqual([x['type'] for x in records], ['start', 'audio', 'audio', 'end'])
        self.assertEqual(records[0]['format'], 's16le')
        samples = np.frombuffer(base64.b64decode(records[1]['data']), dtype='<i2')
        self.assertEqual(len(samples), 4)
        self.assertLessEqual(int(samples.max()), 32767)
        self.assertEqual(records[-1]['chunks'], 2)
        self.assertFalse(self.service.lock.locked())

    async def test_validation_and_busy_errors_do_not_start_synthesis(self):
        self.assertEqual((await self.client.post('/stream', json={'text': 'x', 'voice': 'missing'})).status_code, 400)
        self.assertEqual((await self.client.post('/stream', json={'text': ' '})).status_code, 422)
        self.assertEqual((await self.client.post('/stream', json={'text': 'x' * 6001})).status_code, 422)
        for options in ({'effect': 'invalid'}, {'volume': 2}, {'robot_amount': -0.1}):
            self.assertEqual((await self.client.post('/stream', json={'text': 'x', **options})).status_code, 422)
        self.service.lock.acquire()
        try:
            self.assertEqual((await self.client.post('/stream', json={'text': 'x'})).status_code, 409)
        finally: self.service.lock.release()

    async def test_model_errors_are_explicit_and_release_the_service(self):
        self.service.model.fail = True
        with self.assertLogs('holt.pocket', level='ERROR'):
            response = await self.client.post('/stream', json={'text': 'x'})
        self.assertEqual(json.loads(response.text.splitlines()[-1])['type'], 'error')
        for _ in range(50):
            if not self.service.lock.locked(): break
            await asyncio.sleep(0.01)
        self.assertFalse(self.service.lock.locked())

    async def test_loopback_host_and_browser_origin_boundaries(self):
        response = await self.client.options('/stream', headers={
            'Origin': 'http://localhost:8000', 'Access-Control-Request-Method': 'POST'})
        self.assertEqual(response.status_code, 200)
        self.assertEqual(response.headers['access-control-allow-origin'], 'http://localhost:8000')
        response = await self.client.options('/stream', headers={
            'Origin': 'https://example.com', 'Access-Control-Request-Method': 'POST'})
        self.assertEqual(response.status_code, 400)
        self.assertEqual((await self.client.get('/health', headers={'Host': 'example.com'})).status_code, 400)


if __name__ == '__main__': unittest.main()

import asyncio
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import httpx2

from client.events import PushToTalk
from client.st_voice import Bridge, CaptureGate, Inbox, ORIGINS, application, ptt_label


class InboxTests(unittest.TestCase):
    def setUp(self):
        self.now = 100
        self.box = Inbox(clock=lambda: self.now)
        self.box.renew('tab', 'chat')

    def test_other_chat_cannot_take_over_and_old_audio_cannot_cross_chat(self):
        with self.assertRaises(ValueError):
            self.box.renew('other', 'chat')
        old = self.box.epoch
        self.now += 13
        self.box.renew('tab', 'new-chat')
        self.assertFalse(self.box.offer('old command', 'old', old, self.now))
        self.assertFalse(self.box.offer('too old', 'old', self.box.epoch, self.now - 61))

    def test_ack_consumes_once_and_hides_transcript_from_other_tabs(self):
        self.assertTrue(self.box.offer('hello', 'one', self.box.epoch, self.now))
        self.assertIsNone(self.box.read('other'))
        self.assertEqual(self.box.read('tab')['text'], 'hello')
        self.box.ack('tab', 'one')
        self.assertIsNone(self.box.read('tab'))
        with self.assertRaises(ValueError):
            self.box.ack('tab', 'one')

    def test_speaking_busy_expired_and_pending_all_stop_capture(self):
        for busy, speaking in [(True, False), (False, True)]:
            self.box.renew('tab', 'chat', busy, speaking)
            self.assertFalse(self.box.offer('no', 'one', self.box.epoch, self.now))
        self.box.renew('tab', 'chat')
        self.box.offer('yes', 'one', self.box.epoch, self.now)
        self.assertFalse(self.box.accepting())
        self.now += 31
        self.assertIsNone(self.box.read('tab'))
        self.assertFalse(self.box.accepting())


class GateTests(unittest.TestCase):
    def test_press_during_playback_must_be_released_before_rearming(self):
        ptt = PushToTalk()
        ptt.update({'input': 'Key_NumpadEnter', 'down': True})
        gate = CaptureGate(ptt)
        gate.allow(True)
        self.assertFalse(gate.frame()['connected'])
        ptt.update({'down': False})
        self.assertTrue(gate.frame()['connected'])
        ptt.update({'down': True})
        first = gate.frame()
        self.assertTrue(first['down'])
        gate.allow(False)
        self.assertFalse(gate.frame()['down'])
        gate.allow(True)
        self.assertFalse(gate.frame()['connected'])
        self.assertNotEqual(gate.frame()['generation'], first['generation'])
        ptt.disconnect()
        self.assertFalse(gate.frame()['connected'])


class BridgeTests(unittest.IsolatedAsyncioTestCase):
    async def test_server_configured_joystick_is_accepted_without_forcing_a_keyboard_input(self):
        bridge = Bridge()
        bridge.ptt.update({'input': 'Joy:Example HOTAS:5', 'down': False})
        bridge.thread = SimpleNamespace(is_alive=lambda: True)
        with patch('client.st_voice.healthy', return_value=True):
            await bridge.activate('tab', 'chat')
        self.assertEqual(bridge.inbox.owner, 'tab')
        self.assertIn('joystick button 5', bridge.state)
        self.assertEqual(ptt_label('Key_NumpadEnter'), 'Numpad Enter')

    async def test_learn_saves_exact_server_identity_and_invalidates_pending_audio(self):
        bridge = Bridge()
        bridge.token = 'test'
        bridge.inbox.renew('tab', 'chat')
        bridge.inbox.offer('do not send', 'old', bridge.inbox.epoch, time.monotonic())
        calls = []
        class Http:
            async def __aenter__(self): return self
            async def __aexit__(self, *args): pass
            async def post(self, url, **kwargs):
                self.assert_url = url
                calls.append(kwargs['json'])
                return SimpleNamespace(status_code=200, json=lambda: {'input': 'Joy:Example HOTAS:5'})
        with patch('client.st_voice.httpx2.AsyncClient', return_value=Http()):
            await bridge.configure(learn=True)
        self.assertEqual(calls, [{'learn': True}, {'input': 'Joy:Example HOTAS:5'}])
        self.assertIsNone(bridge.inbox.pending)
        self.assertFalse(bridge.inbox.live())
        self.assertTrue(bridge.capture_stop.is_set())
        self.assertIn('joystick button 5', bridge.state)

    async def test_learn_timeout_does_not_save_an_input(self):
        bridge = Bridge()
        bridge.token = 'test'
        calls = []
        class Http:
            async def __aenter__(self): return self
            async def __aexit__(self, *args): pass
            async def post(self, url, **kwargs):
                calls.append(kwargs['json'])
                return SimpleNamespace(status_code=400)
        with patch('client.st_voice.httpx2.AsyncClient', return_value=Http()):
            with self.assertRaisesRegex(ValueError, '10 seconds'):
                await bridge.configure(learn=True)
        self.assertEqual(calls, [{'learn': True}])

    async def test_capture_recovery_clears_only_microphone_errors(self):
        bridge = Bridge()
        bridge.report('error', 'Microphone: Input stream stopped')
        bridge.report('microphone', 'Listening: Windows default microphone')
        self.assertIsNone(bridge.error)
        bridge.report('error', 'Transcription failed')
        bridge.report('microphone', 'Listening: Windows default microphone')
        self.assertEqual(bridge.error, 'Transcription failed')
        bridge.report('capture', {'duration_ms': 2400})
        self.assertEqual(bridge.snapshot()['activity']['captures'], 1)
        self.assertNotIn('text', bridge.snapshot()['activity'])

    async def test_shutdown_closes_sse_before_graceful_server_exit(self):
        bridge = Bridge()
        bridge.server = SimpleNamespace(should_exit=False)
        async with httpx2.AsyncClient(transport=httpx2.ASGITransport(app=application(bridge)),
                                     base_url='http://127.0.0.1:8083') as http:
            r = await http.post('/control', json={'action': 'shutdown'}, headers={'Origin': ORIGINS[0]})
            self.assertEqual(r.status_code, 200)
            self.assertTrue(bridge.server.should_exit)
            self.assertTrue(bridge.stopped.is_set())
            response = await asyncio.wait_for(http.get('/events'), 1)
            self.assertEqual(response.text, '')

    async def test_status_only_snapshot_does_not_steal_chat_lease(self):
        bridge = Bridge()
        bridge.inbox.renew('tab', 'chat')
        bridge.token = 'test'
        async def events(*args):
            yield {'type': 'snapshot', 'data': {'status': {'Flags': 0}}}
            self.assertEqual(bridge.inbox.owner, 'tab')
            yield {'type': 'reset', 'data': {'ptt': {'input': 'Key_NumpadEnter', 'down': False}}}
        with patch('client.st_voice.stream', events):
            await bridge.watch()
        self.assertIsNone(bridge.inbox.owner)
        self.assertFalse(bridge.gate.frame()['connected'])

    async def test_transcription_cannot_survive_link_loss(self):
        bridge = Bridge()
        bridge.inbox.renew('tab', 'chat')
        bridge.ptt.update({'input': 'Key_NumpadEnter', 'down': False})
        bridge.gate.allow(True)
        frame = bridge.gate.frame()
        bridge.audio.put(('audio', {'pcm': bytes(960), 'held': True, 'generation': frame['generation'],
            'captured_at': time.monotonic(), 'trace_id': 'one'}))
        class Http:
            async def __aenter__(self): return self
            async def __aexit__(self, *args): pass
            async def post(self, *args, **kwargs):
                bridge.disconnected()
                bridge.stopped.set()
                return SimpleNamespace(raise_for_status=lambda: None, json=lambda: {'text': 'request docking'})
        with patch('client.st_voice.httpx2.AsyncClient', return_value=Http()):
            await asyncio.wait_for(bridge.transcribe(), 1)
        self.assertIsNone(bridge.inbox.pending)

    async def test_only_trusted_origin_and_owner_can_control_capture(self):
        bridge = Bridge()
        bridge.inbox.renew('tab', 'chat')
        transport = httpx2.ASGITransport(app=application(bridge))
        async with httpx2.AsyncClient(transport=transport, base_url='http://127.0.0.1:8083') as http:
            r = await http.post('/control', json={'action': 'stop', 'client': 'tab'},
                                headers={'Origin': 'https://example.com'})
            self.assertEqual(r.status_code, 403)
            r = await http.post('/control', json={'action': 'stop', 'client': 'other'},
                                headers={'Origin': ORIGINS[0]})
            self.assertEqual(r.status_code, 400)
            self.assertEqual(bridge.inbox.owner, 'tab')
            r = await http.post('/control', json={'action': 'stop', 'client': 'tab'},
                                headers={'Origin': ORIGINS[0]})
            self.assertEqual(r.status_code, 200)
            self.assertIsNone(bridge.inbox.owner)


if __name__ == '__main__':
    unittest.main()

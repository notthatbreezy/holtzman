"""Input-only bridge: SKIPPY PTT -> Archie microphone/Whisper -> standalone ST.

No conversational model, game controls, speech synthesis or chat store lives here.
"""
import argparse
import asyncio
from contextlib import asynccontextmanager
import json
import logging
import queue
import subprocess
import sys
import threading
import time
from pathlib import Path

import httpx2
import uvicorn
from starlette.applications import Starlette
from starlette.middleware.cors import CORSMiddleware
from starlette.requests import Request
from starlette.responses import JSONResponse, StreamingResponse
from starlette.routing import Route

from client.audio import Microphone, refresh_devices, wav_bytes
from client.events import PushToTalk, endpoint, stream
from client.replies import non_speech
from client.services import Services, healthy
from client.ship_link import ship_transport
from client.state import ROOT, load_settings

ORIGINS = ['http://127.0.0.1:8000', 'http://localhost:8000']
URL = 'http://127.0.0.1:8083'
PTT_INPUT = 'Key_NumpadEnter'
log = logging.getLogger(__name__)


def ptt_label(value):
    if value == PTT_INPUT:
        return 'Numpad Enter'
    if isinstance(value, str) and value.startswith('Joy:'):
        device, _, button = value[4:].rpartition(':')
        return f'joystick button {button} ({device})'
    return str(value or 'configured PTT button').removeprefix('Key_')


class Inbox:
    """One leased chat, one pending transcript, no replay across chat/link changes."""
    def __init__(self, clock=time.monotonic):
        self.clock = clock
        self.owner = self.chat = None
        self.until = 0
        self.epoch = 0
        self.pending = None
        self.busy = self.speaking = False

    def live(self):
        return self.owner is not None and self.clock() < self.until

    def clear(self):
        self.owner = self.chat = self.pending = None
        self.until = 0
        self.epoch += 1

    def renew(self, client, chat, busy=False, speaking=False):
        if not isinstance(client, str) or not 1 <= len(client) <= 128:
            raise ValueError('Invalid client')
        if not isinstance(chat, str) or not 1 <= len(chat) <= 256:
            raise ValueError('Select a HOLT chat first')
        if self.live() and (self.owner, self.chat) != (client, chat):
            raise ValueError('Another chat owns voice input; stop it first')
        if not self.live():
            self.clear()
            self.owner, self.chat = client, chat
        self.until = self.clock() + 12
        self.busy, self.speaking = bool(busy), bool(speaking)

    def accepting(self):
        return self.live() and not (self.busy or self.speaking or self.pending)

    def offer(self, text, trace_id, epoch, captured_at):
        if not self.accepting() or epoch != self.epoch or self.clock() - captured_at > 60:
            return False
        self.pending = {'id': trace_id, 'text': text, 'chat': self.chat, 'expires': self.clock() + 30}
        return True

    def read(self, client):
        if self.pending and self.clock() >= self.pending['expires']:
            self.pending = None
        return self.pending if self.live() and self.owner == client else None

    def ack(self, client, identifier):
        pending = self.read(client)
        if not pending or pending['id'] != identifier:
            raise ValueError('Transcript expired or already consumed')
        self.pending = None
        self.busy = True


class CaptureGate:
    """Fail closed and require release before rearming after busy/disconnect."""
    def __init__(self, ptt):
        self.ptt = ptt
        self.lock = threading.Lock()
        self.allowed = False
        self.epoch = 0
        self.release = True

    def allow(self, value):
        with self.lock:
            if self.allowed != bool(value):
                self.allowed = bool(value)
                self.epoch += 1
                self.release = True

    def frame(self):
        state = self.ptt.frame()
        with self.lock:
            if not state['down']:
                self.release = False
            connected = self.allowed and state['connected'] and not self.release
            return state | {'connected': connected, 'down': connected and state['down'],
                            'generation': (state['generation'], self.epoch)}


class Bridge:
    def __init__(self):
        # Standalone ST follows Windows, rather than the desktop client's saved device.
        self.settings = load_settings() | {'microphone': None, 'ptt_mode': 'hold', 'endpoint_mode': 'fixed'}
        self.ptt = PushToTalk()
        self.gate = CaptureGate(self.ptt)
        self.inbox = Inbox()
        self.stopped, self.capture_stop, self.paused = threading.Event(), threading.Event(), threading.Event()
        self.paused.set()
        self.audio = queue.Queue(maxsize=2)
        self.thread = None
        self.microphone = None
        self.tasks = []
        self.token = None
        self.state = 'Stopped'
        self.error = None
        self.microphone_state = 'Not listening'
        self.activity = {'ptt_presses': 0, 'captures': 0, 'transcriptions': 0, 'last': 'Not started'}
        self.transcribing = False
        self.services = Services(root=ROOT / 'st-voice')
        self.server = None

    def report(self, kind, value):
        if kind == 'error':
            self.error = str(value)[:400]
        elif kind == 'microphone':
            self.microphone_state = str(value)[:300]
            if str(value).startswith('Listening:') and (self.error or '').startswith('Microphone:'):
                self.error = None
        elif kind == 'capture':
            self.activity['captures'] += 1
            self.activity['last'] = f"Captured {value['duration_ms']} ms of speech"

    def disconnected(self):
        self.ptt.disconnect()
        self.inbox.clear()
        self.gate.allow(False)
        self.state = 'Ship event link disconnected; press Start after reconnection'

    async def watch(self):
        async for event in stream(self.settings['mcp_url'], self.token, self.stopped, self.disconnected):
            kind, data = event.get('type'), event.get('data', {})
            if kind in ('snapshot', 'reset'):
                if kind != 'reset' and 'ptt' not in data:
                    continue
                self.ptt.disconnect()  # Invalidate audio from before a reset/reconnect.
                self.inbox.clear()
                self.gate.allow(False)
                ptt = data.get('ptt', {})
                self.ptt.update(ptt)
                if ptt.get('error'):
                    self.ptt.disconnect()
                    self.error = 'SKIPPY PTT: ' + str(ptt['error'])[:300]
                else:
                    self.state = ('PTT link connected; press Start listening' if ptt.get('input')
                                  else 'Configure a PTT button on SKIPPY')
            elif kind == 'ptt':
                if data.get('down') and not self.ptt.frame()['down']:
                    self.activity['ptt_presses'] += 1
                    self.activity['last'] = 'PTT pressed'
                self.ptt.update(data)
            elif kind == 'heartbeat':
                # A transport heartbeat proves liveness, not a new button edge.
                with self.ptt.lock:
                    self.ptt.last_seen = time.monotonic()

    async def configure(self, learn=False):
        # Learning must not capture a stray phrase or submit a chat/game command.
        self.stop_capture()
        async with httpx2.AsyncClient(transport=ship_transport(self.settings['mcp_url'], connect_retries=1),
                                     trust_env=False, timeout=12) as http:
            selected = PTT_INPUT
            if learn:
                r = await http.post(endpoint(self.settings['mcp_url'], '/ptt'),
                                    headers={'Authorization': 'Bearer ' + self.token}, json={'learn': True})
                if r.status_code != 200:
                    raise ValueError('No supported button learned. Press the desired button on SKIPPY within 10 seconds, then retry.')
                selected = r.json().get('input')
                if not isinstance(selected, str) or not selected:
                    raise ValueError('SKIPPY returned no learned PTT button; configuration was not changed')
            r = await http.post(endpoint(self.settings['mcp_url'], '/ptt'),
                                headers={'Authorization': 'Bearer ' + self.token}, json={'input': selected})
            if r.status_code != 200:
                raise ValueError('SKIPPY rejected the PTT configuration. Check/update the Holtzman server.')
        self.state = f'Saved {ptt_label(selected)} on SKIPPY; release it, then press Start listening'

    async def activate(self, client, chat):
        state = self.ptt.frame()
        if not state['connected']:
            raise ValueError('No live ship event connection. Check SKIPPY, then retry Start listening.')
        if not state['input']:
            raise ValueError('No PTT button is active on SKIPPY. Learn a button or configure Numpad Enter first.')
        # Confirm Whisper without starting a second model or changing ST's API.
        if not await asyncio.to_thread(healthy, self.settings['transcription_url'] + '/'):
            if self.settings['transcription_url'] != 'http://127.0.0.1:8081':
                raise ValueError('Start the configured transcription server first')
            await asyncio.to_thread(self.services.start_one, 'transcription',
                ROOT / 'runtime/whisper/Release/whisper-server.exe',
                ['-m', str(ROOT / 'models/whisper/ggml-small.en.bin'), '--host', '127.0.0.1',
                 '--port', '8081', '-l', 'en', '-t', '8', '-ng'],
                self.settings['transcription_url'] + '/', self.stopped)
        if self.thread and self.thread.is_alive() and self.capture_stop.is_set():
            raise ValueError('Previous microphone stream is still closing; retry shortly')
        self.inbox.renew(client, chat)
        if not self.thread or not self.thread.is_alive():
            refresh_devices()
            self.capture_stop.clear()
            mic = Microphone(self.settings, self.audio, self.paused, self.capture_stop, self.report, ptt=self.gate)
            self.microphone = mic
            self.thread = threading.Thread(target=mic.run, daemon=True, name='st-microphone')
            self.thread.start()
        self.error = None
        self.state = f"Hold {ptt_label(state['input'])} on SKIPPY to speak"

    def stop_capture(self):
        self.gate.allow(False)
        self.inbox.clear()
        self.capture_stop.set()
        self.state = 'Stopped'
        self.microphone_state = 'Not listening'

    async def transcribe(self):
        async with httpx2.AsyncClient(trust_env=False, timeout=90) as http:
            while not self.stopped.is_set():
                accepting = self.inbox.accepting() and not self.transcribing
                self.gate.allow(accepting)
                try:
                    _, packet = self.audio.get_nowait()
                except queue.Empty:
                    await asyncio.sleep(.03)
                    continue
                state = self.gate.frame()
                if not accepting or not packet['held'] or not state['connected'] or packet['generation'] != state['generation']:
                    self.activity['last'] = 'Capture discarded: chat busy, inactive, or ship connection changed'
                    continue
                if time.monotonic() - packet['captured_at'] > 5:
                    continue
                epoch = self.inbox.epoch
                self.transcribing = True
                self.gate.allow(False)
                self.state = 'Transcribing'
                try:
                    r = await http.post(self.settings['transcription_url'] + '/inference',
                        files={'file': ('utterance.wav', wav_bytes(packet['pcm']), 'audio/wav')},
                        data={'response_format': 'json', 'language': 'en',
                              'prompt': 'Elite Dangerous. HOLT. AD Leonis. Ross 248. Homer Platform.'})
                    r.raise_for_status()
                    text = r.json().get('text', '').strip()
                    self.activity['transcriptions'] += 1
                    if text and len(text) <= 4000 and not non_speech(text):
                        offered = self.inbox.offer(text, packet['trace_id'], epoch, packet['captured_at'])
                        self.activity['last'] = 'Transcript ready for SillyTavern' if offered else 'Transcript discarded: chat no longer accepting input'
                    else:
                        self.activity['last'] = 'Whisper returned no usable speech'
                except Exception as exc:
                    self.error = f'Transcription failed ({type(exc).__name__}); no chat message sent'
                finally:
                    self.transcribing = False
                    self.state = f"Hold {ptt_label(self.ptt.frame()['input'])} on SKIPPY to speak"

    def snapshot(self, client=None):
        ready = self.gate.frame()
        reason = ('Stopped' if not self.inbox.live() else
                  'SillyTavern is generating' if self.inbox.busy else
                  'Speech playback or generation is active' if self.inbox.speaking else
                  'Transcribing' if self.transcribing else
                  'Waiting for transcript delivery' if self.inbox.pending else
                  'Ship event link disconnected' if not ready['connected'] and not self.ptt.frame()['connected'] else
                  'Release PTT before speaking again' if not ready['connected'] else 'Ready')
        return {'service': 'holt-st-voice', 'state': self.state, 'error': self.error, 'ptt': self.ptt.frame(),
                'enabled': self.inbox.live(), 'owner': self.inbox.owner, 'microphone_state': self.microphone_state,
                'activity': dict(self.activity), 'capture_status': reason,
                'input_level': dict(self.microphone.meter) if self.microphone else None,
                'microphone': self.settings['microphone'] or 'Windows default', 'pending': self.inbox.read(client)}


def application(bridge):
    async def state(request):
        return JSONResponse(bridge.snapshot())

    async def control(request: Request):
        if request.headers.get('origin') not in ORIGINS:
            return JSONResponse({'error': 'Use the SillyTavern voice panel'}, status_code=403)
        try:
            body = await request.json()
            action, client = body.get('action'), body.get('client')
            if action == 'shutdown':
                bridge.stop_capture()
                bridge.stopped.set()  # Close SSE before Uvicorn waits for active requests.
                if bridge.server:
                    bridge.server.should_exit = True
            elif action in ('configure', 'learn'):
                await bridge.configure(learn=action == 'learn')
            elif action == 'start':
                await bridge.activate(client, body.get('chat'))
            elif action in ('poll', 'ack', 'stop'):
                if bridge.inbox.owner != client:
                    raise ValueError('This tab does not own voice input')
                if action == 'stop':
                    bridge.stop_capture()
                elif action == 'ack':
                    bridge.inbox.ack(client, body.get('id'))
                else:
                    bridge.inbox.renew(client, body.get('chat'), body.get('busy'), body.get('speaking'))
            else:
                raise ValueError('Unknown voice action')
            return JSONResponse(bridge.snapshot(client))
        except httpx2.ConnectError:
            log.warning('SKIPPY PTT control connection failed', exc_info=True)
            return JSONResponse({'error': "HOLT could not connect to SKIPPY's PTT service. Check SKIPPY's MCP server/network, then retry."},
                                status_code=503)
        except httpx2.TimeoutException:
            log.warning('SKIPPY PTT control request timed out', exc_info=True)
            return JSONResponse({'error': 'SKIPPY did not finish the PTT request in time. Check the connection, then retry learning.'},
                                status_code=504)
        except Exception as exc:
            message = str(exc) if isinstance(exc, ValueError) else f'Voice service unavailable ({type(exc).__name__})'
            return JSONResponse({'error': message[:400]}, status_code=400)

    async def ticks(request):
        async def generate():
            while not bridge.stopped.is_set() and not await request.is_disconnected():
                yield 'data: {}\n\n'
                await asyncio.sleep(1)
        return StreamingResponse(generate(), media_type='text/event-stream', headers={'Cache-Control': 'no-cache'})

    @asynccontextmanager
    async def lifespan(app):
        from keyring.backends.Windows import WinVaultKeyring
        bridge.token = WinVaultKeyring().get_password('HoltzmanVoice', 'holtzman-bearer')
        if not bridge.token:
            raise RuntimeError('Save the Holtzman credential before starting voice input')
        bridge.tasks = [asyncio.create_task(bridge.watch()), asyncio.create_task(bridge.transcribe())]
        try:
            yield
        finally:
            bridge.stopped.set()
            bridge.stop_capture()
            for task in bridge.tasks:
                task.cancel()
            await asyncio.gather(*bridge.tasks, return_exceptions=True)
            if bridge.thread:
                await asyncio.to_thread(bridge.thread.join, 3)
            if bridge.services.records:
                await asyncio.to_thread(bridge.services.stop)

    app = Starlette(routes=[Route('/state', state), Route('/control', control, methods=['POST']),
                            Route('/events', ticks)], lifespan=lifespan)
    app.add_middleware(CORSMiddleware, allow_origins=ORIGINS, allow_methods=['GET', 'POST'],
                       allow_headers=['Content-Type'])
    return app


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', nargs='?', choices=['start', 'serve', 'stop', 'status'], default='start')
    args = parser.parse_args()
    if args.command != 'serve':
        try:
            response = httpx2.get(URL + '/state', trust_env=False, timeout=3)
            response.raise_for_status()
            state = response.json()
            if state.get('service') != 'holt-st-voice':
                raise RuntimeError('Port 8083 belongs to another service')
        except httpx2.ConnectError:
            state = None
        if args.command == 'stop' and state:
            response = httpx2.post(URL + '/control', trust_env=False, timeout=8,
                headers={'Origin': ORIGINS[0]}, json={'action': 'shutdown'})
            response.raise_for_status()
            print('Standalone voice input stopping')
        elif args.command == 'start' and state is None:
            log_dir = ROOT / 'results'
            log_dir.mkdir(parents=True, exist_ok=True)
            with (log_dir / 'st-voice.log').open('ab') as log:
                subprocess.Popen([sys.executable, '-m', 'client.st_voice', 'serve'],
                    cwd=Path(__file__).resolve().parents[1], stdout=log, stderr=log,
                    creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            print('Standalone voice input starting on ' + URL + '; enable it in ST after configuring PTT')
        else:
            print(json.dumps(state) if state else 'HOLT standalone voice input is stopped')
        return
    bridge = Bridge()
    bridge.server = uvicorn.Server(uvicorn.Config(application(bridge), host='127.0.0.1', port=8083,
                                                log_level='warning', access_log=False))
    bridge.server.run()


if __name__ == '__main__':
    main()

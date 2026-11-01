const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function setup(failGeneration = false) {
    const calls = [];
    const requests = [];
    const generation = { active: false };
    const c = { characterId: 0, characters: [{ name: 'HOLT' }], chatId: 'one', name1: 'Captain', chat: [],
        addOneMessage() { calls.push('render'); },
        async saveChat() { calls.push('save'); },
        async generate() { calls.push('generate'); if (failGeneration) throw Error('backend failed'); } };
    const sandbox = { crypto: { randomUUID: () => 'tab' }, isGenerating: () => generation.active,
        SillyTavern: { getContext: () => c }, document: { getElementById: () => null },
        fetch: async (_url, options) => {
            const body = JSON.parse(options.body); calls.push(body.action); requests.push(body);
            return { ok: true, json: async () => ({ ptt: { connected: true } }) };
        } };
    vm.createContext(sandbox);
    const source = fs.readFileSync('integrations/sillytavern/voice-input/index.js', 'utf8');
    const executable = source.replace("import { isGenerating } from '/script.js';", '');
    vm.runInContext(executable + '\nthis.testSubmit = submit; this.testPoll = async () => { enabled = true; await poll(); };', sandbox);
    return { c, calls, requests, generation, sandbox, submit: sandbox.testSubmit, poll: sandbox.testPoll };
}

test('voice text enters ordinary chat; slash text is not executed as STscript', async () => {
    const x = setup();
    await x.submit({ id: 'receipt', chat: 'one', text: '/send this is spoken text' });
    assert.equal(x.c.chat[0].mes, '/send this is spoken text');
    assert.deepEqual(x.calls, ['ack', 'render', 'save', 'generate']);
});

test('Pocket playback keeps microphone capture gated until the playback tail finishes', async () => {
    const x = setup();
    x.sandbox.document.documentElement = { dataset: { holtPocketSpeaking: 'true' } };
    x.sandbox.document.getElementById = id => id === 'holt-input-status' ? { textContent: '' } : null;
    await x.poll();
    assert.equal(x.requests.at(-1).speaking, true);
    x.sandbox.document.documentElement.dataset.holtPocketSpeaking = 'false';
    await x.poll();
    assert.equal(x.requests.at(-1).speaking, false);
});

test('a repeated receipt never generates another turn, including after failure', async () => {
    const x = setup(true);
    const packet = { id: 'receipt', chat: 'one', text: 'request docking' };
    await assert.rejects(x.submit(packet), /backend failed/);
    await x.submit(packet);
    assert.equal(x.c.chat.length, 1);
    assert.equal(x.calls.filter(c => c === 'generate').length, 1);
});

test('a transcript from another chat cannot be acknowledged or submitted', async () => {
    const x = setup();
    await assert.rejects(x.submit({ id: 'receipt', chat: 'old', text: 'request docking' }), /Chat changed/);
    assert.equal(x.calls.length, 0);
    assert.equal(x.c.chat.length, 0);
});

test('learn button requests joystick learning without starting capture or chat generation', async () => {
    const x = setup();
    x.c.eventTypes = { CHAT_CHANGED: 'chat' };
    x.c.eventSource = { on() {} };
    const elements = {};
    x.sandbox.document = { getElementById: id => elements[id] ||= { append() {}, textContent: '' },
        createElement: () => ({}) };
    vm.runInContext('init()', x.sandbox);
    await elements['holt-input-learn'].onclick();
    assert.deepEqual(x.calls, ['learn']);
    assert.equal(elements['holt-input-learn'].disabled, false);
    assert.equal(x.c.chat.length, 0);
});

test('prompt previews and aborted turns cannot leave voice input stuck busy', async () => {
    const x = setup();
    const handlers = {};
    x.c.eventTypes = { GENERATION_STARTED: 'start', GENERATION_ENDED: 'end',
        GENERATION_STOPPED: 'stop', CHAT_CHANGED: 'chat' };
    x.c.eventSource = { on: (name, handler) => { handlers[name] = handler; } };
    const elements = {};
    x.sandbox.document = { getElementById: id => {
        if (id === 'tts_audio' || id === 'chatterbox-status') return null;
        return elements[id] ||= { append() {}, textContent: '' };
    }, createElement: () => ({}) };
    vm.runInContext('init()', x.sandbox);
    // ST emits this for a dry-run preview without a matching end event.
    await handlers.start?.('normal', {}, true);
    await x.poll();
    assert.equal(x.requests.at(-1).busy, false);
    // A real generation pauses capture, then clears even without an end event.
    x.generation.active = true;
    await x.poll();
    assert.equal(x.requests.at(-1).busy, true);
    x.generation.active = false;
    await x.poll();
    assert.equal(x.requests.at(-1).busy, false);
});

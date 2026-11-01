const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function setup(failGeneration = false) {
    const calls = [];
    const c = { characterId: 0, characters: [{ name: 'HOLT' }], chatId: 'one', name1: 'Captain', chat: [],
        addOneMessage() { calls.push('render'); },
        async saveChat() { calls.push('save'); },
        async generate() { calls.push('generate'); if (failGeneration) throw Error('backend failed'); } };
    const sandbox = { crypto: { randomUUID: () => 'tab' },
        SillyTavern: { getContext: () => c }, document: { getElementById: () => null },
        fetch: async (_url, options) => {
            const body = JSON.parse(options.body); calls.push(body.action);
            return { ok: true, json: async () => ({}) };
        } };
    vm.createContext(sandbox);
    const source = fs.readFileSync('integrations/sillytavern/voice-input/index.js', 'utf8');
    vm.runInContext(source + '\nthis.testSubmit = submit;', sandbox);
    return { c, calls, submit: sandbox.testSubmit };
}

test('voice text enters ordinary chat; slash text is not executed as STscript', async () => {
    const x = setup();
    await x.submit({ id: 'receipt', chat: 'one', text: '/send this is spoken text' });
    assert.equal(x.c.chat[0].mes, '/send this is spoken text');
    assert.deepEqual(x.calls, ['ack', 'render', 'save', 'generate']);
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

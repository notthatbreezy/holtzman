const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('integrations/sillytavern/pocket-tts/player.js', 'utf8');
const loaded = import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));
const pcm = Buffer.alloc(3840); pcm.writeInt16LE(32767, 0); pcm.writeInt16LE(-32768, 2);
const start = { type: 'start', format: 's16le', sample_rate: 24000, channels: 1 };
const frame = index => ({ type: 'audio', index, samples: 1920, data: pcm.toString('base64') });
const end = chunks => ({ type: 'end', chunks, generation_ms: 50 });

function fakeContext() {
    const sources = [];
    const context = { currentTime: 10, state: 'running', destination: {}, outputLatency: 0.02,
        resume: async () => {}, createBuffer: (_channels, count) => ({
            samples: new Float32Array(count), getChannelData() { return this.samples; } }),
        createBufferSource: () => {
            const node = { connect() {}, disconnect() {}, start(when) { this.when = when; },
                stop() { this.onended?.(); } };
            sources.push(node); return node;
        } };
    return { context, sources };
}

function response(records, step = 11) {
    const bytes = new TextEncoder().encode(records.map(x => JSON.stringify(x)).join('\n') + '\n');
    return new Response(new ReadableStream({ start(controller) {
        for (let offset = 0; offset < bytes.length; offset += step) controller.enqueue(bytes.slice(offset, offset + step));
        controller.close();
    } }));
}

test('PCM records survive split network packets and are scheduled without gaps; busy covers playback tail', async () => {
    const { PocketPlayer } = await loaded;
    const { context, sources } = fakeContext();
    const states = [];
    const player = new PocketPlayer({ contextFactory: () => context,
        fetcher: async () => response([start, frame(0), frame(1), frame(2), end(3)]),
        now: () => 100, onState: value => states.push(value) });
    let finished = false;
    const playing = player.play('http://localhost:8005', 'Test', 'k2so', 100).then(x => { finished = true; return x; });
    await new Promise(setImmediate);
    assert.equal(sources.length, 3);
    assert.equal(player.busy, true);
    assert.equal(finished, false);
    assert.ok(Math.abs(sources[0].when - 10.1) < 1e-10);
    for (let index = 1; index < sources.length; index++) {
        assert.ok(Math.abs(sources[index].when - sources[index - 1].when - 0.08) < 1e-10);
    }
    assert.equal(sources[0].buffer.samples[1], -1);
    for (const node of sources) node.onended();
    const metrics = await playing;
    assert.equal(metrics.underruns, 0);
    assert.equal(metrics.audioSeconds, 0.24);
    assert.equal(metrics.firstScheduledMs, 100);
    assert.equal(player.busy, false);
    assert.equal(states.at(-1), false);
});

test('HTTP errors and incomplete/malformed streams release busy state instead of falsely completing', async () => {
    const { PocketPlayer } = await loaded;
    for (const fetcher of [
        async () => new Response(JSON.stringify({ detail: 'Busy' }), { status: 409 }),
        async () => response([start, frame(0)]),
        async () => response([start, frame(2), end(1)]),
        async () => response([start, { ...frame(0), samples: 3 }, end(1)]),
    ]) {
        const { context } = fakeContext();
        const player = new PocketPlayer({ contextFactory: () => context, fetcher });
        await assert.rejects(player.play('http://localhost:8005', 'Test', 'k2so'));
        assert.equal(player.busy, false);
        assert.equal(player.sources.size, 0);
    }
});

test('one player refuses overlapping speech requests', async () => {
    const { PocketPlayer } = await loaded;
    const player = new PocketPlayer(); player.busy = true;
    await assert.rejects(player.play('http://localhost:8005', 'Test', 'k2so'), /already playing/);
});

test('saved provider registers before ST initializes without attempting an early DOM load', () => {
    const extension_settings = { tts: { currentProvider: 'HOLT Pocket', voiceMap: { HOLT: 'Existing' } } };
    let registered = false;
    const code = fs.readFileSync('integrations/sillytavern/pocket-tts/index.js', 'utf8').replace(/^import .*;\r?$/gm, '');
    vm.runInNewContext(code, { extension_settings, document: { getElementById: () => null },
        registerTtsProvider: (name, Provider) => {
            assert.equal(name, 'HOLT Pocket');
            assert.equal(typeof Provider, 'function');
            assert.notEqual(extension_settings.tts.currentProvider, name);
            registered = true;
        } });
    assert.ok(registered);
    assert.equal(extension_settings.tts.currentProvider, 'HOLT Pocket');
    assert.equal(extension_settings.tts.voiceMap.HOLT, 'Existing');
    const manifest = JSON.parse(fs.readFileSync('integrations/sillytavern/pocket-tts/manifest.json', 'utf8'));
    assert.ok(manifest.loading_order < 10);
});

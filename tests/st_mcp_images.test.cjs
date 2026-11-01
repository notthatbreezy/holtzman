const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

async function normalize(payload, fetch) {
    let handler;
    vm.runInNewContext(fs.readFileSync('integrations/sillytavern/tool-schema-compat/index.js', 'utf8'), {
        fetch, btoa,
        SillyTavern: { getContext: () => ({ eventTypes: { CHAT_COMPLETION_SETTINGS_READY: 'ready' },
            eventSource: { on: (_event, callback) => { handler = callback; } } }) },
    });
    await handler(payload);
    return JSON.parse(JSON.stringify(payload));
}

function capture(id = 'capture1', tool = 'screenshot', mime = 'image/jpeg', data = '/9j/AA==') {
    return [{ role: 'assistant', tool_calls: [{ id, type: 'function',
        function: { name: `mcp_holtzman_${tool}`, arguments: '{}' } }], content: '' },
    { role: 'tool', tool_call_id: id, content: JSON.stringify({ toolName: tool, status: 'executed',
        data: { isError: false, content: [{ type: 'text', text: 'Capture metadata' },
            { type: 'image', mimeType: mime, data }] } }) }];
}

test('MCP image reaches Luna as image_url, with metadata and original history intact', async () => {
    const messages = capture();
    const original = JSON.stringify(messages);
    const result = await normalize({ model: 'gpt-6-luna', messages: [...messages] });
    assert.equal(JSON.stringify(messages), original);
    assert.equal(result.messages[1].role, 'tool');
    assert.ok(!result.messages[1].content.includes('/9j/AA=='));
    const receipt = JSON.parse(result.messages[1].content);
    assert.equal(receipt.data.content[0].text, 'Capture metadata');
    assert.equal(receipt.data.isError, false);
    assert.equal(result.messages[2].role, 'user');
    assert.deepEqual(result.messages[2].content[1], { type: 'image_url',
        image_url: { url: 'data:image/jpeg;base64,/9j/AA==', detail: 'high' } });
    const beforeRepeat = JSON.stringify(result);
    assert.equal(JSON.stringify(await normalize(result)), beforeRepeat);
});

test('saved image refs reload as image parts without sending a localhost URL to OpenAI', async () => {
    const messages = capture();
    const receipt = JSON.parse(messages[1].content);
    receipt.data.content[1] = { type: 'image', mimeType: 'image/jpeg', holt_image_url: 'user/images/HOLT-MCP/capture.jpeg' };
    messages[1].content = JSON.stringify(receipt);
    let requested;
    const result = await normalize({ model: 'gpt-6-luna', messages }, async url => {
        requested = url;
        return { ok: true, arrayBuffer: async () => Uint8Array.of(255, 216, 255, 0).buffer };
    });
    assert.equal(requested, '/user/images/HOLT-MCP/capture.jpeg');
    assert.equal(result.messages[2].content[1].image_url.url, 'data:image/jpeg;base64,/9j/AA==');
});

test('capture compaction runs before prompt budgeting and vector lookup, persists, and is idempotent', async () => {
    const calls = new Map();
    const [, tool] = capture();
    const invocation = { name: 'mcp_holtzman_screenshot', result: tool.content };
    const chat = [{ mes: tool.content, extra: { tool_invocations: [invocation], memory: 'Keep memory' } }];
    let uploads = 0, saves = 0;
    const context = { chat, getRequestHeaders: () => ({ 'X-CSRF-Token': 'test' }),
        saveChat: async () => { saves++; },
        eventTypes: { TOOL_CALLS_PERFORMED: 'performed', GENERATION_AFTER_COMMANDS: 'before',
            CHAT_COMPLETION_SETTINGS_READY: 'ready', CHAT_CHANGED: 'changed' },
        eventSource: { on: (event, callback) => calls.set(event, callback) } };
    vm.runInNewContext(fs.readFileSync('integrations/sillytavern/tool-schema-compat/index.js', 'utf8'), {
        SillyTavern: { getContext: () => context }, crypto: { randomUUID: () => 'test-capture' },
        fetch: async (url, options) => {
            assert.equal(url, '/api/images/upload');
            const body = JSON.parse(options.body);
            assert.equal(body.ch_name, 'HOLT-MCP');
            assert.equal(body.image, '/9j/AA==');
            uploads++;
            return { ok: true, json: async () => ({ path: 'user/images/HOLT-MCP/test-capture.jpeg' }) };
        },
    });
    await calls.get('performed')();
    assert.equal(uploads, 1);
    assert.equal(saves, 1);
    assert.ok(!JSON.stringify(chat).includes('/9j/AA=='));
    assert.equal(chat[0].extra.memory, 'Keep memory');
    assert.equal(JSON.parse(invocation.result).data.content[1].holt_image_url, 'user/images/HOLT-MCP/test-capture.jpeg');
    await calls.get('before')();
    await calls.get('changed')();
    assert.equal(uploads, 1);
    assert.equal(saves, 1);
});

test('parallel tool receipts stay together before the images', async () => {
    const [a, t] = capture('one');
    const [b, u] = capture('two', 'nav_target_start', 'image/png', 'AAAA');
    a.tool_calls.push(...b.tool_calls);
    const result = await normalize({ model: 'gpt-6-luna', messages: [a, t, u] });
    assert.deepEqual(result.messages.map(x => x.role), ['assistant', 'tool', 'tool', 'user']);
    assert.equal(result.messages[3].content.filter(x => x.type === 'image_url').length, 2);
    assert.ok(result.messages[3].content[3].image_url.url.startsWith('data:image/png;'));
});

test('long chats include only two latest captures and explain omitted images', async () => {
    const result = await normalize({ model: 'gpt-6-luna', messages: [
        ...capture('old'), ...capture('middle'), ...capture('new'),
    ] });
    const images = result.messages.flatMap(x => Array.isArray(x.content) ? x.content : [])
        .filter(x => x.type === 'image_url');
    assert.equal(images.length, 2);
    assert.match(result.messages[2].content[0].text, /Older.*omitted/);
    for (const message of result.messages.filter(x => x.role === 'tool')) {
        assert.ok(!message.content.includes('/9j/AA=='));
    }
});

test('other models, tools, servers, malformed JSON and invalid images are unchanged', async () => {
    const cases = [
        { model: 'qwen3-coder-next:q8_0', messages: capture() },
        { model: 'gpt-6-luna', messages: capture('x', 'status') },
        { model: 'gpt-6-luna', messages: capture('x', 'screenshot', 'image/svg+xml') },
        { model: 'gpt-6-luna', messages: capture('x', 'screenshot', 'image/jpeg', 'bad url!') },
        { model: 'gpt-6-luna', messages: [{ role: 'tool', content: 'Invalid URL' }] },
    ];
    const other = capture(); other[0].tool_calls[0].function.name = 'mcp_other_screenshot';
    cases.push({ model: 'gpt-6-luna', messages: other });
    for (const item of cases) {
        const original = JSON.parse(JSON.stringify(item));
        assert.deepEqual(await normalize(item), original);
    }
});

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('integrations/sillytavern/context-compaction/compaction.js', 'utf8');
const loaded = import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));
const user = mes => ({ is_user: true, mes });
const events = () => Array.from({ length: 80 }, (_, i) => ({ event: i % 2 ? 'Bounty' : 'FSDJump',
    timestamp: `2026-11-01T12:${String(i % 60).padStart(2, '0')}:00Z`, TotalReward: i,
    StarSystem: 'Sol', Factions: [{ name: 'large detail', state: 'x'.repeat(1000) }] }));
function tool(id = 'old', name = 'recent_events', value = events()) {
    return { is_system: true, is_user: false, send_date: '2026-11-01T12:00:00Z',
        mes: 'Original displayed receipt', extra: { memory: 'Keep memory', api: 'custom', model: 'model',
            tool_invocations: [{ id, name: `mcp_holtzman_${name}`, parameters: '{}', signature: 'keep signature',
                result: JSON.stringify({ toolName: name, status: 'executed', data: { isError: false,
                    content: [{ type: 'text', text: JSON.stringify(value) }] } }) }] } };
}
function history(old = tool()) { return [user('old request'), old, user('previous request'), user('current request')]; }
function copy(chat) { return chat.map(m => ({ ...m })); } // Deliberately shares nested objects like ST.

test('old JSON results shrink before budgeting without mutating the durable archive or protocol identity', async () => {
    const { compactHistory } = await loaded;
    const saved = history(); const original = JSON.stringify(saved); const prompt = copy(saved);
    const before = saved[1].extra.tool_invocations[0];
    const stats = compactHistory(prompt, saved);
    assert.equal(stats.compacted, 1);
    assert.ok(stats.afterChars < stats.beforeChars / 10);
    assert.equal(JSON.stringify(saved), original);
    assert.notEqual(prompt[1].extra, saved[1].extra);
    const after = prompt[1].extra.tool_invocations[0];
    assert.deepEqual({ ...after, result: before.result }, before);
    assert.equal(prompt[1].extra.memory, 'Keep memory');
    const receipt = JSON.parse(after.result);
    const summary = JSON.parse(receipt.data.content[0].text);
    assert.equal(receipt.context_compaction.historical, true);
    assert.equal(summary.total_returned_events, 80);
    assert.equal(summary.event_counts.Bounty, 40);
    assert.ok(summary.omitted_events > 0);
    assert.equal(summary.retained_tail_events.at(-1).TotalReward, 79);
    assert.ok(!prompt[1].mes.includes('large detail'));
});

test('all results in the latest two user turns and chats without a user turn remain full', async () => {
    const { compactHistory } = await loaded;
    const saved = [...history(), tool('new')]; const prompt = copy(saved);
    compactHistory(prompt, saved);
    assert.deepEqual(prompt.at(-1), saved.at(-1));
    const recent = [user('one'), tool('one'), user('two'), tool('two')];
    const recentPrompt = copy(recent); const original = JSON.stringify(recent);
    compactHistory(recentPrompt, recent);
    assert.equal(JSON.stringify(recentPrompt), original);
    const none = [tool()]; const unchanged = JSON.stringify(none);
    compactHistory(none, none);
    assert.equal(JSON.stringify(none), unchanged);
});

test('write receipts, errors, malformed results, unknown servers and image content remain untouched', async () => {
    const { compactHistory } = await loaded;
    const cases = [tool('x', 'ship_action'), tool('x', 'log_entry'), tool('x', 'read_guide')];
    const error = tool('x'); error.extra.tool_invocations[0].error = true; cases.push(error);
    const nested = tool('x', 'status', { error: 'failure', details: 'x'.repeat(10000) }); cases.push(nested);
    const malformed = tool('x'); malformed.extra.tool_invocations[0].result = '{'.repeat(10000); cases.push(malformed);
    const other = tool('x'); other.extra.tool_invocations[0].name = 'mcp_other_recent_events'; cases.push(other);
    const image = tool('x'); const receipt = JSON.parse(image.extra.tool_invocations[0].result);
    receipt.data.content.push({ type: 'image', holt_image_url: 'user/images/HOLT-MCP/test.png' });
    image.extra.tool_invocations[0].result = JSON.stringify(receipt); cases.push(image);
    for (const item of cases) {
        const saved = history(item); const original = JSON.stringify(saved); const prompt = copy(saved);
        assert.equal(compactHistory(prompt, saved).compacted, 0);
        assert.equal(JSON.stringify(prompt), original);
    }
});

test('mixed batches preserve actions byte-for-byte and preserve every ID, argument, signature and order', async () => {
    const { compactHistory } = await loaded;
    const item = tool(); const action = tool('action', 'ship_action').extra.tool_invocations[0];
    item.extra.tool_invocations.push(action);
    const saved = history(item); const prompt = copy(saved);
    compactHistory(prompt, saved);
    assert.equal(prompt[1].extra.tool_invocations[1], action);
    assert.deepEqual(prompt[1].extra.tool_invocations.map(({ result, ...rest }) => rest),
        item.extra.tool_invocations.map(({ result, ...rest }) => rest));
});

test('duplicate IDs and mismatched current results fail closed; reordered prompt copies use original turn age', async () => {
    const { compactHistory } = await loaded;
    const saved = history(); saved.push(tool('old'));
    assert.equal(compactHistory(copy(saved), saved).compacted, 0);
    const clean = history(); const changed = copy(clean);
    changed[1] = tool('old', 'recent_events', [...events(), { event: 'new data' }]);
    assert.equal(compactHistory(changed, clean).compacted, 0);
    const reordered = [clean[3], clean[1], clean[0], clean[2]];
    assert.equal(compactHistory(reordered, clean).compacted, 1);
});

test('compaction is idempotent and disabled policy preserves the exact prompt', async () => {
    const { compactHistory } = await loaded;
    const saved = history(); const prompt = copy(saved);
    compactHistory(prompt, saved); const once = JSON.stringify(prompt);
    compactHistory(prompt, saved); assert.equal(JSON.stringify(prompt), once);
    const untouched = copy(saved); compactHistory(untouched, saved, { enabled: false });
    assert.equal(JSON.stringify(untouched), JSON.stringify(saved));
});

test('schema drift, nested failures and malformed invocation entries are preserved safely', async () => {
    const { compactHistory } = await loaded;
    const values = [tool('x', 'fleet', { fleet: [{ name: 'Actual ship' }], extra: 'x'.repeat(10000) }),
        tool('x', 'fleet', { ships: 'unavailable', extra: 'x'.repeat(10000) }),
        tool('x', 'fleet', { ships: [null], extra: 'x'.repeat(10000) }),
        tool('x', 'status', 'Error: ' + 'x'.repeat(10000)),
        tool('x', 'status', Array(10000).fill('unknown')),
        tool('x', 'recent_events', Array(10000).fill('not an event')),
        tool('x', 'fleet', { ships: [], details: { a: { b: { c: { d: { error: 'failed to retrieve loadout' } } } } }, extra: 'x'.repeat(10000) })];
    for (const item of values) {
        const saved = history(item); const before = JSON.stringify(saved); const prompt = copy(saved);
        assert.equal(compactHistory(prompt, saved).compacted, 0);
        assert.equal(JSON.stringify(prompt), before);
    }
    const saved = history(); saved[1].extra.tool_invocations.push(null, 'invalid');
    const before = JSON.stringify(saved); const prompt = copy(saved);
    assert.equal(compactHistory(prompt, saved).compacted, 1);
    assert.equal(prompt[1].extra.tool_invocations[1], null);
    assert.equal(JSON.stringify(saved), before);
    for (const [name, result] of [[123, tool().extra.tool_invocations[0].result],
        ['mcp_holtzman_fleet', ' '.repeat(6000) + 'null']]) {
        const item = tool(); Object.assign(item.extra.tool_invocations[0], { name, result });
        const archive = history(item); const untouched = JSON.stringify(archive); const request = copy(archive);
        assert.equal(compactHistory(request, archive).compacted, 0);
        assert.equal(JSON.stringify(request), untouched);
    }
});

test('fleet summary keeps ship identities, locations, warnings and explicit equipment omissions', async () => {
    const { compactHistory } = await loaded;
    const value = { ship_count: 2, warnings: ['Equipment can be stale.'], current_ship_id: 7,
        ships: [7, 8].map(id => ({ ship_id: id, ship_type: 'Anaconda', name: `Ship ${id}`,
            location: { system: 'Sol', station: 'Galileo' }, loadout: { age_seconds: 200,
                combat_relevant_equipment: Array.from({ length: 40 }, () => ({ item: 'laser', enabled: true })),
                modules: 'x'.repeat(10000) } })) };
    const saved = history(tool('fleet', 'fleet', value)); const prompt = copy(saved);
    compactHistory(prompt, saved);
    const summary = JSON.parse(JSON.parse(prompt[1].extra.tool_invocations[0].result).data.content[0].text);
    assert.equal(summary.ship_count, 2); assert.equal(summary.ships.length, 2);
    assert.equal(summary.ships[0].ship_id, 7); assert.equal(summary.ships[0].location.station, 'Galileo');
    assert.equal(summary.warnings[0], value.warnings[0]);
    assert.equal(summary.ships[0].loadout.omitted_equipment, 30);
});

test('extension runs only for HOLT, excludes group ID zero, and precedes vector rearrangement', async () => {
    const { compactHistory, defaults } = await loaded;
    let used = 0;
    const context = { characterId: 0, characters: [{ name: 'Other' }], chat: history(), groupId: null };
    const sandbox = { SillyTavern: { getContext: () => context }, extension_settings: {}, defaults,
        compactHistory: (...args) => { used++; return compactHistory(...args); }, saveSettingsDebounced() {},
        document: { getElementById: () => null }, console: { info() {} } };
    const code = fs.readFileSync('integrations/sillytavern/context-compaction/index.js', 'utf8').replace(/^import .*;\r?$/gm, '');
    vm.runInNewContext(code, sandbox);
    await sandbox.holtCompactContext(copy(context.chat)); assert.equal(used, 0);
    context.characters[0].name = 'HOLT'; context.groupId = 0;
    await sandbox.holtCompactContext(copy(context.chat)); assert.equal(used, 0);
    context.groupId = null;
    await sandbox.holtCompactContext(copy(context.chat)); assert.equal(used, 1);
    const manifest = JSON.parse(fs.readFileSync('integrations/sillytavern/context-compaction/manifest.json'));
    assert.ok(manifest.loading_order < 100);
    assert.equal(manifest.generate_interceptor, 'holtCompactContext');
});

test('vector indexing and stale retrieval candidates exclude whole tool groups, retaining conversation', () => {
    const patch = fs.readFileSync('scripts/patch-st-retrieval.py', 'utf8');
    const index = JSON.parse(patch.match(/^INDEX_AFTER = (".*")$/m)[1]);
    const retrieve = JSON.parse(patch.match(/^RETRIEVE_AFTER = (".*")$/m)[1]);
    const receipt = tool(); const ordinary = user('Remember this conversation.');
    const sandbox = { context: { chat: [receipt, ordinary] }, settings: { keep_hidden: true },
        messages: [receipt, ordinary], retainMessages: [], selected: [] };
    const indexed = vm.runInNewContext(index + 'x)', sandbox);
    assert.equal(indexed.length, 1); assert.equal(indexed[0], ordinary);
    vm.runInNewContext(`for (const message of messages) { ${retrieve} continue; } selected.push(message); }`, sandbox);
    assert.equal(sandbox.selected.length, 1); assert.equal(sandbox.selected[0], ordinary);
});

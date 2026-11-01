// Exercises the installed ST functions without starting a browser or game input.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = process.argv[2] || path.join(os.homedir(), 'SillyTavern');

function functionSource(file, name, nextMarker) {
    const source = fs.readFileSync(path.join(root, file), 'utf8');
    const start = source.indexOf(`async function ${name}(`);
    const end = source.indexOf(nextMarker, start);
    assert.ok(start >= 0 && end > start, `Unsupported ST function: ${name}`);
    return source.slice(start, end).trim();
}

(async () => {
    const query = functionSource('public/scripts/extensions/vectors/index.js', 'getQueryText', '\n/**');
    const sandbox = { settings: { query: 2, enabled_chats: false },
        substituteParams: x => x, getStringHash: x => x.length, collapseNewlines: x => x };
    vm.createContext(sandbox);
    vm.runInContext(query, sandbox);
    const chat = [{ mes: 'We are trading.' }, { mes: 'What about combat?' },
        { mes: '_FLEET_PAYLOAD_'.repeat(5000), is_system: true,
            extra: { tool_invocations: [{ name: 'mcp_holtzman_fleet' }] } }];
    const before = JSON.stringify(chat);
    assert.equal(await sandbox.getQueryText(chat, 'world-info'), 'What about combat?\nWe are trading.');
    assert.equal(JSON.stringify(chat), before);
    console.log('PASS: retrieval uses conversation, excludes tool receipts, preserves chat');

    const embedding = functionSource('src/vectors/embedding.js', 'getTransformersVector', '\n/**');
    for (const [declared, expected] of [[2147483648, 512], [256, 256], [undefined, 512]]) {
        const pipe = async text => {
            assert.equal(pipe.tokenizer.model_max_length, expected);
            assert.equal(text, 'Unmodified input');
            return { data: new Float32Array([0.25, 0.5]) };
        };
        pipe.tokenizer = { model_max_length: declared };
        const context = { getPipeline: async () => pipe, TASK: 'feature-extraction' };
        vm.createContext(context);
        vm.runInContext(embedding, context);
        assert.equal(JSON.stringify(await context.getTransformersVector('Unmodified input')), '[0.25,0.5]');
    }
    console.log('PASS: embedding limit caps at 512, respects smaller limits, preserves vector output');
})().catch(error => { console.error(error); process.exitCode = 1; });

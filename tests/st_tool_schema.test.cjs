const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function prepare(tools) {
    let handler;
    const context = { eventTypes: { CHAT_COMPLETION_SETTINGS_READY: 'ready' },
        eventSource: { on: (_event, callback) => { handler = callback; } } };
    const sandbox = { SillyTavern: { getContext: () => context } };
    vm.createContext(sandbox);
    vm.runInContext(fs.readFileSync('integrations/sillytavern/tool-schema-compat/index.js', 'utf8'), sandbox);
    const payload = { tools };
    handler(payload);
    return payload.tools === undefined ? undefined : JSON.parse(JSON.stringify(payload.tools));
}

test('unconstrained MCP property becomes an object schema without losing any tools', () => {
    const tools = [{ type: 'function', function: { name: 'objectives', parameters: {
        type: 'object', properties: { measure: true, scratch: { type: 'boolean', default: false } },
        additionalProperties: false, required: ['measure'],
    } } }, { type: 'function', function: { name: 'plot_route', parameters: { type: 'object' } } }];
    const originalSchema = tools[0].function.parameters;
    const result = prepare(tools);
    assert.equal(result.length, 2);
    assert.deepEqual(result[0].function.parameters.properties.measure, {});
    assert.equal(result[0].function.parameters.additionalProperties, false);
    assert.equal(result[0].function.parameters.properties.scratch.default, false);
    assert.deepEqual(result[0].function.parameters.required, ['measure']);
    assert.equal(originalSchema.properties.measure, true);
    assert.equal(result[1].function.name, 'plot_route');
});

test('nested schema positions convert while boolean enum, const and default data remain intact', () => {
    const schema = { $defs: { any: true, never: false }, properties: {
        values: { type: 'array', items: true }, choice: { anyOf: [true, false] },
        data: { enum: [true, false], const: true, default: { properties: { realData: true } } },
    } };
    const result = prepare([{ type: 'function', function: { parameters: schema } }])[0].function.parameters;
    assert.deepEqual(result.$defs, { any: {}, never: { not: {} } });
    assert.deepEqual(result.properties.values.items, {});
    assert.deepEqual(result.properties.choice.anyOf, [{}, { not: {} }]);
    assert.deepEqual(result.properties.data, schema.properties.data);
});

test('missing tools and non-function tools are left alone', () => {
    assert.deepEqual(prepare(undefined), undefined);
    assert.deepEqual(prepare([{ type: 'web_search' }]), [{ type: 'web_search' }]);
});

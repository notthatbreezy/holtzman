// Run after installing jsonschema@1.5.0 in .local/holt/schema-validator.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { Validator } = require('../.local/holt/schema-validator/node_modules/jsonschema');
const schema = { $schema: 'https://json-schema.org/draft/2020-12/schema', type: 'object',
    $defs: { Format: { type: 'string', enum: ['jpeg', 'png'] },
        Rect: { type: 'object', properties: { x: { type: 'integer' }, y: { type: 'integer' },
            width: { type: 'integer', minimum: 1 }, height: { type: 'integer', minimum: 1 } },
        required: ['x', 'y', 'width', 'height'] } },
    properties: { format: { anyOf: [{ $ref: '#/$defs/Format' }, { type: 'null' }] },
        rect: { anyOf: [{ $ref: '#/$defs/Rect' }, { type: 'null' }] } } };
const validate = value => new Validator().validate(value, schema, {
    throwError: true, base: 'https://mcp.invalid/schema/',
});

test('reproduce anonymous $ref failure in the installed bridge dependency', () => {
    assert.throws(() => new Validator().validate({ format: 'jpeg' }, schema, { throwError: true }),
        /Invalid URL/);
});

test('local base repairs reference resolution while enum and rectangle validation remain enforced', () => {
    const before = JSON.stringify(schema);
    for (const value of [{}, { format: 'jpeg' }, { format: 'png' }, { format: null },
        { rect: { x: 0, y: 0, width: 100, height: 50 } }]) assert.equal(validate(value).valid, true);
    for (const value of [{ format: 'gif' }, { rect: { x: 0, y: 0, width: 100 } },
        { rect: { x: 0, y: 0, width: 0, height: 50 } }]) {
        assert.throws(() => validate(value), error => error.message !== 'Invalid URL');
    }
    assert.equal(JSON.stringify(schema), before);
});

test('regression test uses the same validator options as the deployment patch', () => {
    const patch = fs.readFileSync('scripts/patch-st-mcp-validation.py', 'utf8');
    assert.match(patch, /BASE = "https:\/\/mcp\.invalid\/schema\/"/);
    assert.ok(patch.includes("base: '{BASE}'"));
    assert.ok(patch.includes('base:"{BASE}"'));
});

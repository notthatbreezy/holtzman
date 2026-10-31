// Offline checks against the installed SillyTavern keyword matcher, without a model call.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..');
const book = JSON.parse(fs.readFileSync(path.join(root, 'integrations/sillytavern/Elite-Dangerous.lorebook.json'), 'utf8'));
const upstream = fs.readFileSync(path.join(root, '.local/holt/sillytavern/public/scripts/world-info.js'), 'utf8');
const start = upstream.indexOf('class WorldInfoBuffer {');
const end = upstream.indexOf('class WorldInfoTimedEffects {', start);
assert(start >= 0 && end > start, 'Installed SillyTavern matcher has changed; review the extraction');
const sandbox = {
    MAX_SCAN_DEPTH: 1000,
    world_info_depth: 2,
    world_info_case_sensitive: false,
    world_info_match_whole_words: true,
    world_info_logic: { AND_ANY: 0, NOT_ALL: 1, NOT_ANY: 2, AND_ALL: 3 },
    scan_state: { NONE: 0, INITIAL: 1, RECURSION: 2, MIN_ACTIVATIONS: 3 },
    // All book keys below are checked as literals. Regex handling isn't under test.
    parseRegexFromString: () => null,
    escapeRegex: (value) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
};
vm.runInNewContext(upstream.slice(start, end) + '\nglobalThis.BufferClass = WorldInfoBuffer;', sandbox, { timeout: 2000 });
const entries = Object.values(book.entries);
assert.equal(entries.filter(e => e.constant).length, 1);
const ids = new Set();
for (const entry of entries) {
    const id = entry.extensions.holtzman.id;
    assert(!ids.has(id), `Duplicate ID ${id}`);
    ids.add(id);
    assert.equal(String(entry.uid), Object.keys(book.entries).find(k => book.entries[k] === entry));
    assert(entry.key.length > 0 && entry.content.length > 0);
    assert(entry.key.every(key => !key.startsWith('/')), 'Only literal keys are expected');
    assert(entry.preventRecursion && entry.excludeRecursion && !entry.ignoreBudget);
    assert.equal(entry.scanDepth, 2);
    assert(entry.extensions.holtzman.sources.length > 0);
    assert(entry.extensions.holtzman.sources.every(s => s.url.startsWith('https://')));
    assert(!entry.content.includes('https://'), 'Source links belong in metadata, not spoken context');
}

const cases = [
    ['unrelated fantasy', ['Tell me about the enchanted forest.'], [], ['raxxla', 'galaxy', 'trading']],
    ['Raxxla', ['Do you know where Raxxla is?'], ['raxxla'], ['scooping', 'engineers', 'delacy']],
    ['fuel scoop', ['Can I buy a fuel scoop in Ross 248?'], ['scooping'], ['raxxla', 'titans', 'galaxy']],
    ['Galactic factions', ['Compare the Federation, Empire and Alliance.'], ['federation', 'empire', 'alliance'], ['kumo', 'farseer']],
    ['Guardian AI', ['What happened to the Guardians and their artificial intelligence?'], ['guardians', 'constructs'], ['hutton', 'trading']],
    ['Tight acronym', ['Tell me about SCO.'], ['sco'], ['ranks', 'holo-me']],
    ['Hutton myth', ['Where is my free Anaconda at Hutton Orbital?'], ['hutton'], ['galaxy', 'marlinists']],
    ['Engineer', ['Who is Felicity Farseer?'], ['farseer'], ['martuuk', 'palin']],
    ['Titan history', ['Tell me about Cocijo.'], ['titans'], ['scooping', 'colonisation']],
    ['Punctuation in keys', ['Let us visit Sagittarius A*.'], ['sagittarius'], ['hutton', 'raxxla']],
    ['ship and task', ['Can this Python hunt pirates?'], ['delacy', 'combat'], ['farseer', 'galaxy']],
    ['case insensitive', ['TELL ME ABOUT THE DARK WHEEL.'], ['dark-wheel'], ['scooping']],
    ['no substring firing', ['The engineerings and megagalaxy words are contrived.'], [], ['engineers', 'galaxy']],
    ['scan depth', ['We have changed topics.', 'Yes, a different topic.', 'Tell me about Raxxla.'], [], ['raxxla']],
];

const results = [];
for (const [name, messages, required, excluded] of cases) {
    const buffer = new sandbox.BufferClass(messages, {});
    const matches = entries.filter(e => e.constant || e.key.some(k => buffer.matchKeys(buffer.get(e, 1), k, e)));
    const selected = matches.map(e => e.extensions.holtzman.id);
    for (const id of required) assert(selected.includes(id), `${name}: missing ${id}`);
    for (const id of excluded) assert(!selected.includes(id), `${name}: unexpected ${id}`);
    assert(selected.length <= 8, `${name}: excessive keyword activation`);
    results.push({ name, matches: selected, approximate_content_tokens: Math.ceil(matches.map(e => e.content).join('\n').length / 4) });
}
const report = { entries: entries.length, cases: results.length,
    matcher: 'Unmodified WorldInfoBuffer from installed SillyTavern, isolated with literal-key helpers',
    limits: 'Checks keyword selection and scan depth; not full browser token-budget placement or model quality.',
    results };
console.log(JSON.stringify(report, null, 2));

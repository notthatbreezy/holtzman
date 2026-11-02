/* Pure prompt-copy transformation. Never writes chat, dispatches tools or calls a model. */
const readers = new Set(['fleet', 'recent_events', 'status', 'session_briefing']);
export const defaults = { enabled: true, keepTurns: 2, maxResultChars: 5000 };

function pick(object, keys) {
    return Object.fromEntries(keys.filter(key => object?.[key] !== undefined).map(key => [key, object[key]]));
}

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function failed(value) {
    const pending = [value];
    let inspected = 0;
    while (pending.length) {
        const item = pending.pop();
        if (!item || typeof item !== 'object') continue;
        if (++inspected > 100000) return true; // Too much uninspected structure: fail closed.
        if (item.isError === true || item.success === false || item.error
            || ['failed', 'error', 'rejected'].includes(item.status)) return true;
        for (const child of Object.values(item)) if (child && typeof child === 'object') pending.push(child);
    }
    return false;
}

function eventSummary(events, level) {
    const count = [12, 4, 0][level];
    const fields = ['timestamp', 'event', 'StarSystem', 'StationName', 'ShipID', 'ShipType',
        'ShipName', 'ShipIdent', 'TotalReward', 'Reward', 'Amount', 'Count', 'MissionID',
        'Name', 'Faction', 'VictimFaction', 'Target_Localised', 'JumpDist', 'FuelUsed', 'FuelLevel'];
    const retained = count ? events.slice(-count).map(event => pick(event, fields)) : [];
    const counts = Object.create(null);
    for (const event of events) {
        const name = String(event?.event || 'unknown');
        counts[name] = (counts[name] || 0) + 1;
    }
    return { total_returned_events: events.length, event_counts: counts,
        first_returned_timestamp: events[0]?.timestamp, last_returned_timestamp: events.at(-1)?.timestamp,
        retained_tail_events: retained, omitted_events: events.length - retained.length,
        omitted_fields: 'Nested snapshots and fields outside the compact event schema are omitted.' };
}

function fleetSummary(value, level) {
    const ships = Array.isArray(value.ships) ? value.ships : [];
    return { ...pick(value, ['commander', 'galaxy', 'retrieved_at_utc', 'current_ship_id', 'ship_count', 'coverage', 'warnings']),
        ships: ships.map(ship => {
            const identity = pick(ship, ['id', 'ship_id', 'name', 'ship', 'type', 'type_name', 'ship_type', 'ship_name',
                'user_name', 'ident', 'is_current', 'hot', 'location', 'system', 'station', 'last_observed']);
            if (level < 2 && ship.loadout) {
                const loadout = pick(ship.loadout, ['timestamp', 'age_seconds', 'observed_at', 'available']);
                const equipment = ship.loadout.combat_relevant_equipment;
                if (Array.isArray(equipment)) {
                    const limit = level === 0 ? 10 : 3;
                    loadout.equipment_count = equipment.length;
                    loadout.equipment_sample = equipment.slice(0, limit).map(item => pick(item, ['item', 'slot', 'enabled']));
                    loadout.omitted_equipment = Math.max(0, equipment.length - limit);
                }
                identity.loadout = loadout;
            }
            return identity;
        }), omitted_fields: 'Full module lists, engineering and other detailed ship fields are omitted. Missing equipment details are unknown, not absent.' };
}

function summarizeValue(tool, value, level) {
    if (tool === 'recent_events' && Array.isArray(value)) return eventSummary(value, level);
    if (tool === 'fleet' && value && typeof value === 'object' && !Array.isArray(value)) return fleetSummary(value, level);
    // Volatile status/briefings keep their scalar facts and explicitly omit nested detail.
    return { facts: value && typeof value === 'object' && !Array.isArray(value)
        ? Object.fromEntries(Object.entries(value).filter(([, v]) => v === null
            || ['number', 'boolean'].includes(typeof v) || typeof v === 'string' && v.length <= 300)) : {},
    omitted_fields: 'Nested objects, arrays and long text are omitted from this historical snapshot.' };
}

function compactResult(invocation, maxChars, observedAt) {
    if (typeof invocation.result !== 'string' || invocation.result.length <= maxChars || invocation.error) return null;
    const tool = typeof invocation.name === 'string' && invocation.name.startsWith('mcp_holtzman_')
        ? invocation.name.slice(13) : '';
    if (!readers.has(tool)) return null;
    let receipt;
    try { receipt = JSON.parse(invocation.result); } catch { return null; }
    if (!object(receipt) || receipt.context_compaction || receipt.toolName !== tool || receipt.status !== 'executed'
        || !Array.isArray(receipt.data?.content) || failed(receipt)) return null;
    // Preserve multimodal/unknown content and prose errors, not just recognizable JSON.
    const values = [];
    for (const block of receipt.data.content) {
        if (block?.type !== 'text' || typeof block.text !== 'string') return null;
        try {
            const value = JSON.parse(block.text);
            if (failed(value)) return null;
            if (tool === 'fleet' && (!object(value) || !Array.isArray(value.ships)
                || value.ships.some(ship => !object(ship)))) return null;
            if (tool === 'recent_events' && (!Array.isArray(value)
                || value.some(event => !object(event) || typeof event.event !== 'string'))) return null;
            if (['status', 'session_briefing'].includes(tool) && !object(value)) return null;
            values.push(value);
        } catch { return null; }
    }
    if (!values.length) return null;
    for (let level = 0; level < 3; level++) {
        const reduced = { toolName: receipt.toolName, status: receipt.status,
            data: { isError: false, content: values.map(value => ({ type: 'text',
                text: JSON.stringify(summarizeValue(tool, value, level)) })) },
            context_compaction: { version: 1, historical: true, observed_at: observedAt,
                original_chars: invocation.result.length,
                note: 'Partial historical result, not current ship state. Full original remains in the ST chat archive. Query the tool again for fresh details; never infer an action was performed from omitted data.' } };
        const serialized = JSON.stringify(reduced);
        if (serialized.length <= maxChars) return serialized;
    }
    // If even the minimal structured view is too large, leave it intact. No silent cuts.
    return null;
}

function identities(savedChat) {
    const calls = new Map();
    let turn = 0;
    for (const message of savedChat) {
        if (message?.is_user) turn++;
        const invocations = message?.extra?.tool_invocations;
        for (const call of Array.isArray(invocations) ? invocations : []) {
            if (!object(call) || typeof call.id !== 'string' || !call.id) continue;
            if (calls.has(call.id)) { calls.set(call.id, null); continue; }
            calls.set(call.id, { turn, name: call.name, result: call.result, observedAt: message.send_date });
        }
    }
    return { calls, turn };
}

function render(invocations) {
    const escape = text => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
    return `<details><summary>Tool calls: ${escape(invocations.map(x => x?.displayName || x?.name || '(unrecognized receipt)').join(', '))}</summary><pre><code class="language-json">${escape(JSON.stringify(invocations, null, 2))}</code></pre></details>`;
}

export function compactHistory(coreChat, savedChat, options = {}) {
    const policy = { ...defaults, ...options };
    const stats = { beforeChars: 0, afterChars: 0, compacted: 0, protected: 0 };
    if (!policy.enabled || !Array.isArray(coreChat) || !Array.isArray(savedChat)) return stats;
    const keepTurns = Math.trunc(Math.min(8, Math.max(1, Number(policy.keepTurns) || 2)));
    const maxChars = Math.min(12000, Math.max(1500, Number(policy.maxResultChars) || 5000));
    const { calls, turn } = identities(savedChat);
    for (let index = 0; index < coreChat.length; index++) {
        const message = coreChat[index];
        const original = message?.extra?.tool_invocations;
        if (!Array.isArray(original)) continue;
        let changed = false;
        const invocations = original.map(invocation => {
            if (!object(invocation)) return invocation;
            const size = typeof invocation.result === 'string' ? invocation.result.length : 0;
            stats.beforeChars += size;
            const identity = calls.get(invocation.id);
            const recent = !turn || !identity || identity.turn > turn - keepTurns;
            if (recent) stats.protected++;
            const result = !recent && identity.name === invocation.name && identity.result === invocation.result
                ? compactResult(invocation, maxChars, identity.observedAt) : null;
            stats.afterChars += result?.length ?? size;
            if (!result) return invocation;
            changed = true;
            stats.compacted++;
            return { ...invocation, result };
        });
        if (changed) coreChat[index] = { ...message, mes: render(invocations),
            extra: { ...message.extra, tool_invocations: invocations } };
    }
    return stats;
}

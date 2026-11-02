import { extension_settings } from '/scripts/extensions.js';
import { saveSettingsDebounced } from '/script.js';
import { compactHistory, defaults } from './compaction.js';

const key = 'holtContextCompaction';
extension_settings[key] = { ...defaults, ...extension_settings[key] };
let lastStats = null;

function isHolt(context) {
    return !(context.groupId != null && context.groupId !== '')
        && context.characters?.[context.characterId]?.name === 'HOLT';
}

function display(toolCount) {
    const node = document.getElementById('holt-context-stats');
    if (!node || !lastStats) return;
    const stats = lastStats;
    node.textContent = `Last generation: ${stats.compacted} old results compacted; tool-result text ${stats.beforeChars.toLocaleString()} → ${stats.afterChars.toLocaleString()} characters. Stored chat unchanged.`
        + (toolCount === undefined ? '' : ` Available tools: ${toolCount}.`);
}

globalThis.holtCompactContext = async coreChat => {
    const context = SillyTavern.getContext();
    if (!isHolt(context)) return;
    const stats = compactHistory(coreChat, context.chat, extension_settings[key]);
    lastStats = stats;
    display();
    console.info('[HOLT context]', stats);
};

const context = SillyTavern.getContext();
if (context.eventTypes?.CHAT_COMPLETION_SETTINGS_READY) {
    context.eventSource.on(context.eventTypes.CHAT_COMPLETION_SETTINGS_READY, payload => {
        if (isHolt(SillyTavern.getContext())) display(payload.tools?.length || 0);
    });
}

const holder = document.getElementById('extensions_settings');
if (holder) {
    const panel = document.createElement('div');
    panel.className = 'inline-drawer';
    panel.innerHTML = `<div class="inline-drawer-toggle inline-drawer-header"><b>HOLT context control</b>
      <div class="inline-drawer-icon fa-solid fa-circle-chevron-down down"></div></div>
      <div class="inline-drawer-content">
        <label><input type="checkbox" id="holt-context-enabled">Compact old read-only tool results</label>
        <label for="holt-context-turns">Recent user turns kept in full</label>
        <input id="holt-context-turns" class="text_pole" type="number" min="1" max="8">
        <label for="holt-context-limit">Maximum characters per compact result</label>
        <input id="holt-context-limit" class="text_pole" type="number" min="1500" max="12000" step="500">
        <p>Preserves all tools, current results, action confirmations and failures. Full originals stay in chat.
        Native prompt previews skip generation interceptors; the figures below describe actual generations.</p>
        <p id="holt-context-stats" role="status">Ready for the next HOLT generation.</p>
      </div>`;
    holder.append(panel);
    const enabled = document.getElementById('holt-context-enabled');
    enabled.checked = extension_settings[key].enabled;
    enabled.onchange = () => { extension_settings[key].enabled = enabled.checked; saveSettingsDebounced(); };
    for (const [id, field, low, high] of [['turns', 'keepTurns', 1, 8], ['limit', 'maxResultChars', 1500, 12000]]) {
        const input = document.getElementById(`holt-context-${id}`);
        input.value = extension_settings[key][field];
        input.onchange = () => {
            extension_settings[key][field] = Math.min(high, Math.max(low, Number(input.value) || defaults[field]));
            input.value = extension_settings[key][field]; saveSettingsDebounced();
        };
    }
}

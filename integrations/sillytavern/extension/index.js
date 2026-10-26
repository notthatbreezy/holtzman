/* HOLT owns tools and audio; SillyTavern owns character, lore and conversation. */
const gateway = 'http://127.0.0.1:8082';
const client = crypto.randomUUID();
const silence = '\u2063';
let cursor = 0, polling = false, speaking = false, captureCase = null, generationBusy = false;
let voiceEnabled = false, transcriptBusy = false;

async function control(body) {
    const response = await fetch(`${gateway}/control`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
    });
    if (!response.ok) throw new Error((await response.json()).error?.message || 'HOLT control failed');
    return response.json();
}

function context() { return SillyTavern.getContext(); }

function status(text) { document.querySelector('#holt-status').textContent = text; }

function failure(error) { status(error.message || String(error)); }

async function submitVoice(text) {
    const c = context();
    if (c.characterId === undefined || c.characters[c.characterId]?.name !== 'HOLT') {
        throw new Error('Select the HOLT character before enabling its microphone');
    }
    const message = { name: c.name1, is_user: true, is_system: false, send_date: Date.now(), mes: text };
    c.chat.push(message);
    c.addOneMessage(message);
    await c.saveChat();
    await c.generate('normal');
}

async function poll() {
    if (polling) return;
    polling = true;
    try {
        const response = await fetch(`${gateway}/control?after=${cursor}&client=${client}`);
        const data = await response.json();
        if (!response.ok) throw new Error(data.error?.message || 'Gateway unavailable');
        status(data.state);
        document.querySelector('#holt-controls').checked = data.controls;
        document.querySelector('#holt-voice').disabled = data.running;
        speaking = data.state.startsWith('Speaking');
        for (const event of data.events) {
            cursor = Math.max(cursor, event.id);
            if (event.kind === 'error') {
                status(event.value);
                document.querySelector('#holt-detail').textContent = String(event.value);
            }
            if (event.kind === 'tool' || event.kind === 'service' || event.kind === 'microphone') {
                document.querySelector('#holt-detail').textContent = String(event.value);
            }
            if (event.kind === 'sources') {
                const target = document.querySelector('#holt-sources');
                target.replaceChildren();
                for (const source of event.value) {
                    const url = new URL(source.url);
                    if (!['https:', 'http:'].includes(url.protocol)) continue;
                    const a = document.createElement('a');
                    a.href = url.href; a.textContent = source.title || url.hostname;
                    a.target = '_blank'; a.rel = 'noopener noreferrer'; target.append(a, ' ');
                }
            }
        }
    } catch (error) { failure(error); }
    finally { polling = false; }
}

async function capturePrompts() {
    // Explicit diagnostic button: no model calls or game access. A separate benchmark chat.
    const c = context();
    if (c.characters[c.characterId]?.name !== 'HOLT') throw new Error('Select HOLT first');
    const cases = await fetch('/scripts/extensions/third-party/holt/cases.json').then(r => r.json());
    await c.executeSlashCommandsWithOptions('/newchat');
    const settings = c.chatCompletionSettings;
    const saved = settings.custom_model;
    try {
        for (const item of cases) {
            const active = context();
            // Each fixture starts fresh in memory; the benchmark chat alone is changed.
            active.chat.splice(0);
            for (const m of [...(item.history || []), { role: 'user', content: item.question }]) {
                active.chat.push({ name: m.role === 'user' ? active.name1 : active.name2,
                    is_user: m.role === 'user', is_system: false, mes: m.content, send_date: Date.now() });
            }
            captureCase = item.id;
            await active.generate('normal');
        }
        status(`Captured ${cases.length} actual SillyTavern prompts; no model/game calls.`);
    } finally { captureCase = null; settings.custom_model = saved; }
}

function init() {
    const c = context();
    const bar = document.createElement('div');
    bar.id = 'holt-bar';
    bar.innerHTML = `<div class="holt-row"><strong>HOLT</strong>
      <button id="holt-start" class="menu_button">Start HOLT</button>
      <button id="holt-stop" class="menu_button">Stop HOLT</button>
      <label><input type="checkbox" id="holt-voice" checked> Voice on Start</label>
      <label><input type="checkbox" id="holt-controls"> Game controls</label>
      <select id="holt-model" aria-label="HOLT reasoning model">
        <option value="holt-hosted">Hosted · GPT-5.4 mini · low reasoning</option>
        <option value="holt-hosted-fast">Hosted · GPT-5.4 mini · no reasoning</option>
        <option value="holt-5.6-luna">Hosted · GPT-5.6 Luna · low reasoning</option>
        <option value="holt-6-luna">Hosted · GPT-6 Luna · low reasoning</option>
        <option value="holt-5.6-terra">Hosted · GPT-5.6 Terra · low reasoning</option>
        <option value="holt-6.1-sol">Hosted · GPT-6.1 Sol · low reasoning</option>
        <option value="holt-local">Local · Qwen · fast</option>
        <option value="holt-local-thinking">Local · Qwen · thinking</option>
      </select><span id="holt-status">Connecting to gateway…</span></div>
      <div id="holt-detail"></div><div id="holt-sources"></div>
      <details><summary>HOLT diagnostics</summary>
        <button id="holt-capture" class="menu_button">Capture evaluation prompts (new test chat)</button>
      </details>`;
    document.body.append(bar);
    const model = document.querySelector('#holt-model');
    model.value = c.chatCompletionSettings.custom_model || 'holt-6-luna';
    model.onchange = () => {
        context().chatCompletionSettings.custom_model = model.value;
        context().chatCompletionSettings.openai_max_context = model.value.startsWith('holt-local') ? 8000 : 12000;
        context().saveSettingsDebounced();
        status('Model selected; applies to the next turn.');
    };
    document.querySelector('#holt-start').onclick = async () => {
        try {
            const active = context();
            const holts = active.characters.map((character, index) => ({ character, index }))
                .filter(item => item.character.name === 'HOLT');
            if (holts.length !== 1) throw new Error('Select one uniquely named HOLT character');
            if (active.characters[active.characterId]?.name !== 'HOLT') {
                await active.selectCharacterById(holts[0].index, { switchMenu: false });
            }
            await context().executeSlashCommandsWithOptions('/api-url api=custom connect=true http://127.0.0.1:8082/v1');
            voiceEnabled = document.querySelector('#holt-voice').checked;
            if (voiceEnabled) await control({ action: 'lease', client, takeover: true });
            await control({ action: 'start', listen: voiceEnabled, speak: voiceEnabled });
        } catch (error) { failure(error); }
    };
    document.querySelector('#holt-stop').onclick = async () => {
        voiceEnabled = false;
        context().stopGeneration();
        try { await control({ action: 'stop' }); } catch (error) { failure(error); }
    };
    document.querySelector('#holt-controls').onchange = async event => {
        try { await control({ action: 'controls', enabled: event.target.checked }); } catch (error) { failure(error); }
    };
    document.querySelector('#holt-capture').onclick = () => capturePrompts().catch(failure);
    c.eventSource.on(c.eventTypes.GENERATION_STARTED, () => { generationBusy = true; });
    c.eventSource.on(c.eventTypes.GENERATION_ENDED, () => { generationBusy = false; });
    c.eventSource.on(c.eventTypes.GENERATION_STOPPED, () => { generationBusy = false; });
    c.eventSource.on(c.eventTypes.CHAT_CHANGED, () => {
        if (voiceEnabled && context().characters[context().characterId]?.name !== 'HOLT') {
            voiceEnabled = false;
            control({ action: 'release', client }).catch(failure);
        }
    });
    c.eventSource.on(c.eventTypes.CHAT_COMPLETION_SETTINGS_READY, data => {
        if (!data.model?.startsWith('holt-')) return;
        data.stream = false;
        const mode = captureCase ? 'capture' : data.type === 'normal' ? 'chat' : 'quiet';
        data.custom_include_body = JSON.stringify({ holt: {
            mode, case: captureCase, chat: context().chatId, type: data.type,
        } });
    });
    c.eventSource.on(c.eventTypes.CHARACTER_MESSAGE_RENDERED, async () => {
        const active = context();
        if (active.chat.at(-1)?.mes?.trim() === silence) await active.deleteLastMessage();
    });
    const events = new EventSource(`${gateway}/events?client=${client}`);
    events.onmessage = async message => {
        const event = JSON.parse(message.data);
        if (event.kind !== 'transcript' || !voiceEnabled || transcriptBusy) return;
        transcriptBusy = true;
        try { await submitVoice(event.value); }
        catch (error) { failure(error); }
        finally { transcriptBusy = false; }
    };
    setInterval(poll, 1000);
    poll();
}

init();

/* Input only: use ST's active model, MCP tools and selected speech provider. */
import { isGenerating } from '/script.js';

const endpoint = 'http://127.0.0.1:8083';
const client = crypto.randomUUID();
let enabled = false, submitting = false, polling = false, events = null;
const context = () => SillyTavern.getContext();

function chatId() {
    const c = context();
    if (c.groupId || c.characters[c.characterId]?.name !== 'HOLT' || !c.chatId) {
        throw new Error('Select a HOLT character chat first');
    }
    return c.chatId;
}

function speaking() {
    const audio = document.getElementById('tts_audio');
    return Boolean(audio && !audio.paused && !audio.ended)
        || document.documentElement?.dataset.holtPocketSpeaking === 'true'
        || document.getElementById('chatterbox-status')?.textContent === 'Processing';
}

function status(value) { document.getElementById('holt-input-status').textContent = value; }

async function control(action, extra = {}) {
    const response = await fetch(endpoint + '/control', { method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action, client, ...extra }) });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Voice input request failed');
    return data;
}

async function submit(packet) {
    if (packet.chat !== chatId()) throw new Error('Chat changed; transcript discarded');
    const c = context();
    // Persist the receipt in the user message, before generation. An uncertain
    // network result or failed tool turn must not resubmit game commands.
    const duplicate = c.chat.some(m => m.extra?.holt_voice_id === packet.id);
    await control('ack', { id: packet.id });
    if (duplicate) return;
    const message = { name: c.name1, is_user: true, is_system: false,
        send_date: Date.now(), mes: packet.text, extra: { holt_voice_id: packet.id } };
    c.chat.push(message);
    c.addOneMessage(message);
    await c.saveChat();
    await c.generate('normal');
}

async function poll() {
    if (!enabled || polling) return;
    polling = true;
    try {
        // GENERATION_STARTED also fires for dry-run prompt previews, which may
        // never emit GENERATION_ENDED. Read ST's current state on every poll.
        const data = await control('poll', { chat: chatId(), busy: isGenerating() || submitting, speaking: speaking() });
        status(data.error || (!data.ptt.connected ? 'Ship event link disconnected' :
            data.capture_status && data.capture_status !== 'Ready' ? data.capture_status :
            data.ptt.down ? 'PTT held' : data.state));
        if (data.pending && !isGenerating() && !submitting && !speaking()) {
            submitting = true;
            try { await submit(data.pending); }
            finally { submitting = false; }
        }
    } catch (error) {
        enabled = false;
        events?.close(); events = null;
        status(error.message + ' — press Start to reconnect');
    } finally { polling = false; }
}

function init() {
    const holder = document.getElementById('extensions_settings');
    if (!holder) return;
    const panel = document.createElement('div');
    panel.className = 'inline-drawer';
    panel.innerHTML = `<div class="inline-drawer-toggle inline-drawer-header">
      <b>HOLT voice input</b><div class="inline-drawer-icon fa-solid fa-circle-chevron-down down"></div></div>
      <div class="inline-drawer-content">
        <p>Hold the configured PTT button on SKIPPY; speak into the selected microphone on ARCHIE.
        To change it, learn a button while the game is paused. Learning stops listening.</p>
        <div class="flex-container">
          <button id="holt-input-learn" class="menu_button">Learn PTT button</button>
          <button id="holt-input-configure" class="menu_button">Use Numpad Enter</button>
          <button id="holt-input-start" class="menu_button">Start listening</button>
          <button id="holt-input-stop" class="menu_button">Stop listening</button>
        </div><p id="holt-input-status">Start HOLT-ST-Voice.cmd, then start listening. Learn a button to change PTT.</p>
      </div>`;
    holder.append(panel);
    for (const [id, action] of [['holt-input-configure', 'configure'], ['holt-input-learn', 'learn']]) {
        const button = document.getElementById(id);
        button.onclick = async () => {
            enabled = false; events?.close(); events = null;
            button.disabled = true;
            status(action === 'learn' ? 'Press the desired PTT button on SKIPPY within 10 seconds…' : 'Saving Numpad Enter…');
            try { const data = await control(action); status(data.state); }
            catch (error) { status(error.message); }
            finally { button.disabled = false; }
        };
    }
    document.getElementById('holt-input-start').onclick = async () => {
        try {
            const data = await control('start', { chat: chatId() });
            enabled = true; status(data.state);
            events?.close();
            // Incoming SSE ticks drive renewal even when the game has focus;
            // this avoids relying on heavily throttled background-tab timers.
            events = new EventSource(endpoint + '/events');
            events.onmessage = () => poll();
            events.onerror = () => { status('Voice link reconnecting; capture pauses if the lease expires'); };
            await poll();
        } catch (error) { status(error.message); }
    };
    document.getElementById('holt-input-stop').onclick = async () => {
        enabled = false; events?.close(); events = null;
        try { await control('stop'); status('Stopped'); }
        catch (error) { status(error.message); }
    };
    const c = context();
    c.eventSource.on(c.eventTypes.CHAT_CHANGED, async () => {
        if (!enabled) return;
        enabled = false; events?.close(); events = null;
        try { await control('stop'); } catch { /* Lease expires without replay. */ }
        status('Chat changed; press Start listening for this chat');
    });
}

init();

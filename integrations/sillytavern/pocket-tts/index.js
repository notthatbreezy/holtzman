import { registerTtsProvider, saveTtsProviderSettings } from '/scripts/extensions/tts/index.js';
import { extension_settings } from '/scripts/extensions.js';
import { PocketPlayer } from './player.js';

const providerName = 'HOLT Pocket';

class PocketTtsProvider {
    constructor() {
        this.settings = { provider_endpoint: 'http://127.0.0.1:8005', prebuffer_ms: 100, voiceMap: {} };
        this.ready = false;
        this.separator = ' ';
        this.player = new PocketPlayer({
            onState: (busy, label) => {
                document.documentElement.dataset.holtPocketSpeaking = String(busy);
                const status = document.getElementById('holt-pocket-status');
                if (status) status.textContent = label;
            },
            onMetrics: metrics => {
                this.lastMetrics = metrics;
                const target = document.getElementById('holt-pocket-metrics');
                if (target) target.textContent = `First audio scheduled: ${Math.round(metrics.firstScheduledMs)} ms · Gaps: ${metrics.underruns}`;
                console.info('[HOLT Pocket] Playback metrics', metrics);
            },
        });
    }

    get settingsHtml() {
        return `<p>Local K-2SO voice, continuous streaming, Windows-default output.</p>
          <label for="holt-pocket-endpoint">Pocket service</label>
          <input id="holt-pocket-endpoint" type="text" class="text_pole" />
          <label for="holt-pocket-buffer">Starting audio buffer (milliseconds)</label>
          <input id="holt-pocket-buffer" type="number" min="40" max="500" step="20" class="text_pole" />
          <div class="flex-container"><button id="holt-pocket-enable" class="menu_button">Enable audio</button>
          <button id="holt-pocket-preview" class="menu_button">Test K-2SO</button>
          <button id="holt-pocket-stop" class="menu_button">Stop audio</button></div>
          <p id="holt-pocket-status" role="status">Checking service…</p>
          <small id="holt-pocket-metrics"></small>`;
    }

    async loadSettings(settings = {}) {
        Object.assign(this.settings, settings);
        const endpoint = document.getElementById('holt-pocket-endpoint');
        const buffer = document.getElementById('holt-pocket-buffer');
        endpoint.value = this.settings.provider_endpoint;
        buffer.value = this.settings.prebuffer_ms;
        endpoint.onchange = () => {
            try {
                const url = new URL(endpoint.value);
                if (!['127.0.0.1', 'localhost'].includes(url.hostname) || url.protocol !== 'http:') throw Error('Use a local HTTP service');
                this.settings.provider_endpoint = url.origin;
                saveTtsProviderSettings();
            } catch (error) { this.showError(error); }
        };
        buffer.onchange = () => {
            this.settings.prebuffer_ms = Math.min(500, Math.max(40, Number(buffer.value) || 100));
            buffer.value = this.settings.prebuffer_ms;
            saveTtsProviderSettings();
        };
        document.getElementById('holt-pocket-enable').onclick = () => this.player.unlock()
            .then(() => { document.getElementById('holt-pocket-status').textContent = 'Audio enabled'; })
            .catch(error => this.showError(error));
        document.getElementById('holt-pocket-preview').onclick = () => this.previewTtsVoice('k2so').catch(error => this.showError(error));
        document.getElementById('holt-pocket-stop').onclick = () => this.player.stop();
        // Arm WebAudio during the first real user gesture, avoiding autoplay
        // suspension when a later generated answer starts asynchronously.
        const arm = () => this.player.unlock().catch(() => {});
        document.addEventListener('pointerdown', arm, { once: true, capture: true });
        await this.onRefreshClick();
    }

    showError(error) {
        const status = document.getElementById('holt-pocket-status');
        if (status) status.textContent = error.message;
        console.error('[HOLT Pocket]', error);
    }

    async onRefreshClick() {
        try {
            await this.checkReady();
            document.getElementById('holt-pocket-status').textContent = 'Ready';
        } catch (error) { this.showError(error); }
    }

    async checkReady() {
        const response = await fetch(this.settings.provider_endpoint + '/health');
        if (!response.ok) throw Error('Pocket service is unavailable');
        const health = await response.json();
        this.ready = health.service === 'holt-pocket-tts' && health.ready;
        if (!this.ready) throw Error('Start HOLT-Pocket.cmd and check its status');
    }

    async fetchTtsVoiceObjects() {
        const response = await fetch(this.settings.provider_endpoint + '/voices');
        if (!response.ok) throw Error('Could not load Pocket voices');
        return (await response.json()).map(voice => ({ name: voice.name, voice_id: voice.id,
            lang: voice.lang, preview_url: false }));
    }

    async getVoice(name) {
        const voice = (await this.fetchTtsVoiceObjects()).find(item => item.name === name || item.voice_id === name);
        if (!voice) throw Error(`Unknown Pocket voice: ${name}`);
        return voice;
    }

    async processText(text) {
        return text.replace(/\[\[HOLT_SILENT\]\]/g, '').trim();
    }

    async previewTtsVoice(voiceId) {
        await this.player.unlock();
        return this.player.play(this.settings.provider_endpoint,
            'Captain, docking clearance is confirmed. Try to arrive with the same number of hull panels we left with.',
            voiceId, this.settings.prebuffer_ms);
    }

    // ST natively consumes async-generator providers. This one performs PCM
    // playback itself and yields no blobs, so ST never adds tiny clips/silence
    // to its polled HTML audio queue. The TTS job stays active through playback.
    async* generateTts(text, voiceId) {
        if (text.trim()) await this.player.play(this.settings.provider_endpoint, text, voiceId, this.settings.prebuffer_ms);
    }

    dispose() { this.player.stop(); }
}

// Register before the built-in TTS extension initializes (loading_order 10), so
// a saved Pocket selection exists at reload. Its API otherwise auto-loads the
// provider before the TTS settings DOM has been created. Suppress only that
// synchronous early auto-load; ST's normal init then loads the saved selection.
extension_settings.tts ||= {};
const selectedProvider = extension_settings.tts.currentProvider;
if (!document.getElementById('tts_provider') && selectedProvider === providerName) {
    try {
        extension_settings.tts.currentProvider = '';
        registerTtsProvider(providerName, PocketTtsProvider);
    } finally {
        extension_settings.tts.currentProvider = selectedProvider;
    }
} else registerTtsProvider(providerName, PocketTtsProvider);

/* PCM chunks share the AudioContext clock; no per-clip HTML audio queue. */
export class PocketPlayer {
    constructor({ contextFactory = () => new AudioContext(), fetcher = (...args) => fetch(...args),
        now = () => performance.now(), onState = () => {}, onMetrics = () => {} } = {}) {
        Object.assign(this, { contextFactory, fetcher, now, onState, onMetrics });
        this.context = null;
        this.busy = false;
        this.sources = new Set();
    }

    async unlock() {
        this.context ||= this.contextFactory();
        if (this.context.state !== 'running') await this.context.resume();
        if (this.context.state !== 'running') throw Error('Click Enable audio in the Pocket settings');
        return this.context;
    }

    stop() {
        this.controller?.abort();
        for (const source of this.sources) { try { source.stop(); } catch { /* Already ended. */ } }
    }

    async play(endpoint, text, voice, prebufferMs = 100, effects = {}) {
        if (this.busy) throw Error('Pocket speech is already playing');
        this.busy = true;
        this.onState(true, 'Generating');
        this.controller = new AbortController();
        const completed = [];
        let reader;
        try {
            const context = await this.unlock();
            const started = this.now();
            const response = await this.fetcher(endpoint + '/stream', { method: 'POST',
                headers: { 'Content-Type': 'application/json' }, signal: this.controller.signal,
                body: JSON.stringify({ ...effects, text, voice }) });
            if (!response.ok) {
                let reason = `HTTP ${response.status}`;
                try { reason = (await response.json()).detail || reason; } catch { /* Non-JSON error. */ }
                throw Error(`Pocket: ${reason}`);
            }
            if (!response.body) throw Error('Pocket response has no audio stream');
            reader = response.body.getReader();
            const decoder = new TextDecoder();
            let pending = '', rate = 0, nextTime = 0, frames = 0, chunks = 0, ended = false;
            const metrics = { firstChunkMs: null, firstScheduledMs: null, underruns: 0, gapMs: 0 };
            const consume = line => {
                if (!line.trim()) return;
                const item = JSON.parse(line);
                if (ended) throw Error('Pocket sent audio after completion');
                if (item.type === 'start') {
                    if (rate || item.format !== 's16le' || item.channels !== 1
                        || !Number.isInteger(item.sample_rate) || item.sample_rate < 8000 || item.sample_rate > 48000) {
                        throw Error('Unsupported Pocket audio format');
                    }
                    rate = item.sample_rate;
                } else if (item.type === 'audio') {
                    if (!rate || item.index !== chunks || !Number.isInteger(item.samples) || item.samples < 1) {
                        throw Error('Pocket audio sequence is invalid');
                    }
                    const binary = atob(item.data);
                    if (binary.length !== item.samples * 2) throw Error('Pocket PCM length mismatch');
                    frames += item.samples;
                    if (frames > rate * 180) throw Error('Pocket audio duration limit exceeded');
                    const buffer = context.createBuffer(1, item.samples, rate);
                    const samples = buffer.getChannelData(0);
                    for (let index = 0; index < samples.length; index++) {
                        const unsigned = binary.charCodeAt(index * 2) | binary.charCodeAt(index * 2 + 1) << 8;
                        samples[index] = (unsigned >= 32768 ? unsigned - 65536 : unsigned) / 32768;
                    }
                    if (!chunks) {
                        metrics.firstChunkMs = this.now() - started;
                        nextTime = context.currentTime + prebufferMs / 1000;
                        metrics.firstScheduledMs = metrics.firstChunkMs + prebufferMs;
                    } else if (nextTime < context.currentTime) {
                        metrics.underruns++;
                        metrics.gapMs += (context.currentTime - nextTime) * 1000;
                        nextTime = context.currentTime + 0.01;
                    }
                    const source = context.createBufferSource();
                    source.buffer = buffer;
                    source.connect(context.destination);
                    this.sources.add(source);
                    completed.push(new Promise(resolve => {
                        source.onended = () => { this.sources.delete(source); source.disconnect(); resolve(); };
                    }));
                    source.start(nextTime);
                    nextTime += item.samples / rate;
                    chunks++;
                    this.onState(true, 'Playing');
                } else if (item.type === 'end') {
                    if (!chunks || item.chunks !== chunks) throw Error('Pocket stream ended without complete audio');
                    metrics.generationMs = item.generation_ms;
                    ended = true;
                } else if (item.type === 'error') {
                    throw Error(item.message || 'Pocket synthesis failed');
                } else throw Error('Unknown Pocket stream message');
            };
            while (true) {
                const { value, done } = await reader.read();
                pending += done ? decoder.decode() : decoder.decode(value, { stream: true });
                let newline;
                while ((newline = pending.indexOf('\n')) !== -1) {
                    consume(pending.slice(0, newline));
                    pending = pending.slice(newline + 1);
                }
                if (pending.length > 262144) throw Error('Pocket stream record is too large');
                if (done) break;
            }
            if (pending.trim()) consume(pending);
            if (!ended) throw Error('Pocket audio stream disconnected before completion');
            await Promise.all(completed);
            Object.assign(metrics, { chunks, audioSeconds: frames / rate,
                totalMs: this.now() - started, outputLatencyMs: (context.outputLatency || 0) * 1000 });
            this.onMetrics(metrics);
            return metrics;
        } catch (error) {
            this.stop();
            if (error.name !== 'AbortError') throw error;
            return null;
        } finally {
            await reader?.cancel().catch(() => {});
            this.busy = false;
            this.controller = null;
            this.onState(false, 'Ready');
        }
    }
}

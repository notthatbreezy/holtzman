"""Nonblocking turn checks with stale-result rejection and fixed-mode fallback."""
import concurrent.futures
import time

class TurnWorker:
    def __init__(self, predict=None, clock=time.monotonic):
        self.predict = predict
        self.clock = clock
        self.pool = concurrent.futures.ThreadPoolExecutor(max_workers=1, thread_name_prefix="holt-turn")
        self.pending = None
        self.generation = 0
        self.slow = 0
        self.retry_at = 0
        self.reason = "model unavailable" if predict is None else None

    def cancel(self):
        self.generation += 1
        if self.pending:
            self.pending[0].cancel()

    def check(self, pcm, threshold):
        now = self.clock()
        if not self.predict or now < self.retry_at:
            return None
        if self.pending:
            future, started, generation = self.pending
            if not future.done():
                if now - started > 1:
                    self.retry_at = now + 300
                    self.reason = "model unavailable"
                    return None
                if now - started > .15:
                    # Count one slow inference once, not once per audio frame.
                    if not getattr(future, "holt_slow", False):
                        future.holt_slow = True
                        self.slow += 1
                        if self.slow >= 3:
                            self.retry_at = now + 300
                            self.reason = "model slow"
                return False
            self.pending = None
            if future.cancelled() or generation != self.generation:
                return False
            try:
                score, elapsed = future.result()
            except Exception:
                self.retry_at = now + 300
                self.reason = "model unavailable"
                return None
            if generation == self.generation:
                if elapsed > .15 and not getattr(future, "holt_slow", False):
                    self.slow += 1
                    if self.slow >= 3:
                        self.retry_at = now + 300
                        self.reason = "model slow"
                        return None
                self.reason = None
                if elapsed <= .15:
                    self.slow = 0
                return float(score) >= threshold
        def predict():
            started = time.perf_counter()
            score = self.predict(pcm)
            return score, time.perf_counter() - started
        self.pending = (self.pool.submit(predict), now, self.generation)
        return False

    def close(self):
        self.cancel()
        self.pool.shutdown(wait=False, cancel_futures=True)


MODEL_SHA256 = "07a133aba31e2d0b523f17f8c2e4e65efe6d8f685efd12ca4fe21ebf4e798991"
MODEL_URL = "https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/100f018c9e4302f7e0f39ea49f0fe7b075b918e6/smart-turn-v3.0.onnx"

class SmartTurn:
    """Standalone Smart Turn v3.0, following the upstream 8-second Whisper features.

    Reference: https://github.com/pipecat-ai/smart-turn/blob/main/inference.py
    No network access or model downloads occur during construction.
    """
    def __init__(self, path, checksum=MODEL_SHA256):
        import hashlib
        from pathlib import Path
        import onnxruntime as ort
        from transformers import WhisperFeatureExtractor
        if hashlib.sha256(Path(path).read_bytes()).hexdigest() != checksum:
            raise ValueError("Smart Turn model checksum mismatch")
        options = ort.SessionOptions()
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        options.inter_op_num_threads = options.intra_op_num_threads = 1
        self.session = ort.InferenceSession(str(path), sess_options=options, providers=["CPUExecutionProvider"])
        self.features = WhisperFeatureExtractor(chunk_length=8)

    def __call__(self, pcm):
        import numpy as np
        samples = np.frombuffer(pcm, dtype=np.int16).astype(np.float32) / 32768
        samples = samples[-128000:]
        samples = np.pad(samples, (max(0, 128000 - len(samples)), 0))
        features = self.features(samples, sampling_rate=16000, return_tensors="np", padding="max_length",
                                 max_length=128000, truncation=True, do_normalize=True).input_features
        score = float(self.session.run(None, {"input_features": features.astype(np.float32)})[0][0].item())
        if not np.isfinite(score) or not 0 <= score <= 1:
            raise ValueError("Invalid completion probability")
        return score

def worker(settings, report):
    from client.state import ROOT
    path = settings.get("smart_turn_model") or ROOT / "models/smart-turn-v3.0.onnx"
    model = None
    def predict(pcm):
        nonlocal model
        if model is None:
            model = SmartTurn(path, settings.get("smart_turn_sha256") or MODEL_SHA256)
        return model(pcm)
    return TurnWorker(predict if settings.get("endpoint_mode") == "adaptive" else None)

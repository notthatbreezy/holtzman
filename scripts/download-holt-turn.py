"""Download the pinned standalone Smart Turn model; does not change client settings."""
import hashlib
import sys
from pathlib import Path
import urllib.request
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from client.endpointing import MODEL_URL, MODEL_SHA256
from client.state import ROOT

def main():
    destination = ROOT / "models/smart-turn-v3.0.onnx"
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and hashlib.sha256(destination.read_bytes()).hexdigest() == MODEL_SHA256:
        print("Pinned Smart Turn model already verified.")
        return
    partial = destination.with_suffix(".part")
    with urllib.request.urlopen(MODEL_URL, timeout=60) as source, partial.open("wb") as target:
        total = 0
        while chunk := source.read(1024 * 1024):
            total += len(chunk)
            if total > 16 * 1024 * 1024:
                raise ValueError("Model exceeds expected size bound")
            target.write(chunk)
    if hashlib.sha256(partial.read_bytes()).hexdigest() != MODEL_SHA256:
        partial.unlink()
        raise ValueError("Smart Turn checksum mismatch; nothing installed")
    partial.replace(destination)
    print("Pinned Smart Turn model verified. Adaptive endpointing remains opt-in.")

if __name__ == "__main__":
    main()

"""Resume and verify the pinned HOLT model using four HTTP range requests.

Do not run alongside another downloader writing these files.
Downloads stay under the ignored .local/holt directory.
"""
import concurrent.futures
import hashlib
import json
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / ".local/holt/models"
REVISION = "bc014a17be43adabd7066b7a86075ff935c6a4e2"
FILES = [
    ("Qwen3.5-35B-A3B-Q4_K_M.gguf", 22016023168,
     "3b46d1066bc91cc2d613e3bc22ce691dd77e6f0d33c9060690d24ce6de494375"),
    ("mmproj-F16.gguf", 899283648,
     "a516ab92e8240da4734d68352bdfba84c16e830ee40010b8fac80d69c77272ff"),
]


def fetch_range(url, path, start, end):
    size = end - start + 1
    for attempt in range(4):
        have = path.stat().st_size if path.exists() else 0
        if have == size:
            return path
        if have > size:
            raise RuntimeError(f"Unexpected partial-file size: {path}")
        position = start + have
        try:
            tail = path.with_suffix(".tail")
            result = subprocess.run([
                "curl.exe", "-L", "--fail", "--silent", "--show-error",
                "--max-time", "1800", "--speed-limit", "1024", "--speed-time", "60",
                "--range", f"{position}-{end}", "--output", str(tail),
                "--write-out", "%{http_code}", url,
            ], capture_output=True, text=True)
            if result.stdout.strip() != "206":
                raise RuntimeError(f"Server did not honor byte range: {result.stdout}")
            if tail.exists():
                if tail.stat().st_size > size - have:
                    raise RuntimeError("Range response exceeds expected size")
                with path.open("ab") as output, tail.open("rb") as source:
                    shutil.copyfileobj(source, output, length=4 * 1024 * 1024)
            if result.returncode:
                raise RuntimeError(f"curl download failed: {result.stderr}")
            if path.stat().st_size != size:
                raise RuntimeError("Incomplete range response")
            print(f"Finished {path.name}", flush=True)
            return path
        except Exception:
            if attempt == 3:
                raise
            time.sleep(2)


def download(name, size, expected):
    ROOT.mkdir(parents=True, exist_ok=True)
    target = ROOT / name
    have = target.stat().st_size if target.exists() else 0
    if have > size:
        raise RuntimeError(f"Unexpected model size: {target}")
    if have < size:
        # Persist boundaries before fetching: a failed run can reuse its chunks.
        plan_path = ROOT / (name + ".ranges")
        if plan_path.exists():
            plan = json.loads(plan_path.read_text())
            if plan[0][0] != have:
                raise RuntimeError("Partial model changed since ranges were planned")
        else:
            step = (size - have + 3) // 4
            plan = [(start, min(start + step, size) - 1)
                    for start in range(have, size, step)]
            plan_path.write_text(json.dumps(plan))
        url = f"https://huggingface.co/unsloth/Qwen3.5-35B-A3B-GGUF/resolve/{REVISION}/{name}"
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            chunks = list(executor.map(
                lambda bounds: fetch_range(url, ROOT / f"{name}.{bounds[0]}.part", *bounds), plan))
        # Assemble separately so interruption never changes the resumable prefix.
        assembled = ROOT / (name + ".assembled")
        with assembled.open("wb") as output:
            if have:
                with target.open("rb") as prefix:
                    shutil.copyfileobj(prefix, output, length=4 * 1024 * 1024)
            for chunk in chunks:
                with chunk.open("rb") as source:
                    shutil.copyfileobj(source, output, length=4 * 1024 * 1024)
        candidate = assembled
    else:
        candidate = target
    print(f"Verifying SHA-256: {name}", flush=True)
    with candidate.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if candidate.stat().st_size != size or actual != expected:
        raise RuntimeError(f"Checksum mismatch: {name}")
    if candidate != target:
        candidate.replace(target)
    (ROOT / (name + ".sha256")).write_text(f"{actual}  {name}\n")
    # Delete only this download's exact temporary paths after checksum success.
    plan_path = ROOT / (name + ".ranges")
    if plan_path.exists():
        for start, _ in json.loads(plan_path.read_text()):
            for suffix in ("part", "tail"):
                (ROOT / f"{name}.{start}.{suffix}").unlink(missing_ok=True)
        plan_path.unlink()
    print(f"Verified {name}: {actual}", flush=True)


if __name__ == "__main__":
    for entry in FILES:
        download(*entry)

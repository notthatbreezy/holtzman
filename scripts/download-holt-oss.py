"""Resume and verify the published MXFP4 GPT-OSS 120B GGUF; no credentials needed."""
import hashlib
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
import sys
import time

import httpx2

ROOT = Path(__file__).resolve().parents[1] / ".local/holt/models/gpt-oss-120b"
SIZE = 65369017728
SHA = "2d1f0298ae4b6c874d5a468598c5ce17c1763b3fea99de10b1a07df93cef014f"
URL = "https://huggingface.co/unsloth/gpt-oss-120b-GGUF/resolve/main/gpt-oss-120b-F16.gguf"


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    final = ROOT / "gpt-oss-120b-F16.gguf"
    partial = final.with_suffix(".gguf.partial")
    if not final.exists():
        pieces = ROOT / "pieces"
        pieces.mkdir(exist_ok=True)
        size = 256 * 1024 * 1024
        count = (SIZE + size - 1) // size
        # Reuse completed ranges from the earlier sequential download.
        previous = partial.stat().st_size if partial.exists() else 0
        if previous:
            with partial.open("rb") as source:
                for number in range(previous // size):
                    target = pieces / f"{number:04d}.part"
                    if not target.exists() or target.stat().st_size != size:
                        with target.open("wb") as out:
                            for _ in range(size // (4 * 1024 * 1024)):
                                out.write(source.read(4 * 1024 * 1024))
                    else:
                        source.seek(size, 1)

        def fetch(number):
            first, end = number * size, min(SIZE, (number + 1) * size) - 1
            target = pieces / f"{number:04d}.part"
            if target.exists() and target.stat().st_size == end - first + 1:
                return end - first + 1
            for attempt in range(6):
                offset = target.stat().st_size if target.exists() else 0
                try:
                    with httpx2.Client(trust_env=False, timeout=120, follow_redirects=True) as http:
                        start = first + offset
                        with http.stream("GET", URL + f"?download=true&range={start}-{end}", headers={"Range": f"bytes={start}-{end}"}) as response:
                            response.raise_for_status()
                            if response.status_code != 206 or not response.headers.get("Content-Range", "").startswith(f"bytes {start}-"):
                                raise RuntimeError("Server did not honor range")
                            with target.open("ab") as out:
                                for chunk in response.iter_bytes(4 * 1024 * 1024):
                                    out.write(chunk)
                    if target.stat().st_size == end - first + 1:
                        return end - first + 1
                except (httpx2.HTTPError, OSError, RuntimeError):
                    time.sleep(3)
            raise RuntimeError(f"Range {number} incomplete; rerun to resume")

        completed = 0
        last = time.monotonic()
        with ThreadPoolExecutor(max_workers=16) as pool:
            jobs = [pool.submit(fetch, number) for number in range(count)]
            for job in as_completed(jobs):
                completed += job.result()
                if time.monotonic() - last > 15:
                    print(f"Downloaded {completed / 1e9:.2f}/{SIZE / 1e9:.2f} GB", flush=True)
                    last = time.monotonic()
        assembled = final.with_suffix(".gguf.assembling")
        with assembled.open("wb") as out:
            for number in range(count):
                with (pieces / f"{number:04d}.part").open("rb") as source:
                    while chunk := source.read(8 * 1024 * 1024):
                        out.write(chunk)
        candidate = assembled
    else:
        candidate = final
    digest = hashlib.file_digest(candidate.open("rb"), "sha256").hexdigest()
    if digest != SHA:
        raise SystemExit("Checksum mismatch; model will not be loaded")
    if candidate != final:
        partial.replace(final)
    print("Verified", final.name, SIZE, SHA, flush=True)


if __name__ == "__main__":
    main()

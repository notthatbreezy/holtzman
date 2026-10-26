# Omarchy installation guide for the local HOLT client

Written October 25, 2026. Follow the steps in order. Linux commands run on
the **Omarchy PC**, in Bash, unless a step explicitly says **SKIPPY / PowerShell**.
Open a terminal and enter `bash` if you normally use another shell.

## What this installs

This guide prepares and independently tests the components for the
[local voice architecture](local-voice-architecture.md): a local language
model, speech recognition, speech synthesis, microphone and speaker access,
and an authenticated read-only connection to Holtzman.

**The combined `holtzman-voice` application has not been implemented.**
Completing these steps gives us a tested foundation, not a finished voice
assistant. There is no `holtzman-voice` launch command yet. Pipecat and the
controller will be installed from that application's tested dependency lock
when it exists. The final step describes that remaining work.

The Linux commands have been checked against upstream documentation and
reviewed for syntax. They have not been executed on your Omarchy hardware.
Internet access is needed for installation and model downloads; inference
afterward runs locally. Spansh searches continue to need internet access.

Allow roughly 80 GB of free disk space for models, build files, Python
packages and download caches. The selected language-model file is about
22 GB. Exact installation time depends on download and build speeds.

## 1. Keep the working Holtzman server on SKIPPY

Do not reinstall Holtzman or reset its recipe for this setup. Keep the
existing authentication, allowed hosts, input allow-list, bindings and click
calibration.

**SKIPPY / PowerShell:** check whether the server is already listening:

```powershell
Get-Process holtzman -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 7777 -State Listen -ErrorAction SilentlyContinue
```

If Holtzman is already listening, leave it running. Otherwise, inspect the
installed CLI and start it with the existing config:

```powershell
holtzman --help
holtzman serve --help
holtzman --config "$HOME\holtzman\holtzman.toml" serve
```

Leave that terminal running. Starting the server sends no game input.
The endpoint remains `http://skippy:7777/mcp`. Do not run `init`,
`reset-recipe` or `plot` as part of installation.

## 2. Update Omarchy and install build dependencies

Use **Super + Space -> Update -> Omarchy**, let the update finish, and reboot
if requested. This follows the
[Omarchy update procedure](https://omarchy.org/manual/updates/). Complete
the update before installing packages; do not do a standalone `pacman -Sy`.

In a Bash terminal on Omarchy:

```bash
sudo pacman -S --needed \
  base-devel git cmake ninja curl jq uv pciutils ripgrep \
  vulkan-radeon vulkan-tools vulkan-headers vulkan-icd-loader \
  shaderc spirv-headers ffmpeg portaudio libsndfile espeak-ng
```

This uses Mesa's [AMD Vulkan driver](https://archlinux.org/packages/extra/x86_64/vulkan-radeon/).
Arch's [shaderc package](https://archlinux.org/packages/extra/x86_64/shaderc/files/)
provides `glslc`; it is not a separate `glslc` package here. No ROCm installation
or NPU setup is required for this first pass.

Check the machine and graphics backend:

```bash
lscpu
free -h
df -h "$HOME"
lspci -nnk
vulkaninfo --summary
command -v glslc
```

**Continue when:** the CPU and installed RAM match your machine, there is
enough disk space, and Vulkan lists the AMD GPU. A software renderer such
as llvmpipe alone is not the desired result. If Vulkan fails, fix that
before building the model runtime. Do not change firmware memory settings
based only on the advertised RAM size.

## 3. Create the local directories and Python environment

These paths stay outside any source checkout:

```bash
mkdir -p "$HOME/.local/share/holtzman-voice/src"
mkdir -p "$HOME/.local/share/holtzman-voice/models"
mkdir -p "$HOME/.local/share/holtzman-voice/tests"
install -d -m 700 "$HOME/.config/holtzman-voice"

uv python install 3.12
uv venv --python 3.12 "$HOME/.local/share/holtzman-voice/.venv"

uv pip install \
  --python "$HOME/.local/share/holtzman-voice/.venv/bin/python" \
  'mcp==2.3.0' httpx2 huggingface_hub
```

If the virtual environment already exists, reuse it rather than recreating
it. Python 3.12 gives this setup a consistent starting point independent of
Arch's system Python. The smoke test below uses the
[MCP Python SDK v2 interface](https://github.com/modelcontextprotocol/python-sdk/blob/main/examples/snippets/clients/streamable_basic.py);
older v1 examples return a different transport tuple.

## 4. Check the LAN connection and save the credential privately

```bash
getent hosts skippy
curl --silent --show-error --connect-timeout 5 \
  --output /dev/null --write-out '%{http_code}\n' \
  http://skippy:7777/mcp
```

**Expected:** the hostname resolves and the unauthenticated request returns
`401`. This proves connectivity, not a complete MCP handshake.

If `skippy` does not resolve, find SKIPPY's active LAN IPv4 address using
`ipconfig` on Windows. On Omarchy, run `sudoedit /etc/hosts` and add a line
containing that actual address followed by `skippy`. Keep using the hostname
in the URL so it matches the server's existing `allowed_hosts` entry.

If the connection times out, check the server and Windows Firewall's Private
network rule for TCP 7777. Scope network access to the Omarchy PC as
appropriate; `allowed_hosts` checks the HTTP Host, not the client's source IP.
The existing HTTP endpoint is for the trusted LAN; use TLS or an encrypted
tunnel before sending its bearer credential across an untrusted network.

Open `C:\Users\chris\holtzman\holtzman.toml` locally on SKIPPY in an editor.
Transfer only the token value through a private method such as your password
manager. Do not paste the token into chat, put it in a command, or copy the
whole config to Omarchy. Enter the value without surrounding TOML quotes or
the word `Bearer` when prompted below:

```bash
"$HOME/.local/share/holtzman-voice/.venv/bin/python" - <<'PY'
import getpass
import os
from pathlib import Path

token = getpass.getpass("Holtzman token (hidden): ").strip()
if not token or token.startswith("Bearer ") or "\n" in token or "\r" in token:
    raise SystemExit("Enter the token value only, on one line.")
path = Path.home() / ".config/holtzman-voice/token"
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as output:
    output.write(token)
path.chmod(0o600)
print("Credential saved privately.")
PY
```

Typing or pasting at the hidden prompt does not put the token in shell
history. Keep this file out of source control. Clear any temporary clipboard
or file used for the transfer afterward.

## 5. Prove the MCP connection with a read-only status check

Create a small diagnostic client:

```bash
cat > "$HOME/.local/share/holtzman-voice/tests/check_mcp.py" <<'PY'
import asyncio
import logging
from pathlib import Path

import httpx2
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client


async def main():
    token = (Path.home() / ".config/holtzman-voice/token").read_text().strip()
    if not token:
        raise RuntimeError("Empty credential file")
    async with asyncio.timeout(30):
        async with httpx2.AsyncClient(
            headers={"Authorization": f"Bearer {token}"},
            timeout=15,
            follow_redirects=False,
            trust_env=False,
        ) as http:
            async with streamable_http_client(
                "http://skippy:7777/mcp", http_client=http
            ) as (read, write):
                async with ClientSession(read, write) as session:
                    await session.initialize()
                    tools = await session.list_tools()
                    print("Connected. Tools:", ", ".join(sorted(t.name for t in tools.tools)))
                    result = await session.call_tool("status", {})
                    data = result.model_dump(mode="json", by_alias=True)
                    if data.get("isError"):
                        raise RuntimeError("The status tool returned an error")
                    for block in data.get("content", []):
                        if block.get("type") == "text":
                            print(block["text"])


if __name__ == "__main__":
    logging.disable(logging.CRITICAL)
    try:
        asyncio.run(main())
    except Exception as exc:
        print("MCP check failed:", type(exc).__name__)
        print("Check the server, hostname, token and allowed Host; see guide troubleshooting.")
        raise SystemExit(1) from None
PY

"$HOME/.local/share/holtzman-voice/.venv/bin/python" \
  "$HOME/.local/share/holtzman-voice/tests/check_mcp.py"
```

**Continue when:** it lists the live server's tools and returns ship status.
This script only lists tools and calls `status`; it sends no controls and
does not pass your token to a model. Tool counts can differ as Holtzman grows.
The status data comes from game files, so check freshness before treating it
as live if Elite is closed.

## 6. Build llama.cpp with Vulkan

Use a new source directory, not an existing checkout with local changes:

```bash
git clone --depth 1 --branch v0.7.0 \
  https://github.com/ggml-org/llama.cpp.git \
  "$HOME/.local/share/holtzman-voice/src/llama.cpp"

cmake \
  -S "$HOME/.local/share/holtzman-voice/src/llama.cpp" \
  -B "$HOME/.local/share/holtzman-voice/src/llama.cpp/build" \
  -G Ninja -DCMAKE_BUILD_TYPE=Release -DGGML_VULKAN=ON

cmake --build "$HOME/.local/share/holtzman-voice/src/llama.cpp/build" \
  --target llama-server --parallel 8

"$HOME/.local/share/holtzman-voice/src/llama.cpp/build/bin/llama-server" --list-devices
```

The pinned [v0.7.0 release](https://github.com/ggml-org/llama.cpp/releases/tag/v0.7.0)
provides a reproducible starting point. The upstream
[Vulkan build instructions](https://github.com/ggml-org/llama.cpp/blob/v0.7.0/docs/build.md)
describe the backend. **Continue when:** the server builds and lists the AMD
GPU. Save the device output if memory allocation later fails.

## 7. Download and run the language model

This downloads the specific Q4_K_M file from
[Unsloth's Qwen3.5-35B-A3B quantization repository](https://huggingface.co/unsloth/Qwen3.5-35B-A3B-GGUF/tree/main).
No Hugging Face login should be needed for this public model.

```bash
"$HOME/.local/share/holtzman-voice/.venv/bin/hf" download \
  unsloth/Qwen3.5-35B-A3B-GGUF \
  Qwen3.5-35B-A3B-Q4_K_M.gguf \
  --local-dir "$HOME/.local/share/holtzman-voice/models/qwen"

sha256sum \
  "$HOME/.local/share/holtzman-voice/models/qwen/Qwen3.5-35B-A3B-Q4_K_M.gguf" \
  > "$HOME/.local/share/holtzman-voice/models/qwen/model.sha256"
```

The checksum records the file you installed for later comparison. Create
one reusable startup script:

```bash
cat > "$HOME/.local/share/holtzman-voice/start-model.sh" <<'BASH'
#!/usr/bin/env bash
set -eu
exec "$HOME/.local/share/holtzman-voice/src/llama.cpp/build/bin/llama-server" \
  -m "$HOME/.local/share/holtzman-voice/models/qwen/Qwen3.5-35B-A3B-Q4_K_M.gguf" \
  --alias holt \
  --host 127.0.0.1 --port 8080 \
  -ngl 99 -c 8192 --parallel 1 \
  --jinja --chat-template-kwargs '{"enable_thinking":false}'
BASH
chmod 755 "$HOME/.local/share/holtzman-voice/start-model.sh"
"$HOME/.local/share/holtzman-voice/start-model.sh"
```

Leave this terminal open. Initial loading may take a while. Watch for the
AMD Vulkan backend and GPU offload in its startup log. An allocation failure
is a hardware/backend configuration issue to investigate, not proof that
the PC's entire RAM capacity is usable by Vulkan. Keep the 8K context for
the first test. This launch is text-only; it does not enable screenshot
interpretation.

The [server options](https://github.com/ggml-org/llama.cpp/blob/v0.7.0/tools/server/README.md)
include tool calling with Jinja templates. Disabling thinking here follows
the [quantization publisher's model instructions](https://huggingface.co/unsloth/Qwen3.5-35B-A3B-GGUF/blob/main/README.md)
and is an initial latency choice to evaluate, not a claim about tool accuracy.

In a **second Omarchy terminal**, enter `bash`, then:

```bash
curl --fail --silent --show-error http://127.0.0.1:8080/health

curl --fail --silent --show-error \
  http://127.0.0.1:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  --data-binary @- <<'JSON' | jq '.choices[0].message.content'
{
  "model": "holt",
  "messages": [{"role": "user", "content": "Reply with: Local model ready."}],
  "max_tokens": 64,
  "temperature": 0
}
JSON
```

**Continue when:** health reports OK and a local answer is returned. The
endpoint is bound to loopback; no other PC needs access to it.

Now test a proposed tool call without executing any MCP tool:

```bash
curl --fail --silent --show-error \
  http://127.0.0.1:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  --data-binary @- <<'JSON' | jq '.choices[0].message | {content, tool_calls}'
{
  "model": "holt",
  "messages": [{"role": "user", "content": "Call status to find my current location. Do not guess."}],
  "tools": [{"type": "function", "function": {
    "name": "status",
    "description": "Read current Elite Dangerous ship status.",
    "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
  }}],
  "tool_choice": "auto",
  "max_tokens": 256,
  "temperature": 0
}
JSON
```

**Expected:** a structured `tool_calls` entry naming `status` with empty
arguments, rather than an invented location. This only checks one simple
case. It does not connect the model to Holtzman or certify action reliability.

## 8. Test the microphone and speech recognition

Check PipeWire's devices:

```bash
wpctl status
```

Use Omarchy's sound controls to select the microphone and speakers connected
to this PC. Record a short phrase, then press **Ctrl+C** to finish:

```bash
pw-record --rate 16000 --channels 1 \
  "$HOME/.local/share/holtzman-voice/tests/mic-test.wav"
```

Say: "HOLT, plot a course to AD Leonis from Ross 248." Play it back:

```bash
pw-play "$HOME/.local/share/holtzman-voice/tests/mic-test.wav"
```

If the recording is silent or distorted, resolve the microphone selection
and gain before testing transcription. If `wpctl`, `pw-record` or `pw-play`
is missing, check the existing PipeWire/WirePlumber installation rather than
switching the machine to a different audio server.

Build [whisper.cpp](https://github.com/ggml-org/whisper.cpp) for an initial CPU
baseline and download its English small model:

```bash
git clone --depth 1 https://github.com/ggml-org/whisper.cpp.git \
  "$HOME/.local/share/holtzman-voice/src/whisper.cpp"

git -C "$HOME/.local/share/holtzman-voice/src/whisper.cpp" rev-parse HEAD \
  > "$HOME/.local/share/holtzman-voice/tests/whisper-revision.txt"

cmake \
  -S "$HOME/.local/share/holtzman-voice/src/whisper.cpp" \
  -B "$HOME/.local/share/holtzman-voice/src/whisper.cpp/build" \
  -G Ninja -DCMAKE_BUILD_TYPE=Release -DGGML_VULKAN=OFF

cmake --build "$HOME/.local/share/holtzman-voice/src/whisper.cpp/build" \
  --target whisper-cli --parallel 8

cd "$HOME/.local/share/holtzman-voice/src/whisper.cpp"
bash models/download-ggml-model.sh small.en

./build/bin/whisper-cli \
  -m models/ggml-small.en.bin \
  -f "$HOME/.local/share/holtzman-voice/tests/mic-test.wav" \
  -l en -t 8 \
  --prompt 'Elite Dangerous. HOLT. AD Leonis. Ross 248. Homer Platform.'
```

**Continue when:** the phrase is recognizable and the system names are
usable. Record any recurring name errors. The name prompt is recognition
context, not a guarantee. Test again with normal game audio playing.
This file-based smoke test does not yet configure continuous listening,
Silero detection, a wake word or push-to-talk integration.

## 9. Test local speech synthesis

Keep synthesis on the CPU initially, leaving the GPU for the language model.
Install CPU PyTorch and the
[Kokoro Python package](https://github.com/hexgrad/kokoro):

```bash
uv pip install \
  --python "$HOME/.local/share/holtzman-voice/.venv/bin/python" \
  torch --index-url https://download.pytorch.org/whl/cpu

uv pip install \
  --python "$HOME/.local/share/holtzman-voice/.venv/bin/python" \
  'kokoro==0.9.4' soundfile

"$HOME/.local/share/holtzman-voice/.venv/bin/python" - <<'PY'
from pathlib import Path
import numpy as np
import soundfile as sf
from kokoro import KPipeline

pipeline = KPipeline(lang_code="a", repo_id="hexgrad/Kokoro-82M", device="cpu")
chunks = [
    np.asarray(audio)
    for _, _, audio in pipeline("Captain, local voice systems are ready.", voice="af_heart")
]
output = Path.home() / ".local/share/holtzman-voice/tests/voice-test.wav"
sf.write(output, np.concatenate(chunks), 24000)
print("Voice test saved.")
PY

pw-play "$HOME/.local/share/holtzman-voice/tests/voice-test.wav"
```

The first synthesis downloads model, voice and language assets; subsequent
inference is local. `af_heart` is a test voice, not a final personality
choice. **Continue when:** it speaks clearly through the Omarchy speakers.
Any automatic language-data download must also complete before offline use.

Record the installed Python versions:

```bash
uv pip freeze --python "$HOME/.local/share/holtzman-voice/.venv/bin/python" \
  > "$HOME/.local/share/holtzman-voice/tests/python-packages.txt"
```

## 10. Bring over the existing HOLT prompt

The current personality file on SKIPPY is:

```text
C:\Users\chris\holtzman\holtzman-screenshot\prompts\ship-computer.md
```

Copy **that file only** to Omarchy using a file share, USB drive or another
private transfer method. Save it as:

```text
~/.config/holtzman-voice/ship-computer.md
```

The edited prompt is in the screenshot development checkout; cloning main
alone does not retrieve that local revision. You do not need another
Holtzman server, Rust installation or Windows config on Omarchy.

The voice application will load this prompt when it is implemented. The
llama.cpp service does not automatically load it merely because it exists
on disk. Its tool catalogue must be reconciled with the connected server;
the text-only model launch above cannot inspect screenshots.

## 11. Optionally start the model automatically at login

Do this only after the manual model checks pass. Stop the model in its
original terminal with **Ctrl+C** so port 8080 is free, then:

```bash
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/holt-model.service" <<'UNIT'
[Unit]
Description=HOLT local language model

[Service]
Type=simple
ExecStart=%h/.local/share/holtzman-voice/start-model.sh
Restart=on-failure
RestartSec=5
TimeoutStopSec=20

[Install]
WantedBy=default.target
UNIT

systemctl --user daemon-reload
systemctl --user enable --now holt-model.service
systemctl --user status holt-model.service --no-pager
```

Wait for model loading, then recheck `/health` with the command from step 7.
For logs or to stop automatic startup:

```bash
journalctl --user -u holt-model.service -n 60 --no-pager
systemctl --user disable --now holt-model.service
```

This service starts only the model. It has no Holtzman token and cannot
send game input. The future voice application's audio service should run
inside the logged-in user's desktop session as well.

## 12. What is ready, and what comes next

At this point you should have five independent successes:

1. An authenticated MCP connection and a read-only ship-status result.
2. A local model answering text and proposing a structured `status` call.
3. A clear microphone recording and usable local transcription.
4. Local speech playing through the Omarchy speakers.
5. The existing HOLT system prompt in the intended configuration directory.

The next development task is the small `holtzman-voice` controller: connect
these components using Pipecat, load the prompt, maintain the MCP session,
validate tool calls, and handle deliberate voice turns. It must ship with a
tested dependency lock and launch command before we call the voice setup
installed. Installing Pipecat alone does not provide this application.

Begin the integrated application with read-only status and trade
conversations. Then enable one explicitly requested, supervised route plot
using the existing calibrated recipe. Docking already uses screenshots to
verify selection; enable and validate the local model's vision input before
connecting that workflow to the voice client. The text-only model launch in
this guide does not include the matching vision projector. Reliable action
cancellation remains a separate server improvement. No game-input test is
needed to complete the preparation steps above.

## Troubleshooting

| Symptom | Next check |
| --- | --- |
| `skippy` does not resolve | LAN address and local DNS or `/etc/hosts` |
| Connection refused | Holtzman running and listening on 7777 |
| Connection times out | LAN connectivity and Windows Private firewall rule |
| Authenticated MCP check fails | Correct token without quotes/`Bearer`; actual URL Host allowed; server log for 401/403 or protocol error |
| `MCP check failed: ExceptionGroup` | Underlying transport failed; repeat the unauthenticated connectivity check, then inspect SKIPPY's server log without dumping config |
| `glslc` missing | Install `shaderc`; rerun CMake |
| Vulkan sees only software rendering | AMD graphics driver and kernel; do not proceed with a presumed GPU setup |
| Model cannot allocate memory | Actual Vulkan device memory, other GPU workloads, context size and offload; collect the startup log before changing firmware |
| Model output contains raw tool markup | Model/template/parser mismatch; do not wire it to game actions |
| Port 8080 is already in use | Another model process or service; stop the duplicate or consistently change the port |
| Silent microphone recording | Default input, mute and input gain in Omarchy sound controls |
| TTS fails on phonemes or shared libraries | `espeak-ng`, `libsndfile`, and the selected Python environment |
| A source directory already exists | Inspect and reuse it deliberately; do not delete it or reset another checkout |

For follow-up, hardware/Vulkan output and sanitized error messages are
useful. Never share the token file or a full `holtzman.toml`.

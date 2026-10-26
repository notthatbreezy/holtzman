# Windows installation guide for the local HOLT client

Written October 25, 2026. This version runs the ship computer on a **second
Windows 11 x64 PC**, including the AMD machine if you choose Windows for it.
SKIPPY continues to run Elite Dangerous and the existing Holtzman server.
The microphone and ship-computer speakers connect to the second PC.

Commands run on the **second PC**, in PowerShell 7, unless explicitly marked
**SKIPPY**. These are native Windows instructions; WSL and Docker are not
required. This is an alternative to the [Omarchy guide](omarchy-install.md).

## What is available now

These steps install and independently check the local language/vision model,
speech recognition, speech synthesis, audio devices and authenticated MCP
connection. Screenshot-guided docking is already supported by Holtzman's
current workflow; this guide includes checking local image interpretation.

**The combined `holtzman-voice` controller still needs implementation.**
Installing the components does not yet connect microphone speech to tool
calls. There is no finished HOLT voice-app installer or launch command.
The final step identifies that remaining work.

The commands are based on upstream Windows instructions and checked for
syntax. The complete setup has not been run on your second PC. Keep about
100 GB free for models, Python packages, compiler tools and build files.
Model downloads need internet access; inference runs locally. Spansh remains
an online trade-data source.

## 1. Leave the existing server on SKIPPY

**SKIPPY / PowerShell:** check the existing process and listener:

```powershell
Get-Process holtzman -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 7777 -State Listen -ErrorAction SilentlyContinue
```

If it is already listening, leave it running. Otherwise inspect the installed
CLI and start it with its existing config:

```powershell
holtzman --help
holtzman serve --help
holtzman --config "$HOME\holtzman\holtzman.toml" serve
```

Leave that terminal running. Do not reinstall the server, regenerate its
config, reset its recipe or change bindings for client installation. Preserve
the token, allowed hosts, input allow-list and recorded click targets.

All remaining setup is on the second PC unless stated otherwise.

## 2. Install Windows prerequisites

Apply Windows Update, install the graphics driver recommended by the PC's
manufacturer for its AMD GPU, and reboot. AMD also provides
[official driver downloads](https://www.amd.com/en/support/download/drivers.html).
Vulkan support comes from the graphics driver; the SDK alone does not
provide an accelerated GPU driver.

Open Windows Terminal / PowerShell. Install these tools, accepting any normal
installer elevation prompts:

```powershell
winget install --id Microsoft.PowerShell --exact
winget install --id Git.Git --exact
winget install --id Kitware.CMake --exact
winget install --id astral-sh.uv --exact
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --override '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
```

If a package is already installed, check that it is usable rather than
removing it. Wait for the Visual Studio installer to finish. The selected
[C++ workload](https://learn.microsoft.com/en-us/visualstudio/install/workload-component-id-vs-build-tools?view=vs-2022)
includes the compiler, Windows SDK and runtime components. You do not need
the full Visual Studio IDE.

Also install:

1. The Windows x64 [LunarG Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows),
   with its default development components, including shader tools.
2. The Windows x64 eSpeak NG MSI from the
   [official eSpeak NG releases](https://github.com/espeak-ng/espeak-ng/releases).
   Use its default `C:\Program Files\eSpeak NG` installation directory. This
   is the Windows dependency recommended by
   [Kokoro's installation instructions](https://github.com/hexgrad/kokoro#Windows-Installation).

Close and reopen Windows Terminal, selecting **PowerShell 7** (`pwsh`). Check:

```powershell
$PSVersionTable.PSVersion
git --version
cmake --version
uv --version
Get-CimInstance Win32_Processor | Select-Object Name
Get-CimInstance Win32_ComputerSystem | Select-Object TotalPhysicalMemory
Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion
Get-PSDrive -PSProvider FileSystem

if (-not $env:VULKAN_SDK) { throw 'Reopen the terminal after installing the Vulkan SDK.' }
& (Join-Path $env:VULKAN_SDK 'Bin\vulkaninfo.exe') --summary
& (Join-Path $env:VULKAN_SDK 'Bin\glslc.exe') --version
```

**Continue when:** PowerShell is version 7, the tools work, there is adequate
disk space, and Vulkan lists the AMD GPU. The amount of RAM advertised for
the PC is not necessarily the GPU allocation limit; benchmark the actual
configuration before changing firmware settings.

## 3. Create the client directories and Python environment

Run this path block again whenever you open another terminal for the guide:

```powershell
$HoltRoot = Join-Path $env:LOCALAPPDATA 'HoltzmanVoice'
$HoltConfig = Join-Path $env:APPDATA 'HoltzmanVoice'
$HoltPython = Join-Path $HoltRoot '.venv\Scripts\python.exe'
$HoltHF = Join-Path $HoltRoot '.venv\Scripts\hf.exe'
```

Create the directories and isolated Python environment:

```powershell
New-Item -ItemType Directory -Force -Path $HoltRoot, $HoltConfig, (Join-Path $HoltRoot 'src'), (Join-Path $HoltRoot 'models'), (Join-Path $HoltRoot 'tests') | Out-Null

uv python install 3.12
uv venv --python 3.12 (Join-Path $HoltRoot '.venv')
uv pip install --python $HoltPython 'mcp==2.3.0' httpx2 huggingface_hub keyring pywin32-ctypes sounddevice soundfile
```

Reuse an existing environment instead of recreating it if you are resuming.
No environment activation script or machine-wide Python installation is
needed. [uv supports Windows and WinGet installation](https://docs.astral.sh/uv/getting-started/installation/).

## 4. Check the connection to SKIPPY

```powershell
Resolve-DnsName skippy
Test-NetConnection -ComputerName skippy -Port 7777
curl.exe --silent --show-error --connect-timeout 5 --output NUL --write-out '%{http_code}\n' http://skippy:7777/mcp
```

**Expected:** the host resolves, `TcpTestSucceeded` is true, and the request
without authentication returns `401`.

If the hostname does not resolve, use `ipconfig` on SKIPPY to find its active
LAN IPv4 address. Add that actual address followed by `skippy` to the second
PC's `C:\Windows\System32\drivers\etc\hosts`, using an editor run as
administrator. Keep the URL's hostname as `skippy` to match the server's
existing allowed Host value.

For a timeout, check SKIPPY's server and Private-network firewall rule for
TCP 7777. `allowed_hosts` validates the HTTP Host, not the connecting PC's
source address. Keep network access scoped appropriately. The current HTTP
endpoint is for the trusted LAN; use TLS or an encrypted tunnel across an
untrusted network.

## 5. Save the bearer token in Windows Credential Manager

On SKIPPY, open `C:\Users\chris\holtzman\holtzman.toml` locally in an editor.
Transfer only the token value privately, for example through your password
manager. Do not paste it into chat, put it in a command, or transfer the full
config. Enter the token without TOML quotes or the word `Bearer` below.

Create the credential-entry script on the second PC:

```powershell
@'
import getpass
import warnings
from keyring.backends.Windows import WinVaultKeyring

warnings.simplefilter("error", getpass.GetPassWarning)
token = getpass.getpass("Holtzman token (hidden): ").strip()
if not token or token.startswith("Bearer ") or "\n" in token or "\r" in token:
    raise SystemExit("Enter only the token value, on one line.")
vault = WinVaultKeyring()
vault.persist = "local machine"
vault.set_password("HoltzmanVoice", "holtzman-bearer", token)
print("Credential saved in Windows Credential Manager.")
'@ | Set-Content -LiteralPath (Join-Path $HoltRoot 'tests\save-credential.py') -Encoding utf8

& $HoltPython (Join-Path $HoltRoot 'tests\save-credential.py')
```

The hidden prompt avoids shell history and echoed token output. The explicit
[Windows keyring backend](https://github.com/jaraco/keyring/blob/main/keyring/backends/Windows.py)
stores an encrypted credential for your Windows account. Run later checks
under that same account. Clear any temporary transfer clipboard afterward.

## 6. Test authenticated MCP with read-only ship status

```powershell
@'
import asyncio
import logging
import httpx2
from keyring.backends.Windows import WinVaultKeyring
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client


async def main():
    token = WinVaultKeyring().get_password("HoltzmanVoice", "holtzman-bearer")
    if not token:
        raise RuntimeError("Credential not found for this Windows account")
    async with asyncio.timeout(30):
        async with httpx2.AsyncClient(
            headers={"Authorization": f"Bearer {token}"},
            timeout=15, follow_redirects=False, trust_env=False,
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
                        raise RuntimeError("Status tool returned an error")
                    for block in data.get("content", []):
                        if block.get("type") == "text":
                            print(block["text"])


if __name__ == "__main__":
    logging.disable(logging.CRITICAL)
    try:
        asyncio.run(main())
    except Exception as exc:
        print("MCP check failed:", type(exc).__name__)
        print("Check the server, hostname, credential and allowed Host; see troubleshooting.")
        raise SystemExit(1) from None
'@ | Set-Content -LiteralPath (Join-Path $HoltRoot 'tests\check-mcp.py') -Encoding utf8

& $HoltPython (Join-Path $HoltRoot 'tests\check-mcp.py')
```

**Continue when:** it lists tools and returns ship status. This uses the
[MCP SDK v2 transport](https://github.com/modelcontextprotocol/python-sdk/blob/main/examples/snippets/clients/streamable_basic.py).
It only lists tools and calls `status`, sends no game controls, and keeps
the bearer credential out of model prompts. A closed game can leave old
journal data, so check freshness before treating status as live.

## 7. Build the native Windows model runtime

Use the Start menu shortcut **Developer PowerShell for VS 2022** for this
build step, as recommended by the
[llama.cpp Windows build instructions](https://github.com/ggml-org/llama.cpp/blob/v0.7.0/docs/build.md).
Run the four-variable path block from step 3 in that window. If it opens
Windows PowerShell 5.1, these build commands still work there; use PowerShell
7 again for model startup and the later checks.

Clone into a new directory and build the pinned source release:

```powershell
git clone --depth 1 --branch v0.7.0 https://github.com/ggml-org/llama.cpp.git (Join-Path $HoltRoot 'src\llama.cpp')
if ($LASTEXITCODE -ne 0) { throw 'Clone failed; inspect any existing source directory.' }

cmake -S (Join-Path $HoltRoot 'src\llama.cpp') -B (Join-Path $HoltRoot 'src\llama.cpp\build') -G 'Visual Studio 17 2022' -A x64 -DGGML_VULKAN=ON
if ($LASTEXITCODE -ne 0) { throw 'CMake configuration failed.' }

cmake --build (Join-Path $HoltRoot 'src\llama.cpp\build') --config Release --target llama-server --parallel 8
if ($LASTEXITCODE -ne 0) { throw 'Model runtime build failed.' }

& (Join-Path $HoltRoot 'src\llama.cpp\build\bin\Release\llama-server.exe') --list-devices
```

**Continue when:** the build succeeds and the runtime lists the AMD Vulkan
device. Keep the complete build output directory, including its DLLs.
Do not copy just the executable elsewhere. A source directory that already
exists should be inspected and reused deliberately, not deleted or reset.

## 8. Download the model and its matching vision projector

Back in **PowerShell 7**, run the path block from step 3 if necessary:

```powershell
& $HoltHF download unsloth/Qwen3.5-35B-A3B-GGUF Qwen3.5-35B-A3B-Q4_K_M.gguf mmproj-F16.gguf --local-dir (Join-Path $HoltRoot 'models\qwen')
if ($LASTEXITCODE -ne 0) { throw 'Model download failed.' }

Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $HoltRoot 'models\qwen\Qwen3.5-35B-A3B-Q4_K_M.gguf'), (Join-Path $HoltRoot 'models\qwen\mmproj-F16.gguf') | Format-List | Out-File -LiteralPath (Join-Path $HoltRoot 'tests\model-hashes.txt') -Encoding utf8
```

Both files come from the
[same quantization repository](https://huggingface.co/unsloth/Qwen3.5-35B-A3B-GGUF/tree/main).
The language model is about 22 GB and the projector about 0.9 GB. Do not
substitute a projector from an unrelated model. The hashes record the files
installed for later comparison; no public-model login should be necessary.

Create the startup script:

```powershell
@'
$HoltRoot = Join-Path $env:LOCALAPPDATA 'HoltzmanVoice'
$HoltServer = Join-Path $HoltRoot 'src\llama.cpp\build\bin\Release\llama-server.exe'
$HoltModelArgs = @(
    '-m', (Join-Path $HoltRoot 'models\qwen\Qwen3.5-35B-A3B-Q4_K_M.gguf'),
    '--mmproj', (Join-Path $HoltRoot 'models\qwen\mmproj-F16.gguf'),
    '--alias', 'holt',
    '--host', '127.0.0.1', '--port', '8080',
    '-ngl', '99', '-c', '8192', '--parallel', '1',
    '--jinja', '--chat-template-kwargs', '{"enable_thinking":false}'
)
& $HoltServer @HoltModelArgs
exit $LASTEXITCODE
'@ | Set-Content -LiteralPath (Join-Path $HoltRoot 'start-model.ps1') -Encoding utf8

pwsh.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $HoltRoot 'start-model.ps1')
```

The execution-policy option applies to this process, not a machine-wide
policy change. Leave the terminal open. Watch for Vulkan GPU offload and
successful loading of both model and projector. Keep the 8K context for
the baseline. These are initial settings to benchmark, not a latency promise.

In a **second PowerShell 7 window**, run the path block from step 3, then:

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8080/health'

$HoltRequest = @{
    model = 'holt'
    messages = @(@{ role = 'user'; content = 'Reply with: Local model ready.' })
    max_tokens = 64
    temperature = 0
} | ConvertTo-Json -Depth 10

$HoltReply = Invoke-RestMethod -Uri 'http://127.0.0.1:8080/v1/chat/completions' -Method Post -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($HoltRequest)) -TimeoutSec 120
$HoltReply.choices[0].message.content
```

**Continue when:** health reports OK and the model returns a local answer.
This [server API](https://github.com/ggml-org/llama.cpp/blob/v0.7.0/tools/server/README.md)
is bound to loopback. Only the second PC needs to access it.

Check a structured tool proposal without connecting it to game controls:

```powershell
$HoltRequest = @{
    model = 'holt'
    messages = @(@{ role = 'user'; content = 'Call status to learn my current location. Do not guess.' })
    tools = @(@{
        type = 'function'
        function = @{
            name = 'status'
            description = 'Read current Elite Dangerous ship status.'
            parameters = @{ type = 'object'; properties = @{}; additionalProperties = $false }
        }
    })
    tool_choice = 'auto'
    max_tokens = 256
    temperature = 0
} | ConvertTo-Json -Depth 15

$HoltReply = Invoke-RestMethod -Uri 'http://127.0.0.1:8080/v1/chat/completions' -Method Post -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($HoltRequest)) -TimeoutSec 120
$HoltReply.choices[0].message | ConvertTo-Json -Depth 15
```

**Expected:** `tool_calls` naming `status` with empty arguments, rather
than a fabricated location. This does not execute a tool or establish
reliability beyond this simple check.

## 9. Test local screenshot interpretation

Copy an existing, readable PNG screenshot of the game's Contacts panel
from SKIPPY to `tests\cockpit.png` under `$HoltRoot`. Use a crop that clearly
shows the station name and highlight. No game input or new calibration is
needed for this check.

```powershell
@'
import base64
import json
import os
from pathlib import Path
from urllib.request import Request, urlopen

image = Path(os.environ["LOCALAPPDATA"]) / "HoltzmanVoice/tests/cockpit.png"
encoded = base64.b64encode(image.read_bytes()).decode("ascii")
payload = {
    "model": "holt",
    "messages": [{"role": "user", "content": [
        {"type": "text", "text": "Read this Elite Dangerous panel. Which tab is active, which station name is highlighted, and is Request Docking highlighted? Say if anything is unreadable. Do not guess or claim an action occurred."},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64," + encoded}},
    ]}],
    "max_tokens": 200,
    "temperature": 0,
}
request = Request(
    "http://127.0.0.1:8080/v1/chat/completions",
    data=json.dumps(payload).encode("utf-8"),
    headers={"Content-Type": "application/json"},
)
with urlopen(request, timeout=120) as response:
    result = json.load(response)
print(result["choices"][0]["message"]["content"])
'@ | Set-Content -LiteralPath (Join-Path $HoltRoot 'tests\check-vision.py') -Encoding utf8

& $HoltPython (Join-Path $HoltRoot 'tests\check-vision.py')
```

**Continue when:** its description agrees with what is visibly highlighted.
Test several saved states before relying on a model for docking. A wrong or
uncertain result means adjust the crop/model/runtime and retest; a successful
request alone does not mean the panel was interpreted correctly.

This exercises llama.cpp's
[multimodal input](https://github.com/ggml-org/llama.cpp/blob/v0.7.0/docs/multimodal.md).
The screenshot is sent only to the local model endpoint. In the integrated
client, Holtzman captures the game window on SKIPPY, while vision inference
stays on the second PC.

## 10. Test the microphone and local speech recognition

On the second PC, choose the mic and speakers in **Settings -> System ->
Sound**. Under **Settings -> Privacy & security -> Microphone**, enable
microphone access and access for desktop apps.

List the devices:

```powershell
& $HoltPython -m sounddevice
```

Record eight seconds and play it back using
[sounddevice](https://python-sounddevice.readthedocs.io/en/0.5.3/usage.html):

```powershell
@'
import os
from pathlib import Path
import sounddevice as sd
import soundfile as sf

path = Path(os.environ["LOCALAPPDATA"]) / "HoltzmanVoice/tests/mic-test.wav"
input("Press Enter, then say: HOLT, plot a course to AD Leonis from Ross 248. ")
audio = sd.rec(8 * 16000, samplerate=16000, channels=1, dtype="float32")
sd.wait()
sf.write(path, audio, 16000, subtype="PCM_16")
print("Playing back the recording.")
sd.play(audio, 16000)
sd.wait()
'@ | Set-Content -LiteralPath (Join-Path $HoltRoot 'tests\check-audio.py') -Encoding utf8

& $HoltPython (Join-Path $HoltRoot 'tests\check-audio.py')
```

Resolve silence, clipping or a wrong input device before continuing. If
necessary, set `sd.default.device = (INPUT_ID, OUTPUT_ID)` in the test script,
using the actual numbers from the device list, not those placeholder names.

In **Developer PowerShell for VS 2022**, rerun the path block from step 3 and
build a pinned CPU baseline of
[whisper.cpp](https://github.com/ggml-org/whisper.cpp/releases/tag/v1.9.5):

```powershell
git clone --depth 1 --branch v1.9.5 https://github.com/ggml-org/whisper.cpp.git (Join-Path $HoltRoot 'src\whisper.cpp')
if ($LASTEXITCODE -ne 0) { throw 'Clone failed; inspect any existing source directory.' }

cmake -S (Join-Path $HoltRoot 'src\whisper.cpp') -B (Join-Path $HoltRoot 'src\whisper.cpp\build') -G 'Visual Studio 17 2022' -A x64 -DGGML_VULKAN=OFF
if ($LASTEXITCODE -ne 0) { throw 'Whisper CMake configuration failed.' }

cmake --build (Join-Path $HoltRoot 'src\whisper.cpp\build') --config Release --target whisper-cli --parallel 8
if ($LASTEXITCODE -ne 0) { throw 'Whisper build failed.' }

& $HoltHF download ggerganov/whisper.cpp ggml-small.en.bin --local-dir (Join-Path $HoltRoot 'models\whisper')
if ($LASTEXITCODE -ne 0) { throw 'Whisper model download failed.' }

& (Join-Path $HoltRoot 'src\whisper.cpp\build\bin\Release\whisper-cli.exe') -m (Join-Path $HoltRoot 'models\whisper\ggml-small.en.bin') -f (Join-Path $HoltRoot 'tests\mic-test.wav') -l en -t 8 --prompt 'Elite Dangerous. HOLT. AD Leonis. Ross 248. Homer Platform.'
```

**Continue when:** the transcription is useful, including system names.
Repeat with ordinary game audio playing. Name context helps but does not
guarantee correct recognition. This file-based check does not yet configure
continuous listening, push-to-talk or a wake word.

## 11. Test local speech synthesis

Back in **PowerShell 7**, rerun the path block if needed. Keep speech
synthesis on the CPU initially, leaving the GPU for language and vision:

```powershell
uv pip install --python $HoltPython torch --index-url https://download.pytorch.org/whl/cpu
uv pip install --python $HoltPython 'kokoro==0.9.4' soundfile

@'
import os
from pathlib import Path
import numpy as np
import sounddevice as sd
import soundfile as sf
from phonemizer.backend.espeak.wrapper import EspeakWrapper

dll = Path(os.environ["ProgramFiles"]) / "eSpeak NG/libespeak-ng.dll"
if not dll.is_file():
    raise SystemExit("Install the x64 eSpeak NG MSI or correct the DLL path in this script.")
dll_directory = os.add_dll_directory(str(dll.parent))
EspeakWrapper.set_library(str(dll))

from kokoro import KPipeline

pipeline = KPipeline(lang_code="a", repo_id="hexgrad/Kokoro-82M", device="cpu")
chunks = [
    np.asarray(audio)
    for _, _, audio in pipeline("Captain, local voice systems are ready.", voice="af_heart")
]
audio = np.concatenate(chunks)
output = Path(os.environ["LOCALAPPDATA"]) / "HoltzmanVoice/tests/voice-test.wav"
sf.write(output, audio, 24000)
sd.play(audio, 24000)
sd.wait()
print("Local voice test complete.")
'@ | Set-Content -LiteralPath (Join-Path $HoltRoot 'tests\check-voice.py') -Encoding utf8

& $HoltPython (Join-Path $HoltRoot 'tests\check-voice.py')
uv pip freeze --python $HoltPython | Set-Content -LiteralPath (Join-Path $HoltRoot 'tests\python-packages.txt') -Encoding utf8
```

Kokoro's first run downloads model, voice and language assets. Once cached,
inference is local. `af_heart` is a test voice; choose HOLT's final voice by
auditioning alternatives. **Continue when:** speech plays clearly through
the speakers connected to the second PC.

## 12. Copy the existing HOLT personality prompt

Copy this file from SKIPPY using your file share, USB drive or another
private transfer method:

```text
C:\Users\chris\holtzman\holtzman-screenshot\prompts\ship-computer.md
```

Save it on the second PC at:

```powershell
Join-Path $HoltConfig 'ship-computer.md'
```

That prints the intended destination path; copy the file there with your
file manager. Copy only the prompt, not `holtzman.toml`. The current edited
prompt lives in the screenshot development checkout, so cloning main alone
does not retrieve that revision.

The future controller will load the prompt and reconcile its tool catalogue
with the connected server. The model service does not load it automatically
just because the file exists.

## 13. Optional: start the model when you sign in

Only after manual checks pass, stop the manually launched model with
**Ctrl+C** so port 8080 is free. Open **Task Scheduler -> Create Task**:

1. Name it `HOLT local model`.
2. Select your normal Windows account and **Run only when user is logged on**.
   Leave **Run with highest privileges** unchecked.
3. Add an **At log on** trigger for that account.
4. Add a **Start a program** action using the PowerShell executable and
   arguments printed by the commands below.
5. Set the action's **Start in** directory to the printed working directory.
   Adjust the battery/power conditions if they would prevent it starting.
6. Save and use **Run** once, then recheck `http://127.0.0.1:8080/health`.

```powershell
Write-Output ('Program: ' + (Get-Command pwsh.exe).Source)
Write-Output ('Arguments: -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "' + (Join-Path $HoltRoot 'start-model.ps1') + '"')
Write-Output ('Start in: ' + $HoltRoot)
```

The model starts hidden and stays bound to loopback. This task has no
Holtzman credential and sends no controls. Use Task Scheduler to disable or
end it before running a manual copy. The future audio/controller process
should also run as your signed-in user, with access to the same Credential
Manager entry and desktop audio devices.

## 14. What remains to implement

You should now have independent checks for MCP ship status, local text and
tool-call output, screenshot interpretation, microphone transcription and
spoken replies. The HOLT personality prompt is ready in its intended path.

The next development task is the `holtzman-voice` controller with a tested
dependency lock: connect local audio/STT/LLM/TTS using Pipecat, load the
prompt, dispatch validated MCP tool calls and verify results. Start with
read-only status and trade discussions. Add one explicitly requested,
supervised route plot, then the existing screenshot-guided docking flow.
No additional docking calibration is assumed here.

Reliable action cancellation remains a separate Holtzman improvement.
Interrupting speech or canceling a client request does not yet guarantee
that an ongoing route recipe stopped. None of the installation checks above
needs to send game input.

## Troubleshooting

| Symptom | Next check |
| --- | --- |
| Command not found after installation | Reopen the terminal; check PATH and use PowerShell 7 |
| `VULKAN_SDK` is empty | SDK installation and newly opened terminal |
| Vulkan does not list the AMD GPU | OEM/AMD graphics driver and reboot; the SDK is not the GPU driver |
| CMake cannot find compiler or Windows SDK | VS 2022 C++ workload, recommended components and Developer PowerShell |
| CMake cannot find Vulkan or `glslc` | SDK environment, headers and shader-tools installation |
| Model executable reports missing DLL | Keep build output together; check the Visual C++ runtime installation |
| GPU allocation fails | Actual Vulkan memory limit, context size and other GPU workloads; collect the startup log before changing firmware |
| Model image request fails | Matching projector loaded, correct runtime/model pair and a valid PNG |
| Model guesses the highlighted row | Crop/readability/model accuracy; resolve it before using docking controls |
| Hostname or TCP check fails | LAN DNS/hosts, server listener and SKIPPY's Private-network firewall rule |
| MCP check fails | Same Windows user as credential entry; token without quotes/`Bearer`; allowed Host; inspect SKIPPY's log for 401/403 or protocol errors |
| Credential backend fails | Windows environment and `keyring`/`pywin32-ctypes`; do not switch to plaintext storage as a workaround |
| Mic is silent or inaccessible | Windows microphone privacy settings, selected input, mute and device IDs |
| Audio reports invalid sample rate | Try the mic's Windows shared-mode device from the list; the test records 16 kHz mono |
| TTS cannot load eSpeak | x64 MSI, actual `libespeak-ng.dll` path and matching architecture |
| Port 8080 is in use | Duplicate manual model or scheduled task |

Share sanitized errors and hardware/device output for follow-up. Never share
the bearer credential, Credential Manager contents or full server config.

param([int]$Port = 8081)
$ErrorActionPreference = 'Stop'
$HoltRoot = Join-Path $PSScriptRoot '..\.local\holt'
$HoltServer = Join-Path $HoltRoot 'runtime\whisper\Release\whisper-server.exe'
$HoltModel = Join-Path $HoltRoot 'models\whisper\ggml-small.en.bin'
if (-not (Test-Path -LiteralPath $HoltServer)) { throw 'Whisper runtime is missing.' }
if (-not (Test-Path -LiteralPath $HoltModel)) { throw 'English recognition model is missing.' }
& $HoltServer -m $HoltModel --host 127.0.0.1 --port $Port -l en -t 8 -ng
exit $LASTEXITCODE

param([int]$Port = 8081, [ValidateSet('cpu','vulkan')][string]$Backend = 'cpu', [string]$RuntimePath = '', [string]$ModelPath = '')
$ErrorActionPreference = 'Stop'
$HoltRoot = Join-Path $PSScriptRoot '..\.local\holt'
$HoltServer = Join-Path $HoltRoot 'runtime\whisper\Release\whisper-server.exe'
$HoltModel = Join-Path $HoltRoot 'models\whisper\ggml-small.en.bin'
if ($Backend -eq 'vulkan') {
    $HoltServer = Join-Path $HoltRoot 'runtime\whisper-vulkan\whisper-server.exe'
    $HoltModel = Join-Path $HoltRoot 'models\whisper\ggml-large-v3-turbo.bin'
}
if ($RuntimePath) { $HoltServer = $RuntimePath }
if ($ModelPath) { $HoltModel = $ModelPath }
if (-not (Test-Path -LiteralPath $HoltServer)) { throw 'Whisper runtime is missing.' }
if (-not (Test-Path -LiteralPath $HoltModel)) { throw 'English recognition model is missing.' }
$HoltArgs = @('-m', $HoltModel, '--host', '127.0.0.1', '--port', "$Port", '-l', 'en', '-t', '8')
if ($Backend -eq 'cpu') { $HoltArgs += '-ng' }
& $HoltServer @HoltArgs
exit $LASTEXITCODE

param([int]$Port = 8080, [int]$ContextSize = 16384, [switch]$TextOnly)
$ErrorActionPreference = 'Stop'
$HoltRoot = Join-Path $PSScriptRoot '..\.local\holt'
$HoltServer = Join-Path $HoltRoot 'runtime\llama-server.exe'
$HoltModel = Join-Path $HoltRoot 'models\Qwen3.5-35B-A3B-Q4_K_M.gguf'
$HoltProjector = Join-Path $HoltRoot 'models\mmproj-F16.gguf'
if (-not (Test-Path -LiteralPath $HoltServer)) { throw 'HOLT runtime is missing.' }
if (-not (Test-Path -LiteralPath $HoltModel)) { throw 'HOLT model is missing.' }
$HoltArguments = @(
    '-m', $HoltModel, '--alias', 'holt', '--host', '127.0.0.1',
    '--port', "$Port", '-ngl', '99', '-c', "$ContextSize", '--parallel', '1',
    '--jinja', '--reasoning', 'off'
)
if (-not $TextOnly) {
    if (-not (Test-Path -LiteralPath $HoltProjector)) { throw 'Matching vision projector is missing.' }
    $HoltArguments += @('--mmproj', $HoltProjector, '--image-min-tokens', '1024')
}
& $HoltServer @HoltArguments
exit $LASTEXITCODE

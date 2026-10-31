param([ValidateSet('start', 'stop', 'status')][string]$Action = 'start')
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$serverDir = Join-Path $repo '.local/holt/chatterbox-server'
$python = Join-Path $serverDir 'venv/Scripts/python.exe'
$serverScript = Join-Path $serverDir 'server.py'
$recordPath = Join-Path $repo '.local/holt/run/chatterbox.json'
$record = if (Test-Path -LiteralPath $recordPath) { Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json }
$running = $null
if ($record) {
    $candidate = Get-Process -Id $record.processId -ErrorAction SilentlyContinue
    if ($candidate -and $candidate.StartTime.ToUniversalTime().Ticks.ToString() -eq $record.startTicks) {
        $processInfo = Get-CimInstance Win32_Process -Filter "ProcessId=$($candidate.Id)"
        if ($processInfo.ExecutablePath -eq $python -and $processInfo.CommandLine.Contains($serverScript)) {
            $running = $candidate
        }
    }
}
if ($Action -eq 'stop') {
    if ($running) {
        # Windows venv Python may launch a second Python process. Stop only its
        # direct Python child with this exact server script, then the launcher.
        Get-CimInstance Win32_Process -Filter "ParentProcessId=$($running.Id)" |
            Where-Object { $_.Name -eq 'python.exe' -and $_.CommandLine.Contains($serverScript) } |
            ForEach-Object { Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }
        Stop-Process -Id $running.Id -ErrorAction SilentlyContinue
        Write-Output 'Chatterbox stopped.'
    } else { Write-Output 'No managed Chatterbox process is running.' }
    exit 0
}
if ($Action -eq 'start' -and -not $running) {
    if (-not (Test-Path -LiteralPath $python)) { throw 'Chatterbox is not installed. See docs/chatterbox-sillytavern.md.' }
    # Avoid starting another copy against an already occupied port.
    $occupied = Get-NetTCPConnection -LocalPort 8004 -State Listen -ErrorAction SilentlyContinue
    if ($occupied) { throw 'Port 8004 is already in use. Check the existing service before starting another copy.' }
    $env:HF_HOME = Join-Path $serverDir 'hf-cache'
    $env:OMP_NUM_THREADS = '8'
    $env:MKL_NUM_THREADS = '8'
    New-Item -ItemType Directory -Force (Split-Path $recordPath -Parent) | Out-Null
    $resultsDir = Join-Path $repo '.local/holt/results'
    New-Item -ItemType Directory -Force $resultsDir | Out-Null
    $running = Start-Process -FilePath $python -ArgumentList ('"' + $serverScript + '"') -WorkingDirectory $serverDir -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $resultsDir 'chatterbox-stdout.log') -RedirectStandardError (Join-Path $resultsDir 'chatterbox-stderr.log')
    @{processId=$running.Id; startTicks=$running.StartTime.ToUniversalTime().Ticks.ToString()} | ConvertTo-Json | Set-Content -LiteralPath $recordPath
    Write-Output 'Chatterbox starting. Initial model loading can take a little time.'
}
if ($running) {
    Write-Output "Chatterbox process $($running.Id): http://127.0.0.1:8004"
    try {
        $data = Invoke-RestMethod 'http://127.0.0.1:8004/api/ui/initial-data' -TimeoutSec 3
        if ($data.model_info.loaded) {
            Write-Output "Model loaded on $($data.model_info.device). Use the SillyTavern voice preview to verify playback."
        } else { Write-Output 'API responding, but speech model failed to load. Check chatterbox-stderr.log.' }
    } catch { Write-Output 'API not responding: it may be starting or busy synthesizing. See .local/holt/results/chatterbox-stderr.log.' }
} else { Write-Output 'Chatterbox is stopped.' }

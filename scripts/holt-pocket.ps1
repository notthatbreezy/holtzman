param(
    [ValidateSet('start', 'stop', 'status', 'menu')][string]$Action = 'menu',
    [switch]$IfSelected
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$python = Join-Path $repo '.local/holt/pocket-env/Scripts/python.exe'
$recordPath = Join-Path $repo '.local/holt/run/pocket.json'
if ($IfSelected) {
    $settingsPath = Join-Path $env:USERPROFILE 'SillyTavern/data/default-user/settings.json'
    if (-not (Test-Path -LiteralPath $settingsPath)) { exit 0 }
    $settings = Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
    if ($settings.extension_settings.tts.currentProvider -ne 'HOLT Pocket') { exit 0 }
}
if ($Action -eq 'menu') {
    Write-Host 'HOLT Pocket TTS'
    Write-Host '1. Start speech service'
    Write-Host '2. Stop speech service'
    Write-Host '3. Status'
    $choice = Read-Host 'Choose 1, 2 or 3 (Enter to close)'
    $Action = switch ($choice) { '1' { 'start' } '2' { 'stop' } '3' { 'status' } default { exit 0 } }
}
$record = if (Test-Path -LiteralPath $recordPath) { Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json }
$running = $null
if ($record) {
    $candidate = Get-Process -Id $record.processId -ErrorAction SilentlyContinue
    if ($candidate -and $candidate.StartTime.ToUniversalTime().Ticks.ToString() -eq $record.startTicks) {
        $info = Get-CimInstance Win32_Process -Filter "ProcessId=$($candidate.Id)"
        if ($info.ExecutablePath -eq $python -and $info.CommandLine -match '-m client\.pocket_service(?: |$)') {
            $running = $candidate
        }
    }
}
if ($Action -eq 'stop') {
    if ($running) {
        $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($running.Id)" |
            Where-Object { $_.Name -eq 'python.exe' -and $_.CommandLine -match '-m client\.pocket_service(?: |$)' })
        $stopping = @($running.Id) + @($children | ForEach-Object { [int]$_.ProcessId })
        $children | ForEach-Object { Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }
        Stop-Process -Id $running.Id -ErrorAction SilentlyContinue
        Wait-Process -Id $stopping -Timeout 10 -ErrorAction SilentlyContinue
        Write-Output 'Pocket speech stopped.'
    } else { Write-Output 'Pocket speech is stopped.' }
    exit 0
}
if ($Action -eq 'start' -and -not $running) {
    if (-not (Test-Path -LiteralPath $python)) { throw 'Pocket is not installed. See docs/pocket-tts-sillytavern.md.' }
    if (Get-NetTCPConnection -LocalPort 8005 -State Listen -ErrorAction SilentlyContinue) {
        throw 'Port 8005 is already occupied; no additional service was started.'
    }
    New-Item -ItemType Directory -Force (Split-Path $recordPath -Parent) | Out-Null
    $logs = Join-Path $repo '.local/holt/results'
    New-Item -ItemType Directory -Force $logs | Out-Null
    $running = Start-Process -FilePath $python -ArgumentList '-m client.pocket_service' -WorkingDirectory $repo `
        -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $logs 'pocket-stdout.log') `
        -RedirectStandardError (Join-Path $logs 'pocket-stderr.log')
    @{processId=$running.Id; startTicks=$running.StartTime.ToUniversalTime().Ticks.ToString()} |
        ConvertTo-Json | Set-Content -LiteralPath $recordPath
    $deadline = (Get-Date).AddSeconds(30)
    do {
        try {
            $health = Invoke-RestMethod 'http://127.0.0.1:8005/health' -TimeoutSec 1
            if ($health.service -eq 'holt-pocket-tts' -and $health.ready) { break }
        } catch { }
        Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline -and -not $running.HasExited)
}
if (-not $running) { Write-Output 'Pocket speech is stopped.'; exit 0 }
try {
    $health = Invoke-RestMethod 'http://127.0.0.1:8005/health' -TimeoutSec 3
    if ($health.service -ne 'holt-pocket-tts') { throw 'Port 8005 belongs to another service' }
    if (-not $health.ready) { throw 'Pocket failed to load. See .local/holt/results/pocket-stderr.log.' }
    Write-Output "Pocket ready: CPU, $($health.threads) threads, K-2SO voices, http://127.0.0.1:8005"
    if ($health.busy) { Write-Output 'Currently generating speech.' }
} catch { throw 'Pocket is not ready. See .local/holt/results/pocket-stderr.log.' }

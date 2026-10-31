# Build holtzman from GitHub (default: main) and restart the MCP server with it.
# The running server keeps serving until the new build succeeds. The previous
# binary is kept as holtzman.exe.bak and restored if the new one won't start.
param(
    [string]$Branch = 'main',
    [string]$Config = (Join-Path $HOME 'holtzman\holtzman.toml'),
    [string]$Repo = 'https://github.com/notthatbreezy/holtzman'
)
$ErrorActionPreference = 'Stop'
$Config = (Resolve-Path -LiteralPath $Config).Path
$ConfigDir = Split-Path -Parent $Config
$Installed = Join-Path $HOME '.cargo\bin\holtzman.exe'
$Backup = "$Installed.bak"
$Staging = Join-Path $env:LOCALAPPDATA 'holtzman-update'
$StdOut = Join-Path $ConfigDir 'server.stdout.log'
$StdErr = Join-Path $ConfigDir 'server.stderr.log'

$Listen = (Select-String -LiteralPath $Config -Pattern '^\s*listen\s*=\s*"([^"]+)"').Matches
if (-not $Listen) { throw "No listen address in $Config." }
$Listen = $Listen[0].Groups[1].Value
$Port = [int]($Listen -split ':')[-1]

function Get-Listener {
    Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
}

function Wait-Port([bool]$Open, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if ([bool](Get-Listener) -eq $Open) { return $true }
        Start-Sleep -Milliseconds 250
    }
    return $false
}

# Start the server fully detached (no inherited handles), so this script can
# exit even when its own output is piped, e.g. when an agent runs it.
function Start-Server {
    $line = "`"$Installed`" -c `"$Config`" serve > `"$StdOut`" 2> `"$StdErr`""
    $hidden = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ ShowWindow = [uint16]0 }
    $result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
        CommandLine = "cmd.exe /d /c `"$line`""
        CurrentDirectory = $ConfigDir
        ProcessStartupInformation = $hidden
    }
    if ($result.ReturnValue -ne 0) { throw "Couldn't start the server (error $($result.ReturnValue))." }
}

function Stop-NewServer {
    Get-CimInstance Win32_Process -Filter "Name='holtzman.exe'" |
        Where-Object { $_.CommandLine -and $_.CommandLine.Contains($Config) -and $_.CommandLine -match '\bserve\b' } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

# Pin the exact commit so what we report is what we built.
$Commit = (git ls-remote $Repo "refs/heads/$Branch") -split "\s+" | Select-Object -First 1
if ($LASTEXITCODE -ne 0 -or -not $Commit) { throw "Can't find branch $Branch at $Repo." }
$Short = $Commit.Substring(0, 7)
Write-Host "Building holtzman $Branch @ $Short..."
# The staging target dir is reused, and Cargo treats a git package's build
# script as fresh across revisions. Drop holtzman's own build-script state
# (dependencies stay cached) and pass the commit so the label is never stale.
$Release = Join-Path $Staging 'target\release'
Get-ChildItem -LiteralPath (Join-Path $Release 'build'), (Join-Path $Release '.fingerprint') `
    -Directory -Filter 'holtzman-*' -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
$env:HOLTZMAN_GIT_COMMIT = $Commit
& cargo install --git $Repo --rev $Commit --locked --force `
    --root $Staging --target-dir (Join-Path $Staging 'target') holtzman
if ($LASTEXITCODE -ne 0) { throw 'Build failed; the running server was left alone.' }
$Built = Join-Path $Staging 'bin\holtzman.exe'

# Stop whatever holtzman is serving on our port, and nothing else.
$Listener = Get-Listener
if ($Listener) {
    $Owner = Get-CimInstance Win32_Process -Filter "ProcessId=$($Listener.OwningProcess)"
    if ($Owner.Name -ne 'holtzman.exe') {
        throw "Port $Port is held by $($Owner.Name) (PID $($Owner.ProcessId)), not holtzman."
    }
    Write-Host "Stopping server PID $($Owner.ProcessId)"
    Stop-Process -Id $Owner.ProcessId -Force
    if (-not (Wait-Port $false 10)) { throw "Port $Port is still in use." }
}

if (Test-Path -LiteralPath $Installed) { Copy-Item -LiteralPath $Installed $Backup -Force }
Copy-Item -LiteralPath $Built $Installed -Force

Start-Server
if (Wait-Port $true 15) {
    Write-Host "holtzman $Branch @ $Short serving on $Listen (PID $((Get-Listener).OwningProcess))"
    exit 0
}

Write-Warning 'The new build did not start. Last lines of its logs:'
Get-Content -LiteralPath $StdOut, $StdErr -Tail 10 -ErrorAction SilentlyContinue
Stop-NewServer
if (-not (Test-Path -LiteralPath $Backup)) { throw 'No previous build to restore.' }
Copy-Item -LiteralPath $Backup $Installed -Force
Start-Server
if (Wait-Port $true 15) {
    Write-Warning "Restored the previous build (PID $((Get-Listener).OwningProcess)); $Short was not installed."
    exit 1
}
throw "Neither build started; see $StdOut and $StdErr."

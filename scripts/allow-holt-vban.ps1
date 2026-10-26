# Run elevated on the HOLT processing PC, not SKIPPY. No network-profile changes.
param([string]$Peer = '192.168.50.41')
$ErrorActionPreference = 'Stop'
$PeerIp = [System.Net.IPAddress]::Parse($Peer)
if ($PeerIp.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork) {
    throw 'An IPv4 peer address is required.'
}
$BridgeExe = 'C:\Program Files (x86)\VB\Voicemeeter\voicemeeterpro.exe'
if (-not (Test-Path -LiteralPath $BridgeExe)) { throw 'Install Banana first.' }
$RuleName = 'HOLT-VBAN-SKIPPY'
$RepoRoot = Split-Path -Parent $PSScriptRoot
$BridgeDirectory = Join-Path $RepoRoot '.local\holt\audio-bridge'
# Windows may create broad app-allow rules on first launch. Retain a rollback record
# and disable those for this freshly installed dedicated mixer; our peer rule replaces them.
$BroadRules = @(Get-NetFirewallApplicationFilter -Program $BridgeExe -ErrorAction SilentlyContinue |
    Get-NetFirewallRule | Where-Object {
        $_.Name -ne $RuleName -and $_.Direction -eq 'Inbound' -and
        $_.Action -eq 'Allow' -and $_.Enabled -eq 'True'
    })
if ($BroadRules.Count -gt 0) {
    $BroadBackup = Join-Path $BridgeDirectory 'disabled-broad-firewall-rules.json'
    if (-not (Test-Path -LiteralPath $BroadBackup)) {
        $BroadRules | Select-Object Name,DisplayName,Enabled,Profile | ConvertTo-Json | Set-Content -LiteralPath $BroadBackup
    }
    $BroadRules | Disable-NetFirewallRule | Out-Null
}
$Existing = Get-NetFirewallRule -Name $RuleName -ErrorAction SilentlyContinue
if ($Existing) { Remove-NetFirewallRule -Name $RuleName }
New-NetFirewallRule -Name $RuleName -DisplayName 'HOLT VBAN microphone from SKIPPY only' `
    -Direction Inbound -Action Allow -Enabled True -Protocol UDP -LocalPort 6980 `
    -RemoteAddress $PeerIp.IPAddressToString -Program $BridgeExe -Profile Any `
    -EdgeTraversalPolicy Block | Out-Null
$Output = Join-Path $RepoRoot '.local\holt\audio-bridge\firewall-status.json'
@{ rule = $RuleName; remote_address = $PeerIp.IPAddressToString; protocol = 'UDP';
   local_port = 6980; program = $BridgeExe; profile = 'Any';
   time_utc = [DateTime]::UtcNow.ToString('o') } | ConvertTo-Json | Set-Content -LiteralPath $Output

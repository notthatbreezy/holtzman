$ErrorActionPreference = 'Stop'
$HoltPython = Join-Path $PSScriptRoot '..\.local\holt\voice-env\Scripts\python.exe'
Write-Host 'Copy only the token value from SKIPPY into the hidden prompt below.'
Write-Host 'Do not paste the token into chat. Input will not be displayed.'
& $HoltPython (Join-Path $PSScriptRoot 'check-holt-mcp.py') --save-credential
if ($LASTEXITCODE -ne 0) { throw 'Credential was not saved.' }
Write-Host 'Credential saved. You can close this window and return to the chat.'

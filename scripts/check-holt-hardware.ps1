$ErrorActionPreference = 'Stop'
$HoltRoot = Join-Path $PSScriptRoot '..\.local\holt'
$HoltResults = Join-Path $HoltRoot 'results'
New-Item -ItemType Directory -Force $HoltResults | Out-Null
$HoltComputer = Get-CimInstance Win32_ComputerSystem
$HoltMemory = Get-CimInstance Win32_PhysicalMemory
$HoltCpu = Get-CimInstance Win32_Processor
$HoltBios = Get-CimInstance Win32_BIOS
$HoltOs = Get-CimInstance Win32_OperatingSystem
$HoltGpu = Get-CimInstance Win32_VideoController
$HoltUpdate = New-Object -ComObject Microsoft.Update.SystemInfo
$HoltReport = [ordered]@{
    timestamp_utc = [DateTime]::UtcNow.ToString('o')
    model = $HoltComputer.Model
    installed_memory_bytes = ($HoltMemory | Measure-Object Capacity -Sum).Sum
    system_memory_bytes = $HoltComputer.TotalPhysicalMemory
    cpu = $HoltCpu.Name
    cores = $HoltCpu.NumberOfCores
    logical_processors = $HoltCpu.NumberOfLogicalProcessors
    bios = $HoltBios.SMBIOSBIOSVersion
    windows = $HoltOs.Caption
    windows_build = $HoltOs.BuildNumber
    graphics = @($HoltGpu | ForEach-Object {
        [ordered]@{name=$_.Name; driver_version=$_.DriverVersion; driver_date=$_.DriverDate}
    })
    windows_update_reboot_required = $HoltUpdate.RebootRequired
}
$HoltReport | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $HoltResults 'hardware.json')
$HoltVulkan = Get-Command vulkaninfo.exe -ErrorAction SilentlyContinue
if ($HoltVulkan) {
    & $HoltVulkan.Source --summary 2>&1 | Out-File (Join-Path $HoltResults 'vulkan-summary.txt')
    & $HoltVulkan.Source 2>&1 | Out-File (Join-Path $HoltResults 'vulkan-full.txt')
}
$HoltServer = Join-Path $HoltRoot 'runtime\llama-server.exe'
if (Test-Path -LiteralPath $HoltServer) {
    & $HoltServer --list-devices 2>&1 | Tee-Object -FilePath (Join-Path $HoltResults 'llama-devices.txt')
}
$HoltReport | ConvertTo-Json -Depth 6

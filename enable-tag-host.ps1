param([switch]$Disable)
$ErrorActionPreference = 'Stop'
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$name = 'MicNoiseReducer.TagHost'
if ($Disable) {
    Remove-ItemProperty -LiteralPath $key -Name $name -ErrorAction SilentlyContinue
    Write-Host 'TAG host autostart disabled. The current host keeps running until sign-out.'
    return
}
$exe = Join-Path $PSScriptRoot 'bin\mic_tag_host.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Run .\build.ps1 first.' }
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -LiteralPath $key -Name $name -Value ('"' + $exe + '"')
Start-Process -FilePath $exe -WorkingDirectory $PSScriptRoot -WindowStyle Hidden
Write-Host 'TAG host enabled at Windows sign-in. The UI and NVIDIA processing can remain off.'

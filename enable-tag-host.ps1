param([switch]$Disable, [switch]$Remove)
$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'bin\mic_tag_host.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Build or install the current Mic Noize host first.' }
if ($Disable -and $Remove) { throw 'Choose -Disable or -Remove.' }
$mode = if ($Remove) { '--task-remove' } elseif ($Disable) { '--task-disable' } else { '--task-enable' }
$process = Start-Process -FilePath $exe -ArgumentList $mode -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw 'Host task configuration failed; see results/tag-host.log.' }
if (!$Disable -and !$Remove) {
    $process = Start-Process -FilePath $exe -ArgumentList '--task-start' -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Host task start failed; see results/tag-host.log.' }
    Write-Host 'TAG host scheduled for the current interactive user. Login enabled; crash recovery: 3 attempts, 1 minute apart.'
} else {
    Write-Host 'TAG host login disabled. The current host keeps running until stopped or sign-out.'
}

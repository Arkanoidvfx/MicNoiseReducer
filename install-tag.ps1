param([switch]$Elevated)
$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'vendor\tag-2.0.0.1903-demo'
$driver = Join-Path $root 'driver\x64\ThinAudioGateway_4d699d4a.sys'
$catalog = Join-Path $root 'driver\ThinAudioGateway_4d699d4a.cat'
$manager = Join-Path $root 'wdmdrvmgr\x64\wdmdrvmgr.exe'
$inf = Join-Path $root 'driver\ThinAudioGateway_4d699d4a.inf'
$signTool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
foreach ($file in @($driver,$catalog,$manager,$inf,$signTool)) {
    if (-not (Test-Path -LiteralPath $file)) { throw "Missing file: $file" }
}
foreach ($file in @($driver,$manager)) {
    if ((Get-AuthenticodeSignature -LiteralPath $file).Status -ne 'Valid') { throw "Invalid signature: $file" }
}
& $signTool verify /kp /c $catalog $driver
if ($LASTEXITCODE -ne 0) { throw 'Kernel driver signature check failed.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    if ($Elevated) { throw 'Administrator rights were not granted.' }
    $process = Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"'),'-Elevated') -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "TAG installation failed: $($process.ExitCode)" }
    exit
}
$log = Join-Path $PSScriptRoot 'results\tag-install.log'
& $manager -q -h 'Root\ThinAudioGateway_4d699d4a\0000' -i 'ThinAudioGateway_4d699d4a-65a5-40ec-9875-8e6d5fc01e0c' $inf *> $log
if ($LASTEXITCODE -ne 0) { throw "TAG installer failed ($LASTEXITCODE). See $log" }
Write-Output 'TAG driver installed. Windows security settings were not changed.'

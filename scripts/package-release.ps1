[CmdletBinding()]
param([Parameter(Mandatory)][string]$Version, [switch]$Stage)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$publish = Join-Path $root 'publish'
$releases = Join-Path $root 'Releases'
if ($Stage) { & (Join-Path $PSScriptRoot 'stage-release.ps1') -Version $Version }
if (-not (Test-Path (Join-Path $publish 'MicNoize.exe'))) { throw 'Release is not staged.' }
New-Item -ItemType Directory -Force $releases | Out-Null
dotnet tool restore
dotnet tool run vpk -- pack `
    --packId MicNoize `
    --packVersion $Version `
    --packDir $publish `
    --mainExe MicNoize.exe `
    --packTitle 'Mic Noize' `
    --packAuthors 'Arkanoid VFX' `
    --channel win-x64-stable `
    --outputDir $releases `
    --releaseNotes (Join-Path $root 'release\notes.md') `
    --noPortable
if ($LASTEXITCODE -ne 0) { throw 'Velopack packaging failed.' }
$setup = Get-ChildItem $releases -File -Filter '*-Setup.exe' |
    Where-Object Name -ne 'Setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($setup) {
    Copy-Item $setup.FullName (Join-Path $releases "Mic-Noize-Setup-$Version.exe") -Force
    Copy-Item $setup.FullName (Join-Path $releases 'Setup.exe') -Force
}
Get-ChildItem $releases -File | Where-Object Name -ne 'checksums.sha256' | Get-FileHash -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash)  $([IO.Path]::GetFileName($_.Path))" } |
    Set-Content (Join-Path $releases 'checksums.sha256') -Encoding ascii

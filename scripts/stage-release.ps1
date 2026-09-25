[CmdletBinding()]
param([Parameter(Mandatory)][string]$Version)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cmake = 'C:\Program Files\CMake\bin\cmake.exe'
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$stage = Join-Path $root 'publish'
$hostStage = Join-Path $root 'build\host-release-stage'
$env:CARGO_TARGET_DIR = Join-Path $root 'build\rust'
$env:CARGO_HOME = Join-Path $root '.cache\cargo'
$env:TEMP = Join-Path $root '.tmp'
$env:TMP = $env:TEMP

if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Version must be semver.' }
if ((Get-Content (Join-Path $root 'release\version.txt') -Raw).Trim() -ne $Version) {
    throw 'release/version.txt does not match requested version.'
}
$cargoVersion = [regex]::Match((Get-Content (Join-Path $root 'ui\Cargo.toml') -Raw),'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($cargoVersion -ne $Version) { throw 'ui/Cargo.toml does not match requested version.' }
if ([IO.Path]::GetFullPath($stage) -ne [IO.Path]::GetFullPath((Join-Path $root 'publish'))) { throw 'Unsafe staging path' }
New-Item -ItemType Directory -Force $env:CARGO_HOME,$env:TEMP,$stage,$hostStage | Out-Null
Remove-Item -LiteralPath $stage -Recurse -Force
New-Item -ItemType Directory -Force $stage | Out-Null

& $cmake --build (Join-Path $root 'build\native') --config Release --target mic_engine
if ($LASTEXITCODE -ne 0) { throw 'Native engine build failed.' }
& $cmake --build (Join-Path $root 'build\native') --config Release --target mic_tag_host -- "/p:OutDir=$hostStage\"
if ($LASTEXITCODE -ne 0) { throw 'Bundled host build failed.' }
& $cargo build --release --locked --manifest-path (Join-Path $root 'ui\Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Rust UI build failed.' }

Copy-Item (Join-Path $env:CARGO_TARGET_DIR 'release\micnoize.exe') (Join-Path $stage 'MicNoize.exe')
Copy-Item (Join-Path $hostStage 'mic_tag_host.exe') $stage
$tagVersion = Get-Content (Join-Path $root 'src\tag_version.h') -Raw
$protocol = [uint32]([regex]::Match($tagVersion,'MNR_TAG_PROTOCOL\s+(\d+)L').Groups[1].Value)
$hostBuild = [uint32]([regex]::Match($tagVersion,'MNR_TAG_BUILD\s+(\d+)L').Groups[1].Value)
if (!$protocol -or !$hostBuild) { throw 'Host protocol/build metadata missing.' }
$bundle = @{version=$Version;protocol=$protocol;host_build=$hostBuild;ui_sha256=(Get-FileHash (Join-Path $stage 'MicNoize.exe')).Hash;host_sha256=(Get-FileHash (Join-Path $stage 'mic_tag_host.exe')).Hash} | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $stage 'micnoize-bundle.json'),$bundle)
Copy-Item (Join-Path $root 'LICENSE') $stage
Copy-Item (Join-Path $root 'release\notes.md') $stage
Get-ChildItem $stage -File | Get-FileHash -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash)  $([IO.Path]::GetFileName($_.Path))" } |
    Set-Content (Join-Path $stage 'checksums.sha256') -Encoding ascii

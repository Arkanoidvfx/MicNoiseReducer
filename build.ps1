param([switch]$Samples, [switch]$NativeOnly)
$ErrorActionPreference = 'Stop'
Get-PSDrive -PSProvider FileSystem | Select-Object Name,@{Name='FreeGB';Expression={[math]::Round($_.Free/1GB,1)}}
if (-not $Samples) { & (Join-Path $PSScriptRoot 'prepare-deps.ps1') }
$sdk = Join-Path $PSScriptRoot 'vendor\nvidia-afx-3.0.0'
$build = Join-Path $PSScriptRoot 'build\native'
$source = $PSScriptRoot
if ($Samples) { $source = Join-Path $sdk 'samples'; $build = Join-Path $PSScriptRoot 'build\nvidia-samples' }
$cmake = (Get-Command cmake -ErrorAction SilentlyContinue).Source
if (-not $cmake) { $cmake = 'C:\Program Files\CMake\bin\cmake.exe' }
if (-not (Test-Path -LiteralPath $cmake)) { throw 'Install CMake and Visual Studio 2022 C++ Build Tools.' }
$previousSdk = $env:AFX_SDK_ROOT
try {
    $env:AFX_SDK_ROOT = $sdk
    & $cmake -S $source -B $build -G 'Visual Studio 17 2022' -A x64
    if ($LASTEXITCODE -ne 0) { throw 'CMake configure failed.' }
    if ($Samples) { & $cmake --build $build --config Release }
    else { & $cmake --build $build --config Release --target mic_check mic_tag mic_tag_host effects_check bridge_check }
    if ($LASTEXITCODE -ne 0) { throw 'C++ build failed.' }
} finally {
    $env:AFX_SDK_ROOT = $previousSdk
}
if (-not $Samples -and -not $NativeOnly) {
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (-not (Test-Path -LiteralPath $cargo)) { throw 'Install Rust stable x86_64-pc-windows-msvc with rustup.' }
    $previousTarget = $env:CARGO_TARGET_DIR
    $previousCache = $env:CARGO_HOME
    $previousTemp = $env:TEMP
    $previousTmp = $env:TMP
    try {
        $env:CARGO_TARGET_DIR = Join-Path $PSScriptRoot 'build\rust'
        $env:CARGO_HOME = Join-Path $PSScriptRoot '.cache\cargo'
        $env:TEMP = Join-Path $PSScriptRoot '.tmp'; $env:TMP = $env:TEMP
        New-Item -ItemType Directory -Force $env:CARGO_HOME,$env:TEMP | Out-Null
        & $cargo build --release --locked --manifest-path (Join-Path $PSScriptRoot 'ui\Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
        $destination = Join-Path $PSScriptRoot 'bin\MicNoize.exe'
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release\micnoize.exe') -Destination $destination
        Write-Host "Built: $destination"
    } finally {
        $env:CARGO_TARGET_DIR=$previousTarget; $env:CARGO_HOME=$previousCache; $env:TEMP=$previousTemp; $env:TMP=$previousTmp
    }
}

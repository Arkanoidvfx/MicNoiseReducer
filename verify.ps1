$ErrorActionPreference = 'Stop'
$cmake = 'C:\Program Files\CMake\bin\cmake.exe'
if (-not (Test-Path -LiteralPath $cmake)) { $cmake = (Get-Command cmake).Source }
$ctest = Join-Path (Split-Path -Parent $cmake) 'ctest.exe'
if (-not (Test-Path -LiteralPath $ctest)) { $ctest = (Get-Command ctest).Source }
$previousTarget=$env:CARGO_TARGET_DIR; $previousCache=$env:CARGO_HOME; $previousTemp=$env:TEMP; $previousTmp=$env:TMP
try {
    $env:TEMP = Join-Path $PSScriptRoot '.tmp'; $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Force $env:TEMP | Out-Null
    # Check executables must match the engine they test. The host and UI targets are
    # deliberately excluded: a running mic_tag_host must never be replaced by verification.
    & $cmake --build (Join-Path $PSScriptRoot 'build\native') --config Release --target mic_engine mic_check effects_check bridge_check
    if ($LASTEXITCODE -ne 0) { throw 'Check build failed.' }
    & $ctest --test-dir (Join-Path $PSScriptRoot 'build\native') -C Release --output-on-failure
    if ($LASTEXITCODE -ne 0) { throw 'Native checks failed.' }
    $env:CARGO_TARGET_DIR=Join-Path $PSScriptRoot 'build\rust'
    $env:CARGO_HOME=Join-Path $PSScriptRoot '.cache\cargo'
    $cargo=Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    & $cargo test --release --locked --manifest-path (Join-Path $PSScriptRoot 'ui\Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    & $cargo clippy --release --locked --all-targets --manifest-path (Join-Path $PSScriptRoot 'ui\Cargo.toml') -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust lint failed.' }
} finally { $env:CARGO_TARGET_DIR=$previousTarget; $env:CARGO_HOME=$previousCache; $env:TEMP=$previousTemp; $env:TMP=$previousTmp }

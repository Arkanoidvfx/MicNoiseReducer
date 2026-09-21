# Build, check and install

Read only the section needed. Commands assume the existing configured checkout and a fresh PowerShell process. Native checks and UI snapshots do not need NVIDIA inference. Real microphone/NVIDIA runs require the global VRAM preflight; `run.ps1` performs its own check.

## Incremental build

Use absolute paths so shell cwd cannot redirect the work:

```powershell
$project = 'D:\Projects\Audio\MicNoiseReducer'
$cmake = 'C:\Program Files\CMake\bin\cmake.exe'
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$env:CARGO_TARGET_DIR = Join-Path $project 'build\rust'
$env:CARGO_HOME = Join-Path $project '.cache\cargo'
$env:TEMP = Join-Path $project '.tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:CARGO_HOME,$env:TEMP | Out-Null
```

For C++ changes, choose only required targets. `effects_check` and `bridge_check` also rebuild their `mic_engine` dependency. UI-only edits can skip this when the native library is current.

```powershell
& $cmake --build "$project\build\native" --config Release --target mic_engine
if ($LASTEXITCODE -ne 0) { throw 'Native build failed' }
& $cargo build --release --locked --manifest-path "$project\ui\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
```

Rust links `build/native/Release/mic_engine.lib` and `rubberband.lib`; `ui/build.rs` watches them. Cargo output is `build/rust/release/mic-ui.exe`; the installed UI is `bin/MicNoiseReducer-rust.exe`. A successful Cargo build alone does **not** update the installed app.

`build.ps1` is the full dependency/configure/build/install path. It includes `mic_tag_host` and copies the UI executable: either may be locked by a running process. Do not use it as the default incremental command or kill the host to make it pass. For a fresh checkout use the README prerequisites/full build instructions. Keep the frozen `bin/MicNoiseReducer-legacy.exe` intact.

## Focused checks

| Changed behavior | Build / run |
|---|---|
| DSP / routed source / gain / phrase | build target `effects_check`; run `bin/effects_check.exe` |
| C ABI / binding validation / engine control | build target `bridge_check`; run `bin/bridge_check.exe` |
| Ring / drift logic | build target `mic_check`; run `bin/mic_check.exe --self-test` |
| UI handlers / keyboard / values | Cargo `test --release --locked --manifest-path "$project\ui\Cargo.toml" controller_tests` |
| INI / Rust logic more broadly | same Cargo test command without the filter |
| Broad release validation when justified | `verify.ps1`: rebuilds `mic_engine`, `mic_check`, `effects_check`, `bridge_check` (never the host or UI), then CTest, Rust tests, Clippy |

Check `$LASTEXITCODE` immediately after every native command; throw on failure. Never hide an unsuccessful build behind a successful `Get-Content` or other last command. Save substantial output in `results/<task>-check.log` and inspect failures with tight context instead of loading all logs.

Controller tests use in-memory settings and skip the native shell, device discovery and model scanning; they may run alongside the installed UI and TAG host. Close only the verified installed UI before executable snapshots that acquire the single-instance mutex. Keep TAG host running. Existing logs in `results` describe historical runs, not a fresh validation. Do not encode test counts; discover current tests when necessary.

## Install and reopen the UI

After build and selected checks pass, stop the matching installed UI and wait for its exit before copying. This resets processing; the relaunched UI starts the saved route itself after device discovery (there is no Start button). Do not restart for documentation-only changes.

```powershell
$installed = Join-Path $project 'bin\MicNoiseReducer-rust.exe'
$built = Join-Path $project 'build\rust\release\mic-ui.exe'
if (-not (Test-Path -LiteralPath $built)) { throw 'Built UI missing' }
Get-Process -Name 'MicNoiseReducer-rust' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $installed } |
    ForEach-Object { Stop-Process -Id $_.Id; $_.WaitForExit() }
Copy-Item -LiteralPath $built -Destination $installed -ErrorAction Stop
Start-Process -FilePath $installed -WorkingDirectory $project
```

Before stopping, preserve any pending user edits; prefer the application's Exit when appropriate. Never terminate processes by a broad audio/Discord/NVIDIA name match. Never touch `mic_tag_host` during this sequence. A visible window is intentional here because this is the user's interactive UI. The current controller automatically starts a saved valid audio route after device discovery; check VRAM before launching.

For an explicitly background update, launch with `--start-tray` and `-WindowStyle Hidden`: no Iced window is created until the user opens the tray. RVC stops its owned sidecar process tree immediately on toggle-off, with a bounded wait and visible error on failure; it also stops it on UI exit. The native idle worker waits for an event rather than polling. `bridge_check` covers disabled RVC bypass and idle-worker shutdown without a model. Rebuild the Python 3.10/PyInstaller sidecar when `src/vcclient_server.py` or `src/rvc_import.py` changes; the installed wrapper is `vendor/vcclient-2.1.4-alpha/dist/main/mnr_vcclient_server.exe`.

## Visual and hardware checks

Own-app render hook, with normal UI closed:

```powershell
$capture = Start-Process -FilePath $installed -WorkingDirectory $project -PassThru -ArgumentList @('--ui-benchmark', '--ui-snapshot', ('"' + "$project\results\ui.png" + '"'))
$capture.WaitForExit()
if ($capture.ExitCode -ne 0) { throw 'UI snapshot failed' }
```

Inspect the PNG with an image tool before calling visual QA complete. Optional flags: `--ui-small`, `--ui-scale 2`, `--ui-settings`, `--ui-rvc`. Include `--ui-benchmark` with snapshots to suppress automatic microphone startup; without a snapshot it runs the show/hide/reopen check. Run the installed executable in `bin` so settings and model paths resolve correctly. Wait for the QA process to exit before reopening the normal UI. If the user is playing or requests background-only work, do not activate windows, inject input, or launch visible UI checks.

For hardware tasks build `mic_check`, then use `--list` to get **current** device indices. Relevant commands: `--discord-capture` (Discord process capture only, no saved audio/NVIDIA/output), `--monitor-check INPUT_INDEX TAG`, `--smoke-phrases INPUT_INDEX TAG 14 2 40`, `--persistent-tag INPUT_INDEX`, `--tag-reconnect INPUT_INDEX` (stalls the output thread for 120/400 ms against the live host; the session must survive). For supported WASAPI tests replace TAG with the current output index. Consult the relevant README section and `src/check.cpp` arguments before running a hardware test.

Do not record user speech to files without an explicit recording task. Synthetic DSP/ABI checks do not establish physical hotkey behavior, subjective voice quality, USB unplug recovery, lock/sleep/Explorer recovery or game FPS. FPS testing is deferred. Do not change Discord settings for a test.

Headphone transport check: `mic_check --headphones-check OUTPUT_INDEX 0` for dry stereo or `1` for NVIDIA. Resolve a physical output with `--list`; this sends synthetic stereo with final output muted and records no microphone audio. NVIDIA mode requires the VRAM preflight. It needs the headphone-capable TAG host; never restart the host merely to run the check. The check executable uses an 8 MiB stack for its existing audio test objects.

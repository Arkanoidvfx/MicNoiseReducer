---
name: micnoisereducer
description: "Work on MicNoiseReducer in this Windows project: Rust/Iced UI, C++ NVIDIA audio engine, TAG/WASAPI, Discord-only effects, hotkeys, settings, builds and focused checks. Use for project changes or troubleshooting; not for unrelated audio projects or general NVIDIA advice."
---

# MicNoiseReducer

Resolve all source paths below from the project root (the folder containing `AGENTS.md` and `CMakeLists.txt`). Default location: `D:\Projects\Audio\MicNoiseReducer`.

## Find the smallest relevant path

| Task | Read first | Follow through when needed |
|---|---|---|
| UI layout / keyboard interaction | `ui/src/view.rs`, `ui/src/main.rs` | controller tests in `main.rs`; preserve focus and scrolling |
| Settings / defaults / persistence | `ui/src/settings.rs`, relevant `main.rs` handlers | existing INI compatibility tests; never reset live `settings.ini` |
| Gain / pitch / phrase / reverse DSP | `src/effects.hpp` | callers in `src/audio.cpp`, `src/effects_check.cpp` |
| RVC model / import / controls / latency | `ui/src/rvc.rs`, `src/rvc_import.py`, `src/vcclient_server.py` | `src/rvc_import_check.py` (CPU import); RvcClient in `src/audio.cpp`, `src/rvc_check.py`; local `/mnr/convert` PCM API |
| Microphone / output / monitor / Discord capture | `src/audio.cpp`, `src/audio.hpp` | `src/check.cpp`; both TAG and WASAPI paths |
| Hotkeys / stale state / tray / lifecycle | `src/hotkeys.hpp`, `src/bridge.cpp` | `src/bridge_check.cpp`, UI controller |
| C ABI / control messages | `src/bridge.h`, `src/bridge.cpp`, `ui/src/engine.rs` | all native callers and Rust FFI; fixed-width fields/layout |
| Persistent virtual microphone | `src/tag_host.cpp`, `src/tag_link.hpp`, `src/tag.hpp` | `enable-tag-host.ps1`; host owns the driver connection |
| Build / dependencies / linkage | `CMakeLists.txt`, `ui/build.rs`, `build.ps1` | [workflow](references/workflow.md) |

Search symbols in `src` and `ui/src` before reading entire large files. Trace producers, queues and consumers before changing audio semantics. `src/main.cpp` is the old Win32 interface, not the current UI.

## Preserve these contracts

- Current stack: Iced 0.14 with CPU tiny-skia; C++20 static engine via C ABI; NVIDIA SDK 3.0.0.51, Ampere denoiser v1/v2; Rubber Band 4.0.0 LiveShifter. Consult pinned manifests before dependency work. NVIDIA assets are under `vendor/nvidia-afx-3.0.0`, hashes in `nvidia-assets.sha256`.
- Main flow: 48 kHz mono / 480-sample NVIDIA blocks → pitch / phrase DSP → bounded output queue → output effects → Mute → TAG or WASAPI. Keep allocation, file I/O and model work out of the Windows render loop. Inactive pitch must add no permanent latency or replay an old tail.
- `RoutedSample` carries Discord identity, per-sample effect category bits (1 other effects, 2 boost; replay preserves them) and effect epoch through the queue. Apply shared Discord gain to the corresponding output samples, not according to the current hotkey. Both output paths use `OutputEffects`; boost is applied before the queue, and routed microphone background stays separate from Discord gain and effects-only monitoring. `LastEffect` keeps one bounded shared replay slot in DSP. Independent other-effect/boost monitoring (C ABI monitor modes 2/3/4; full voice is 1) taps final TAG/WASAPI output through a bounded SPSC queue; preserve sample flags, epoch rejection and Mute. Full monitoring takes priority over effects-only monitoring on the same Monitor instance.
- Desktop/Discord names in code refer to Windows **process loopback** of native Discord/Canary/PTB. This source bypasses NVIDIA. Source selection remains fixed during a phrase; do not leak the microphone or an old pitch tail across switches.
- RVC (`RvcClient` + `RvcPlayout`) runs on the microphone right after NVIDIA, before the Discord source switch, pitch and phrases: the converted voice is what those effects and the Discord-hold background mix receive. It is a fixed delay line of `chunk + rvcSlack` (200 ms): both rings carry generation-tagged samples with aligned indices, the worker emits exactly one output sample per consumed input (zeros for skipped or failed chunks), late audio is dropped, missing audio is **silence**, never the dry voice. Only the DSP thread trims `output_`. `effects_check` covers the playout; `mic_check --rvc-check` needs the live sidecar.
- There are ten bindings: five microphone effects, then five Discord effects; monitor and replay bindings follow, then microphone alternate-intensity hold (13 ABI keys; legacy 12-key calls clear slot 12). Exact modifiers, duplicate rejection, fresh-press latches and the 250 ms stale-state cutoff are intentional. Preserve resets on Mute/Stop/error/client loss/lock/sleep/rebinding.
- Speed is record-while-held with live microphone passthrough, play-on-release, with pitch coupled to speed. Reverse differs by source: microphone passes the normal phrase; Discord recording sends no normal phrase to the virtual microphone. Both then wait 150 ms before reverse playback. Preserve the smoothed end and bounded phrase storage.
- Boost caps at 2000%; overload is optional hard clipping, not stronger denoising. Discord effects share a saved 0–100% output control, default 50%, after boost/overload. It does not scale microphone samples. See README only for additional ranges and user behavior needed by the task.
- Microphone noise presets use `intensity` / `alternateIntensity`; slot 12 has its own `noiseHeldSample` with the existing epoch/250 ms freshness rules. The DSP selects strength before NVIDIA; no UI polling or model restart is involved. Both levels and the binding persist. Do not mix this hold into effect routing/replay flags.
- Noise suppression above 100% is an experimental SDK request; acceptance and extra suppression are not established. No Speaker Focus or Broadcast-equivalence claims.
- UI starts stopped; close hides to tray; explicit Exit stops the engine but leaves TAG host alive. Unfocused/minimized/tray UI does not poll or animate meters; its lifecycle timer runs at 250 ms versus 50 ms for focused running UI. Native hotkey polling/heartbeat stays independent.
- Headphones are a separate opt-in stereo path: `Headphones` in `audio.cpp`, C ABI `mnr_headphone*`, and the headphones UI page. The same TAG host owns both lines (driver open is exclusive); `HeadphoneLink.v1` is separate from the unchanged microphone `TagLink.v1`. Stop frees headphone models/threads and disconnects its endpoint. Preserve physical-output validation, stereo, bounded queues, final Mute and independent microphone controls.
- TAG host supplies silence without a producer and owns endpoint persistence. IPC uses an acknowledged shared block, not another playback queue. The host drops a producer after 50 ms without data; `TagClient::write` then returns false once, re-registers and `tagLoop` re-primes (`Stats::tagReconnects`) — keep that path, never turn the first rejection back into a session failure. Host/IPC upgrades need coordinated compatibility handling, not a routine host restart workaround.

## Implement and verify

1. Select the path above, inspect current code and make the requested change. For UI changes, follow the applicable taste skill while preserving the existing graphite/orange layout, keyboard focus and 100–200% scaling unless redesign is requested.
2. Read only the relevant section of [workflow.md](references/workflow.md) when building, testing or deploying. A C++ change must rebuild `mic_engine` before Cargo links it. Do not run the full bootstrap build against a live host for an ordinary edit.
3. Run the smallest existing checks that exercise the changed behavior. New non-trivial DSP needs a focused regression in the existing check executable. Verify shared output changes for both routes, neutral bypass, finite bounded samples and Mute.
4. If shipping an executable, follow the install sequence in the workflow. If changing only docs, validate paths and instructions without touching running audio.
5. Report what changed, what actually passed and any untested hardware/listening requirement. Update the relevant README section (behavior only) and add a dated CHANGELOG.md entry (checks, logs, limits) without appending a transcript.

Rubber Band is GPL/commercial; TAG demo distribution remains unresolved. This local personal build does not establish redistribution rights. Do not expand an ordinary feature task into licensing, driver installation or model replacement work.

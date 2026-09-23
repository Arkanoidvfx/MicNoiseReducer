use std::{
    ffi::c_char,
    os::windows::process::CommandExt,
    process::{Child, Command as ProcessCommand, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct Snapshot {
    pub state: i32,
    pub muted: i32,
    pub pitch_active: i32,
    pub boost_active: i32,
    pub input_peak: f32,
    pub output_peak: f32,
    pub process_ms: f32,
    pub queue_ms: f32,
    pub pitch_delay_ms: f32,
    pub pitch_max_ms: f32,
    pub underruns: u32,
    pub drops: u32,
    pub epoch: u32,
    pub captured_key: u32,
    pub rvc_state: i32,
    pub rvc_latency_ms: f32,
}
unsafe extern "C" {
    fn mnr_create(error: *mut c_char, capacity: u32) -> usize;
    fn mnr_destroy(p: usize);
    fn mnr_start(
        p: usize,
        input: *const u8,
        il: u32,
        output: *const u8,
        ol: u32,
        version: i32,
        buffer: u32,
        period: u32,
        graphs: i32,
        intensity: f32,
        error: *mut c_char,
        cap: u32,
    ) -> i32;
    fn mnr_stop(p: usize);
    fn mnr_headphones(
        p: usize,
        enabled: i32,
        output: *const u8,
        length: u32,
        denoise: i32,
        error: *mut c_char,
        cap: u32,
    ) -> i32;
    fn mnr_headphone_controls(p: usize, intensity: f32, volume: f32, pitch: i32, muted: i32);
    fn mnr_headphone_state(p: usize, text: *mut c_char, cap: u32) -> i32;
    fn mnr_rvc_settings(p: usize, slot: u32, pitch: i32, index: u32, chunk_ms: u32, gain: u32);
    fn mnr_monitor(p: usize, enabled: i32, error: *mut c_char, cap: u32) -> i32;
    fn mnr_monitor_state(p: usize, text: *mut c_char, cap: u32) -> i32;
    fn mnr_monitor_peak(p: usize) -> f32;
    fn mnr_controls(
        p: usize,
        volume: f32,
        boost: f32,
        pitch: i32,
        intensity: f32,
        muted: i32,
        slow: f32,
        fast: f32,
        overload: i32,
        discord_volume: f32,
        rvc_enabled: i32,
    );
    fn mnr_phrase_state(p: usize, seconds: *mut f32) -> i32;
    fn mnr_discord_state(p: usize, text: *mut c_char, capacity: u32, active: *mut i32) -> i32;
    fn mnr_denoiser_state(p: usize, text: *mut c_char, capacity: u32) -> i32;
    fn mnr_phrase_cancel(p: usize);
    fn mnr_snapshot(p: usize, s: *mut Snapshot, error: *mut c_char, cap: u32, meters: i32);
    fn mnr_devices(capture: i32, result: *mut c_char, capacity: u32) -> i32;
    fn mnr_gpu(text: *mut c_char, capacity: u32) -> i32;
    fn mnr_bindings(p: usize, keys: *const u32, count: u32);
    fn mnr_alternate_intensity(p: usize, intensity: f32);
    fn mnr_capture_key(p: usize, enabled: i32);
    fn mnr_events(p: usize) -> u32;
    fn mnr_shell_start(p: usize, error: *mut c_char, cap: u32) -> i32;
    fn mnr_tray_hint(p: usize);
    fn mnr_replace_file(from: *const u8, fl: u32, to: *const u8, tl: u32) -> i32;
    fn mnr_usage(cpu: *mut u64, memory: *mut u64);
    fn mnr_sound_load(p: usize, id: u32, samples: *const f32, count: u32, gain: f32) -> i32;
    fn mnr_sound_gain(p: usize, id: u32, gain: f32) -> i32;
    fn mnr_sound_clear(p: usize);
    fn mnr_sound_play(p: usize, id: u32);
    fn mnr_sound_volume(p: usize, volume: f32);
    fn mnr_sound_bindings(p: usize, ids: *const u32, keys: *const u32, count: u32) -> i32;
    fn mnr_sound_state(p: usize, position: *mut f32, length: *mut f32) -> u32;
    fn mnr_pick_paths(mode: i32, result: *mut c_char, capacity: u32) -> i32;
    fn mnr_last_clip(p: usize, out: *mut f32, capacity: u32, generation: *mut u32) -> u32;
}
/// Modal Windows picker; blocks the calling thread, so run it from a background task.
pub fn pick_paths(folder: bool) -> Result<Vec<std::path::PathBuf>, String> {
    let mut b = vec![0u8; 65536];
    match unsafe { mnr_pick_paths(if folder { 0 } else { 1 }, b.as_mut_ptr().cast(), b.len() as u32) } {
        1 => Ok(decoded(&b)
            .lines()
            .filter(|l| !l.is_empty())
            .map(std::path::PathBuf::from)
            .collect()),
        0 => Ok(vec![]),
        _ => Err(decoded(&b)),
    }
}
/// Send-able handle for loading clips from a decode task without holding the controller.
#[derive(Clone, Copy)]
pub struct SoundLoader(usize);
impl SoundLoader {
    pub fn load(&self, id: u32, samples: &[f32], gain: f32) -> Result<(), String> {
        if unsafe { mnr_sound_load(self.0, id, samples.as_ptr(), samples.len() as u32, gain) } == 0 {
            return Err("Движок отклонил звук".into());
        }
        Ok(())
    }
}
pub fn usage() -> (u64, u64) {
    let (mut cpu, mut memory) = (0, 0);
    unsafe { mnr_usage(&mut cpu, &mut memory) };
    (cpu, memory)
}
fn decoded(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())])
        .into_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}
impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}
/// NVIDIA model architecture folder and GPU name of CUDA device 0, as the engine will load it.
pub fn gpu() -> Result<(String, String), String> {
    let mut b = [0u8; 512];
    let ok = unsafe { mnr_gpu(b.as_mut_ptr().cast(), b.len() as u32) } != 0;
    let text = decoded(&b);
    match text.split_once('\t') {
        Some((arch, name)) if ok => Ok((arch.into(), name.into())),
        _ => Err(text),
    }
}
pub fn devices(capture: bool) -> Result<Vec<Device>, String> {
    let mut b = vec![0u8; 65536];
    if unsafe { mnr_devices(capture as i32, b.as_mut_ptr().cast(), b.len() as u32) } == 0 {
        return Err(decoded(&b));
    }
    Ok(decoded(&b)
        .lines()
        .filter_map(|s| {
            s.split_once('\t').map(|(id, name)| Device {
                id: id.into(),
                name: name.into(),
            })
        })
        .collect())
}
#[derive(Clone, Debug)]
pub struct Config {
    pub input: String,
    pub output: String,
    pub version: i32,
    pub buffer: u32,
    pub period: u32,
    pub graphs: i32,
    pub intensity: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub slow: f32,
    pub fast: f32,
    pub volume: f32,
    pub boost: f32,
    pub overload: bool,
    pub discord_volume: f32,
    pub pitch: i32,
    pub intensity: f32,
    pub alternate_intensity: f32,
    pub muted: bool,
    pub rvc: bool,
    pub rvc_options: crate::rvc::Options,
}
#[derive(Clone, Debug)]
pub enum Reply {
    Headphones(Result<(), String>),
    Started(Result<(), String>),
    Quit,
    Devices(Result<(Vec<Device>, Vec<Device>), String>),
    Saved(Result<(), String>),
    Monitor(Result<(), String>),
    Rvc(Result<(), String>),
}
enum Command {
    Headphones(bool, String, bool),
    Start(Config),
    Quit,
    Devices,
    Monitor(i32),
    Rvc(bool, crate::rvc::Options),
    Save(std::path::PathBuf, String),
}
fn start_rvc(options: crate::rvc::Options) -> Result<Child, String> {
    let root = std::env::var_os("MNR_RUNTIME_ROOT")
        .map(std::path::PathBuf::from)
        .ok_or("MNR_RUNTIME_ROOT is not set")?;
    let runtime = root.join("vendor/vcclient-2.1.4-alpha/dist/main");
    let executable = runtime.join("mnr_vcclient_server.exe");
    if !executable.exists() {
        return Err("Свежий RVC-сервер не найден; запустите build.ps1".into());
    }
    ProcessCommand::new(executable)
        .args([
            "--slot",
            &options.slot.to_string(),
            "--pitch",
            &options.pitch.to_string(),
            "--index",
            &options.index.to_string(),
            "--chunk-ms",
            &options.chunk.to_string(),
        ])
        .current_dir(runtime)
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())
}
fn wait_for_exit(
    process: &mut Child,
    timeout: Duration,
) -> Result<Option<std::process::ExitStatus>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = process.try_wait().map_err(|e| e.to_string())? {
            return Ok(Some(status));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        thread::sleep(remaining.min(Duration::from_millis(20)));
    }
}
fn stop_rvc(child: &mut Option<Child>) -> Result<(), String> {
    let Some(process) = child.as_mut() else {
        return Ok(());
    };
    if process.try_wait().map_err(|e| e.to_string())?.is_none() {
        let mut killer = ProcessCommand::new("taskkill")
            .args(["/PID", &process.id().to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Не удалось запустить taskkill: {e}"))?;
        match wait_for_exit(&mut killer, Duration::from_secs(2)) {
            Ok(Some(status)) if status.success() => {}
            result => {
                let _ = killer.kill();
                // Keep the child handle so a later stop can retry safely.
                return Err(format!("Не удалось остановить дерево RVC: {result:?}"));
            }
        }
        if wait_for_exit(process, Duration::from_secs(2))?.is_none() {
            return Err("RVC не завершился за 2 секунды".into());
        }
    }
    child.take();
    Ok(())
}
fn set_rvc(
    child: &mut Option<Child>,
    enabled: bool,
    options: crate::rvc::Options,
) -> Result<(), String> {
    if !enabled {
        stop_rvc(child)
    } else if child.is_none() {
        start_rvc(options).map(|process| *child = Some(process))
    } else {
        Ok(())
    }
}
fn report_rvc_stop(child: &mut Option<Child>, replies: &mpsc::Sender<Reply>) {
    if let Err(error) = stop_rvc(child) {
        // Persist exit-time failures too: the UI may close before reading the reply.
        if let Ok(exe) = std::env::current_exe()
            && let Some(root) = exe.parent().and_then(|p| p.parent())
        {
            let folder = root.join("results");
            if std::fs::create_dir_all(&folder).is_ok() {
                let _ = std::fs::write(folder.join("rvc-stop-error.log"), &error);
            }
        }
        let _ = replies.send(Reply::Rvc(Err(error)));
    }
}
pub struct Engine {
    p: usize,
    tx: mpsc::Sender<Command>,
    rx: mpsc::Receiver<Reply>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Engine {
    pub fn new(controls: Controls) -> Result<Option<Self>, String> {
        let mut error = [0u8; 4096];
        let p = unsafe { mnr_create(error.as_mut_ptr().cast(), 4096) };
        if p == 0 {
            return Err(decoded(&error));
        }
        // Controller tests exercise the real ABI without tray, hotkeys or TAG ownership.
        let shell = if cfg!(test) {
            1
        } else {
            unsafe { mnr_shell_start(p, error.as_mut_ptr().cast(), 4096) }
        };
        if shell <= 0 {
            unsafe { mnr_destroy(p) };
            return if shell == 0 {
                Ok(None)
            } else {
                Err(decoded(&error))
            };
        }
        let (tx, requests) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        unsafe {
            let o = controls.rvc_options;
            mnr_alternate_intensity(p, controls.alternate_intensity);
            mnr_rvc_settings(p, o.slot, o.pitch, o.index, o.chunk, o.gain);
            mnr_controls(
                p,
                controls.volume,
                controls.boost,
                controls.pitch,
                controls.intensity,
                controls.muted as i32,
                controls.slow,
                controls.fast,
                controls.overload as i32,
                controls.discord_volume,
                controls.rvc as i32,
            )
        };
        let initial_rvc = controls.rvc;
        let worker = thread::spawn(move || {
            let mut rvc = None;
            if initial_rvc {
                match start_rvc(controls.rvc_options) {
                    Ok(child) => rvc = Some(child),
                    Err(e) => {
                        let _ = replies.send(Reply::Rvc(Err(e)));
                    }
                }
            }
            loop {
                if let Some(child) = rvc.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            rvc = None;
                            let _ = replies.send(Reply::Rvc(Err(format!(
                                "Сервер завершился ({status}); results/rvc-server-error.log"
                            ))));
                        }
                        Err(e) => {
                            report_rvc_stop(&mut rvc, &replies);
                            let _ = replies.send(Reply::Rvc(Err(e.to_string())));
                        }
                        _ => {}
                    }
                }
                match requests.recv_timeout(Duration::from_millis(250)) {
                    Ok(Command::Headphones(enabled, output, denoise)) => {
                        let mut error = [0u8; 4096];
                        let ok = unsafe {
                            mnr_headphones(
                                p,
                                enabled as i32,
                                output.as_ptr(),
                                output.len() as u32,
                                denoise as i32,
                                error.as_mut_ptr().cast(),
                                4096,
                            )
                        };
                        let _ = replies.send(Reply::Headphones(if ok != 0 {
                            Ok(())
                        } else {
                            Err(decoded(&error))
                        }));
                    }
                    Ok(Command::Start(c)) => {
                        let mut error = [0u8; 4096];
                        let ok = unsafe {
                            mnr_start(
                                p,
                                c.input.as_ptr(),
                                c.input.len() as u32,
                                c.output.as_ptr(),
                                c.output.len() as u32,
                                c.version,
                                c.buffer,
                                c.period,
                                c.graphs,
                                c.intensity,
                                error.as_mut_ptr().cast(),
                                4096,
                            )
                        };
                        let _ = replies.send(Reply::Started(if ok != 0 {
                            Ok(())
                        } else {
                            Err(decoded(&error))
                        }));
                    }
                    Ok(Command::Devices) => {
                        let result = devices(true).and_then(|i| devices(false).map(|o| (i, o)));
                        let _ = replies.send(Reply::Devices(result));
                    }
                    Ok(Command::Monitor(mode)) => {
                        let mut error = [0u8; 4096];
                        let ok = unsafe { mnr_monitor(p, mode, error.as_mut_ptr().cast(), 4096) };
                        let _ = replies.send(Reply::Monitor(if ok != 0 {
                            Ok(())
                        } else {
                            Err(decoded(&error))
                        }));
                    }
                    Ok(Command::Rvc(enabled, options)) => {
                        let result = set_rvc(&mut rvc, enabled, options);
                        let _ = replies.send(Reply::Rvc(result));
                    }
                    Ok(Command::Save(path, contents)) => {
                        let _ = replies.send(Reply::Saved(atomic_save(&path, &contents)));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(Command::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        unsafe {
                            mnr_headphones(p, 0, b"".as_ptr(), 0, 0, std::ptr::null_mut(), 0)
                        };
                        report_rvc_stop(&mut rvc, &replies);
                        unsafe { mnr_stop(p) };
                        let _ = replies.send(Reply::Quit);
                        break;
                    }
                }
            }
        });
        Ok(Some(Self {
            p,
            tx,
            rx,
            worker: Some(worker),
        }))
    }
    pub fn start(&self, c: Config) {
        let _ = self.tx.send(Command::Start(c));
    }
    pub fn headphones(&self, enabled: bool, output: String, denoise: bool) {
        let _ = self.tx.send(Command::Headphones(enabled, output, denoise));
    }
    pub fn headphone_controls(&self, intensity: f32, volume: f32, pitch: i32, muted: bool) {
        unsafe { mnr_headphone_controls(self.p, intensity, volume, pitch, muted as i32) };
    }
    pub fn headphone_state(&self) -> (i32, String) {
        let mut text = [0u8; 4096];
        let state = unsafe { mnr_headphone_state(self.p, text.as_mut_ptr().cast(), 4096) };
        (state, decoded(&text))
    }
    pub fn monitor(&self, mode: i32) {
        let _ = self.tx.send(Command::Monitor(mode));
    }
    pub fn rvc(&self, enabled: bool, options: crate::rvc::Options) {
        let _ = self.tx.send(Command::Rvc(enabled, options));
    }
    pub fn monitor_peak(&self) -> f32 {
        unsafe { mnr_monitor_peak(self.p) }
    }
    pub fn monitor_state(&self) -> (i32, String) {
        let mut text = [0u8; 4096];
        let state = unsafe { mnr_monitor_state(self.p, text.as_mut_ptr().cast(), 4096) };
        (state, decoded(&text))
    }
    pub fn quit(&self) {
        let _ = self.tx.send(Command::Quit);
    }
    pub fn refresh(&self) {
        let _ = self.tx.send(Command::Devices);
    }
    pub fn save(&self, p: std::path::PathBuf, s: String) {
        let _ = self.tx.send(Command::Save(p, s));
    }
    pub fn reply(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
    // Only atomic controls cross threads; lifecycle operations remain worker-owned.
    pub fn controls(&self, c: Controls) {
        unsafe {
            let o = c.rvc_options;
            mnr_alternate_intensity(self.p, c.alternate_intensity);
            mnr_rvc_settings(self.p, o.slot, o.pitch, o.index, o.chunk, o.gain);
            mnr_controls(
                self.p,
                c.volume,
                c.boost,
                c.pitch,
                c.intensity,
                c.muted as i32,
                c.slow,
                c.fast,
                c.overload as i32,
                c.discord_volume,
                c.rvc as i32,
            )
        }
    }
    pub fn bindings(&self, keys: [u32; 13]) {
        unsafe { mnr_bindings(self.p, keys.as_ptr(), keys.len() as u32) }
    }
    pub fn sound_loader(&self) -> SoundLoader {
        SoundLoader(self.p)
    }
    pub fn sound_gain(&self, id: u32, gain: f32) -> bool {
        unsafe { mnr_sound_gain(self.p, id, gain) != 0 }
    }
    pub fn sound_clear(&self) {
        unsafe { mnr_sound_clear(self.p) }
    }
    pub fn sound_play(&self, id: u32) {
        unsafe { mnr_sound_play(self.p, id) }
    }
    pub fn sound_volume(&self, volume: f32) {
        unsafe { mnr_sound_volume(self.p, volume) }
    }
    /// Pairs of (clip id, key); id 0 is the stop key. Rejected as a whole on any conflict.
    pub fn sound_bindings(&self, bindings: &[(u32, u32)]) -> bool {
        let ids: Vec<u32> = bindings.iter().map(|b| b.0).collect();
        let keys: Vec<u32> = bindings.iter().map(|b| b.1).collect();
        unsafe { mnr_sound_bindings(self.p, ids.as_ptr(), keys.as_ptr(), ids.len() as u32) != 0 }
    }
    /// (playing clip id or 0, position seconds, length seconds).
    pub fn sound_state(&self) -> (u32, f32, f32) {
        let (mut position, mut length) = (0.0, 0.0);
        let id = unsafe { mnr_sound_state(self.p, &mut position, &mut length) };
        (id, position, length)
    }
    /// The newest finished hold-effect recording and its generation, or `None` while the
    /// engine has nothing newer than `known`.
    pub fn last_clip(&self, known: u32) -> Option<(u32, Vec<f32>)> {
        let mut generation = 0;
        let count = unsafe { mnr_last_clip(self.p, std::ptr::null_mut(), 0, &mut generation) };
        if generation == known || count == 0 {
            return None;
        }
        let mut samples = vec![0.0; count as usize];
        let written = unsafe { mnr_last_clip(self.p, samples.as_mut_ptr(), count, &mut generation) };
        samples.truncate(written.min(count) as usize);
        Some((generation, samples))
    }
    pub fn discord_state(&self) -> (i32, bool, String) {
        let mut text = [0u8; 2048];
        let mut active = 0;
        let state =
            unsafe { mnr_discord_state(self.p, text.as_mut_ptr().cast(), 2048, &mut active) };
        (state, active != 0, decoded(&text))
    }
    /// 1 NVIDIA denoiser, 2 running without one (with the reason), 0 otherwise.
    pub fn denoiser_state(&self) -> (i32, String) {
        let mut text = [0u8; 1024];
        let state = unsafe { mnr_denoiser_state(self.p, text.as_mut_ptr().cast(), 1024) };
        (state, decoded(&text))
    }
    pub fn phrase(&self) -> (i32, f32) {
        let mut seconds = 0.0;
        let state = unsafe { mnr_phrase_state(self.p, &mut seconds) };
        (state, seconds)
    }
    pub fn cancel_phrase(&self) {
        unsafe { mnr_phrase_cancel(self.p) }
    }
    pub fn capture(&self, enabled: bool) {
        unsafe { mnr_capture_key(self.p, enabled as i32) }
    }
    pub fn hint(&self) {
        unsafe { mnr_tray_hint(self.p) }
    }
    pub fn events(&self) -> u32 {
        unsafe { mnr_events(self.p) }
    }
    pub fn snapshot(&self, meters: bool) -> (Snapshot, String) {
        let mut s = Snapshot::default();
        let mut e = [0u8; 4096];
        unsafe { mnr_snapshot(self.p, &mut s, e.as_mut_ptr().cast(), 4096, meters as i32) };
        (s, decoded(&e))
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        unsafe { mnr_destroy(self.p) }
    }
}

pub fn atomic_save(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let temporary = path.with_extension("ini.tmp");
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    // Win32 profile APIs used by the legacy UI expect UTF-16 LE for Unicode INI files.
    let mut encoded = vec![0xff, 0xfe];
    for word in contents.encode_utf16() {
        encoded.extend(word.to_le_bytes());
    }
    use std::io::Write;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    let from = temporary.to_str().ok_or("Invalid settings path")?;
    let to = path.to_str().ok_or("Invalid settings path")?;
    if unsafe {
        mnr_replace_file(
            from.as_ptr(),
            from.len() as u32,
            to.as_ptr(),
            to.len() as u32,
        )
    } == 0
    {
        return Err("Не удалось заменить settings.ini".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_wait_and_rvc_stop() {
        struct Cleanup(Option<Child>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Some(child) = self.0.as_mut() {
                    let _ = child.kill();
                }
            }
        }
        // A disposable sleeper exercises process control without loading any model.
        let mut child = Cleanup(Some(
            ProcessCommand::new(
                std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                    .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            )
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        ));
        assert!(
            wait_for_exit(child.0.as_mut().unwrap(), Duration::ZERO)
                .unwrap()
                .is_none()
        );
        let options = crate::rvc::Options::load(&crate::settings::Settings::for_test(""));
        // Exercise the same toggle-off path as the worker, with no grace period/model.
        let began = Instant::now();
        set_rvc(&mut child.0, false, options).unwrap();
        assert!(child.0.is_none());
        assert!(began.elapsed() < Duration::from_secs(5));
        set_rvc(&mut child.0, false, options).unwrap();
        assert!(child.0.is_none());

        child.0 = Some(
            ProcessCommand::new("cmd.exe")
                .args(["/C", "exit", "0"])
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(
            wait_for_exit(child.0.as_mut().unwrap(), Duration::from_secs(2))
                .unwrap()
                .unwrap()
                .success()
        );
        stop_rvc(&mut child.0).unwrap();
        assert!(child.0.is_none());
    }
}

#![windows_subsystem = "windows"]
mod components;
mod engine;
mod paths;
mod rvc;
mod settings;
mod telemetry;
mod updater;
mod view;
use engine::{Config, Controls, Device, Engine, Reply, Snapshot};
use iced::{Element, Font, Size, Subscription, Task, Theme, keyboard, window};
use settings::{Settings, key_name};
use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const TAG_HOST_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const TAG_HOST_RUN_NAME: &str = "MicNoiseReducer.TagHost";
const EXIT_EVENT: u32 = 2;
const RESTART_EVENT: u32 = 4;
/// Mirrors `mic::rvcSlack` (src/audio.hpp): RVC output is a fixed delay line of chunk + slack.
const RVC_SLACK_MS: u32 = 200;

/// Keyboard focus targets. Values overlap between pages (each page has its own Tab order) and
/// are asserted literally by the controller tests, so they must never change.
mod focus {
    pub const NONE: usize = usize::MAX;
    /// Page tabs: `TAB_BASE + page` (0 microphone, 1 voice changer, 2 settings, 3 headphones).
    pub const TAB_BASE: usize = 40;
    pub mod bind {
        pub const ACCEPT: usize = 0;
        pub const CLEAR: usize = 1;
        pub const CANCEL: usize = 2;
    }
    pub mod settings {
        pub const INPUT: usize = 0;
        pub const OUTPUT: usize = 1;
        pub const VERSION: usize = 2;
        pub const BUFFER: usize = 3;
        pub const REFRESH: usize = 5;
        pub const DONE: usize = 6;
        pub const QUIT: usize = 7;
        pub const AUTOSTART: usize = 8;
        pub const UPDATE: usize = 57;
        pub const APPLY_UPDATE: usize = 58;
    }
    pub mod effects {
        pub const INPUT: usize = 35;
        pub const INTENSITY: usize = 34;
        pub const ALT_INTENSITY: usize = 37;
        pub const NOISE_BIND: usize = 38;
        pub const OVERLOAD: usize = 21;
        pub const BOOST: usize = 2;
        pub const BOOST_BIND: usize = 3;
        pub const PITCH: usize = 4;
        pub const PITCH_BIND: usize = 5;
        pub const SLOW: usize = 10;
        pub const SLOW_BIND: usize = 11;
        pub const FAST: usize = 12;
        pub const FAST_BIND: usize = 13;
        pub const CANCEL_PHRASE: usize = 14;
        pub const REVERSE_BIND: usize = 15;
        /// Discord bindings of the five effects: `DISCORD_BIND_BASE + effect`.
        pub const DISCORD_BIND_BASE: usize = 16;
        pub const MONITOR: usize = 9;
        pub const MONITOR_BIND: usize = 23;
        pub const REPLAY_BIND: usize = 32;
        pub const DISCORD_VOLUME: usize = 22;
        pub const EFFECTS_MONITOR: usize = 31;
        pub const BOOST_MONITOR: usize = 36;
    }
    pub mod rvc {
        pub const ENABLE: usize = 24;
        pub const MODEL: usize = 25;
        pub const PITCH: usize = 26;
        pub const INDEX: usize = 27;
        pub const GAIN: usize = 28;
        pub const CHUNK: usize = 29;
        pub const REFRESH: usize = 30;
        pub const ADVANCED: usize = 33;
        pub const IMPORT: usize = 39;
        pub const NAME: usize = 44;
        pub const RENAME: usize = 45;
        pub const DELETE: usize = 46;
        pub const INSTALL: usize = 59;
    }
    pub mod headphones {
        pub const OUTPUT: usize = 50;
        pub const TOGGLE: usize = 51;
        pub const NOISE: usize = 52;
        pub const INTENSITY: usize = 53;
        pub const VOLUME: usize = 54;
        pub const PITCH: usize = 55;
        pub const MUTE: usize = 56;
    }
}
static RESTART: AtomicBool = AtomicBool::new(false);

fn restart_requested(events: u32) -> bool {
    events & (EXIT_EVENT | RESTART_EVENT) == RESTART_EVENT
}

fn tag_host_autostart() -> bool {
    Command::new("reg")
        .args(["query", TAG_HOST_RUN_KEY, "/v", TAG_HOST_RUN_NAME])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: a GUI parent would otherwise flash a console.
        .output()
        .is_ok_and(|output| output.status.success())
}

fn set_tag_host_autostart(enabled: bool) -> Result<(), String> {
    let mut command = Command::new("reg");
    command.creation_flags(0x08000000);
    if enabled {
        let paths = paths::Paths::resolve()?;
        let component_host = paths.components.join("bin/mic_tag_host.exe");
        let exe = if component_host.is_file() {
            component_host
        } else {
            paths.app.join("mic_tag_host.exe")
        };
        if !exe.exists() {
            return Err("mic_tag_host.exe не найден; запустите build.ps1".into());
        }
        command.args([
            "add",
            TAG_HOST_RUN_KEY,
            "/v",
            TAG_HOST_RUN_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &format!(r#""{}""#, exe.display()),
            "/f",
        ]);
    } else {
        command.args(["delete", TAG_HOST_RUN_KEY, "/v", TAG_HOST_RUN_NAME, "/f"]);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

#[derive(Debug, Clone)]
enum Msg {
    HeadphoneToggle,
    HeadphoneOutput(Device),
    HeadphoneNoise(bool),
    HeadphoneIntensity(f32),
    HeadphoneVolume(f32),
    HeadphonePitch(f32),
    HeadphoneMute,
    Tick,
    Opened(window::Id),
    WindowFocus(window::Id, bool),
    Hide,
    Show,
    Minimize,
    Drag,
    Quit,
    Monitor,
    EffectsMonitor(bool),
    BoostMonitor(bool),
    Settings,
    Page(u8),
    Refresh,
    Autostart(bool),
    Input(Device),
    Output(Device),
    Version(i32),
    Buffer(u32),
    Intensity(f32),
    AlternateIntensity(f32),
    Boost(f32),
    Overload(bool),
    DiscordVolume(f32),
    Pitch(f32),
    Slow(f32),
    Fast(f32),
    Rvc(bool),
    RvcModel(rvc::Model),
    RvcPitch(f32),
    RvcIndex(f32),
    RvcGain(f32),
    RvcChunk(u32),
    RvcRefresh,
    RvcImport,
    RvcImported(Result<Option<(Vec<rvc::Model>, u32)>, String>),
    RvcName(String),
    RvcRename,
    RvcDelete,
    RvcAdvanced,
    RvcInstall,
    RvcInstalled(Result<String, String>),
    CoreInstalled(Result<String, String>),
    UpdateCheck,
    UpdateChecked(updater::Status),
    ApplyUpdate,
    CancelPhrase,
    Bind(usize),
    CancelBind,
    ClearBind,
    AcceptBind,
    Key(keyboard::Key, keyboard::Modifiers, bool),
    Noop,
    Screenshot(window::Screenshot),
}
struct App {
    headphone_page: bool,
    headphone_output: Option<Device>,
    headphone_denoise: bool,
    headphone_intensity: f32,
    headphone_volume: f32,
    headphone_pitch: i32,
    headphone_muted: bool,
    headphone_state: i32,
    headphone_busy: bool,
    headphone_message: String,
    engine: Engine,
    settings: Settings,
    window: Option<window::Id>,
    window_focused: bool,
    hidden_window: Option<window::Id>,
    inputs: Vec<Device>,
    outputs: Vec<Device>,
    input: Option<Device>,
    output: Option<Device>,
    controls: Controls,
    rvc_models: Vec<rvc::Model>,
    runtime_root: PathBuf,
    component_root: PathBuf,
    core_installing: bool,
    rvc_runtime_installed: bool,
    rvc_runtime_installing: bool,
    keys: [u32; 13],
    discord_state: i32,
    discord_source: bool,
    discord_message: String,
    phrase_state: i32,
    phrase_seconds: f32,
    version: i32,
    buffer: u32,
    period: u32,
    graphs: i32,
    snapshot: Snapshot,
    monitor: i32,
    monitor_all: bool,
    effects_monitor: bool,
    boost_monitor: bool,
    monitor_message: String,
    message: String,
    details: bool,
    rvc_page: bool,
    rvc_advanced: bool,
    rvc_importing: bool,
    rvc_import_note: String,
    rvc_name: String,
    rvc_delete_confirm: bool,
    update_checking: bool,
    update_ready: bool,
    update_status: String,
    apply_after_quit: bool,
    busy: bool,
    quitting: bool,
    binding: Option<usize>,
    candidate: u32,
    auto_started: bool,
    focus: usize,
    dirty: Option<Instant>,
    hint_shown: bool,
    autostart: bool,
    tray_ok: bool,
    peak: f32,
    ticks: u64,
    capture_path: Option<PathBuf>,
    capture_started: bool,
    qa_scale: f32,
    benchmark: bool,
    usage: (Instant, u64),
    measurements: String,
}
fn timer(visible: bool) -> Task<Msg> {
    Task::perform(
        async move {
            std::thread::sleep(Duration::from_millis(if visible { 50 } else { 250 }));
        },
        |_| Msg::Tick,
    )
}
fn window_icon() -> window::Icon {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/app-64.png")));
    let mut reader = decoder
        .read_info()
        .expect("decode embedded application icon");
    let mut rgba = vec![0; reader.output_buffer_size().expect("application icon size")];
    let info = reader
        .next_frame(&mut rgba)
        .expect("read embedded application icon");
    window::icon::from_rgba(rgba[..info.buffer_size()].to_vec(), info.width, info.height)
        .expect("valid embedded application icon")
}
impl App {
    fn open(_scale: f32) -> (window::Id, Task<Msg>) {
        let small = std::env::args().any(|s| s == "--ui-small");
        let (id, task) = window::open(window::Settings {
            size: if small {
                Size::new(620.0, 440.0)
            } else {
                Size::new(820.0, 820.0)
            },
            min_size: Some(if small {
                Size::new(620.0, 440.0) // Explicit QA mode only.
            } else {
                Size::new(820.0, 820.0)
            }),
            position: window::Position::Centered,
            icon: Some(window_icon()),
            decorations: false,
            exit_on_close_request: false,
            ..Default::default()
        });
        (id, task.map(Msg::Opened))
    }
    fn new() -> Result<Option<(Self, Task<Msg>)>, String> {
        let paths = paths::Paths::resolve()?;
        let runtime_root = paths.runtime_root().to_path_buf();
        // Native engine and the RVC worker share this version-independent component root.
        unsafe { std::env::set_var("MNR_RUNTIME_ROOT", &runtime_root) };
        let settings = Settings::load(&paths.data)?;
        telemetry::record(&paths.data, "session-start", serde_json::json!({}));
        Self::from_settings_and_runtime(settings, runtime_root, paths.components)
    }
    #[cfg(test)]
    fn from_settings(settings: Settings) -> Result<Option<(Self, Task<Msg>)>, String> {
        let runtime_root = settings
            .path
            .parent()
            .ok_or("Invalid settings path")?
            .to_path_buf();
        Self::from_settings_and_runtime(settings, runtime_root.clone(), runtime_root)
    }
    fn from_settings_and_runtime(
        settings: Settings,
        runtime_root: PathBuf,
        component_root: PathBuf,
    ) -> Result<Option<(Self, Task<Msg>)>, String> {
        let headphone_denoise = settings.number("headphones", "denoise", 1, 0, 1) != 0;
        let headphone_intensity =
            settings.number("headphones", "intensity", 80, 0, 200) as f32 / 100.0;
        let headphone_volume = settings.number("headphones", "volume", 70, 0, 100) as f32 / 100.0;
        let headphone_pitch = settings.number("headphones", "pitch", 0, -12, 12);
        let effects_monitor = settings.number("effects", "monitor_effects", 0, 0, 1) != 0;
        let boost_monitor = settings.number("effects", "monitor_boost", 0, 0, 1) != 0;
        let controls = Controls {
            slow: settings.number("effects", "slow_speed", 70, 50, 95) as f32 / 100.0,
            fast: settings.number("effects", "fast_speed", 150, 105, 200) as f32 / 100.0,
            volume: 1.0,
            boost: settings.number("effects", "boost", 300, 100, 2000) as f32 / 100.0,
            overload: settings.number("effects", "overload", 0, 0, 1) != 0,
            discord_volume: settings.number("effects", "discord_volume", 50, 0, 100) as f32 / 100.0,
            pitch: settings.number("effects", "pitch", -5, -12, 12),
            intensity: settings.number("audio", "intensity", 100, 0, 200) as f32 / 100.0,
            alternate_intensity: settings.number("audio", "alternate_intensity", 15, 0, 200) as f32
                / 100.0,
            muted: false,
            rvc: settings.number("effects", "rvc_enabled", 0, 0, 1) != 0,
            rvc_options: rvc::Options::load(&settings),
        };
        let Some(engine) = Engine::new(controls)? else {
            return Ok(None);
        };
        let mut keys = [
            settings.number("effects", "boost_key", 0, 0, 2046) as u32,
            settings.number("effects", "pitch_key", 0, 0, 2046) as u32,
            settings.number("effects", "slow_key", 0, 0, 2046) as u32,
            settings.number("effects", "fast_key", 0, 0, 2046) as u32,
            settings.number("effects", "reverse_key", 0, 0, 2046) as u32,
            settings.number("effects", "discord_boost_key", 0, 0, 2046) as u32,
            settings.number("effects", "discord_pitch_key", 0, 0, 2046) as u32,
            settings.number("effects", "discord_slow_key", 0, 0, 2046) as u32,
            settings.number("effects", "discord_fast_key", 0, 0, 2046) as u32,
            settings.number("effects", "discord_reverse_key", 0, 0, 2046) as u32,
            settings.number("effects", "monitor_key", 0, 0, 2046) as u32,
            settings.number("effects", "replay_key", 119 | 256, 0, 2046) as u32,
            settings.number("effects", "noise_key", 0, 0, 2046) as u32,
        ];
        for i in 0..keys.len() {
            if keys[i] != 0
                && (!(3..=254).contains(&(keys[i] & 255)) || keys[..i].contains(&keys[i]))
            {
                keys[i] = 0;
            }
        }
        engine.bindings(keys);
        if !cfg!(test) {
            engine.refresh();
        }
        let version = settings.number("audio", "version", 2, 1, 2);
        let buffer = settings.number("audio", "buffer_ms", 40, 10, 80) as u32;
        let period = settings.number("audio", "period_ms", 5, 2, 20) as u32;
        let graphs = settings.number("audio", "cuda_graphs", -1, -1, 1);
        let hint_shown = settings.get("ui", "tray_hint") == Some("1");
        let args: Vec<_> = std::env::args().collect();
        let capture_path = args
            .iter()
            .position(|s| s == "--ui-snapshot")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        let qa_scale = args
            .iter()
            .position(|s| s == "--ui-scale")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 1.0 && *v <= 2.0)
            .unwrap_or(1.0);
        let (window, open) = if cfg!(test) || args.iter().any(|s| s == "--start-tray") {
            (None, Task::none())
        } else {
            let (id, task) = Self::open(qa_scale);
            (Some(id), task)
        };
        let core_installing = !cfg!(test) && !components::core_installed(&runtime_root);
        let rvc_runtime_installed = cfg!(test) || components::rvc_installed(&runtime_root);
        let rvc_models = if cfg!(test) {
            vec![]
        } else {
            rvc::models(&runtime_root).unwrap_or_default()
        };
        let rvc_name = rvc_models
            .iter()
            .find(|model| model.slot == controls.rvc_options.slot)
            .map(|model| model.name.clone())
            .unwrap_or_default();
        Ok(Some((
            Self {
                engine,
                settings,
                window,
                window_focused: false,
                hidden_window: None,
                inputs: vec![],
                outputs: vec![],
                input: None,
                output: None,
                headphone_page: args.iter().any(|s| s == "--ui-headphones"),
                headphone_output: None,
                headphone_denoise,
                headphone_intensity,
                headphone_volume,
                headphone_pitch,
                headphone_muted: false,
                headphone_state: 0,
                headphone_busy: false,
                headphone_message: String::new(),
                controls,
                rvc_models,
                runtime_root,
                component_root: component_root.clone(),
                core_installing,
                rvc_runtime_installed,
                rvc_runtime_installing: false,
                keys,
                phrase_state: 0,
                discord_state: 0,
                discord_source: false,
                discord_message: String::new(),
                phrase_seconds: 0.0,
                version,
                buffer,
                period,
                graphs,
                snapshot: Snapshot::default(),
                monitor: 0,
                monitor_all: false,
                effects_monitor,
                boost_monitor,
                monitor_message: String::new(),
                message: if core_installing {
                    "Устанавливаем основной NVIDIA/TAG runtime…".into()
                } else {
                    String::new()
                },
                details: args.iter().any(|s| s == "--ui-settings"),
                rvc_page: args.iter().any(|s| s == "--ui-rvc"),
                rvc_advanced: false,
                rvc_importing: false,
                rvc_import_note: String::new(),
                rvc_name,
                rvc_delete_confirm: false,
                update_checking: !cfg!(test),
                update_ready: false,
                update_status: if cfg!(test) {
                    String::new()
                } else {
                    "Проверяем обновления…".into()
                },
                apply_after_quit: false,
                busy: false,
                quitting: false,
                binding: None,
                candidate: 0,
                auto_started: false,
                focus: focus::NONE,
                dirty: None,
                hint_shown,
                autostart: !cfg!(test) && tag_host_autostart(),
                tray_ok: true,
                peak: 0.0,
                ticks: 0,
                capture_path,
                capture_started: false,
                qa_scale,
                benchmark: cfg!(test) || args.iter().any(|s| s == "--ui-benchmark"),
                usage: (Instant::now(), 0),
                measurements: "state,cpu_one_core_percent,working_set_mb\n".into(),
            },
            Task::batch([
                open,
                timer(true),
                if cfg!(test) {
                    Task::none()
                } else {
                    Task::perform(async { updater::check_and_download() }, Msg::UpdateChecked)
                },
                if core_installing {
                    Task::perform(
                        async move { components::install_core(&component_root) },
                        Msg::CoreInstalled,
                    )
                } else {
                    Task::none()
                },
            ]),
        )))
    }
    fn config(&self) -> Option<Config> {
        Some(Config {
            input: self.input.as_ref()?.id.clone(),
            output: self.output.as_ref()?.id.clone(),
            version: self.version,
            buffer: self.buffer,
            period: self.period,
            graphs: self.graphs,
            intensity: self.controls.intensity,
        })
    }
    fn save(&mut self) {
        if let Some(d) = &self.headphone_output {
            self.settings.set("headphones", "output", &d.id);
        }
        self.settings
            .set("headphones", "denoise", self.headphone_denoise as i32);
        self.settings.set(
            "headphones",
            "intensity",
            (self.headphone_intensity * 100.0).round() as i32,
        );
        self.settings.set(
            "headphones",
            "volume",
            (self.headphone_volume * 100.0).round() as i32,
        );
        self.settings
            .set("headphones", "pitch", self.headphone_pitch);
        if self.benchmark {
            self.dirty = None;
            return;
        }
        if let Some(d) = &self.input {
            self.settings.set("audio", "input", &d.id);
        }
        if let Some(d) = &self.output {
            self.settings.set("audio", "output", &d.id);
        }
        for (k, v) in [
            ("version", self.version),
            ("buffer_ms", self.buffer as i32),
            (
                "intensity",
                (self.controls.intensity * 100.0).round() as i32,
            ),
            (
                "alternate_intensity",
                (self.controls.alternate_intensity * 100.0).round() as i32,
            ),
        ] {
            self.settings.set("audio", k, v);
        }
        for (k, v) in [
            ("volume", 100),
            ("boost", (self.controls.boost * 100.0).round() as i32),
            ("overload", self.controls.overload as i32),
            (
                "discord_volume",
                (self.controls.discord_volume * 100.0).round() as i32,
            ),
            ("pitch", self.controls.pitch),
            ("boost_key", self.keys[0] as i32),
            ("pitch_key", self.keys[1] as i32),
            ("slow_key", self.keys[2] as i32),
            ("fast_key", self.keys[3] as i32),
            ("reverse_key", self.keys[4] as i32),
            ("discord_boost_key", self.keys[5] as i32),
            ("discord_pitch_key", self.keys[6] as i32),
            ("discord_slow_key", self.keys[7] as i32),
            ("discord_fast_key", self.keys[8] as i32),
            ("discord_reverse_key", self.keys[9] as i32),
            ("monitor_key", self.keys[10] as i32),
            ("replay_key", self.keys[11] as i32),
            ("noise_key", self.keys[12] as i32),
            ("monitor_effects", self.effects_monitor as i32),
            ("monitor_boost", self.boost_monitor as i32),
            ("slow_speed", (self.controls.slow * 100.0).round() as i32),
            ("fast_speed", (self.controls.fast * 100.0).round() as i32),
            ("rvc_enabled", self.controls.rvc as i32),
        ] {
            self.settings.set("effects", k, v);
        }
        self.settings.set("ui", "tray_hint", self.hint_shown as i32);
        self.controls.rvc_options.save(&mut self.settings);
        self.engine
            .save(self.settings.path.clone(), self.settings.text());
        self.dirty = None;
    }
    fn headphone_changed(&mut self) {
        self.engine.headphone_controls(
            self.headphone_intensity,
            self.headphone_volume,
            self.headphone_pitch,
            self.headphone_muted,
        );
        self.dirty = Some(Instant::now());
    }
    fn headphone_outputs(&self) -> Vec<Device> {
        self.outputs
            .iter()
            .filter(|d| {
                let name = d.name.to_lowercase();
                d.id != "TAG"
                    && ![
                        "thin audio",
                        "micnoisereducer",
                        "cable",
                        "voicemeeter",
                        "broadcast",
                    ]
                    .iter()
                    .any(|v| name.contains(v))
            })
            .cloned()
            .collect()
    }
    fn changed(&mut self) {
        self.engine.controls(self.controls);
        self.dirty = Some(Instant::now());
    }
    fn running(&self) -> bool {
        matches!(self.snapshot.state, 1..=4)
    }
    fn ui_active(&self) -> bool {
        self.window.is_some() && self.window_focused
    }
    fn monitor_mode(&self) -> i32 {
        if self.monitor_all {
            1
        } else {
            match (self.effects_monitor, self.boost_monitor) {
                (false, false) => 0,
                (true, false) => 2,
                (false, true) => 3,
                (true, true) => 4,
            }
        }
    }
    fn auto_start(&mut self) {
        if self.auto_started
            || self.benchmark
            || self.busy
            || self.running()
            || self.core_installing
        {
            return;
        }
        let Some(c) = self.config() else { return };
        let output = self.output.as_ref().unwrap();
        if output.id != "TAG" && !output.name.contains("Voicemeeter") {
            self.message =
                "Автозапуск остановлен: выберите TAG или Voicemeeter в настройках.".into();
            return;
        }
        self.auto_started = true;
        self.busy = true;
        self.snapshot.state = 1;
        self.save();
        self.engine.start(c);
    }
    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => {
                self.ticks += 1;
                if self.rvc_runtime_installing
                    && let Some(progress) = components::progress()
                {
                    self.rvc_import_note = format!("Загрузка RVC runtime: {progress}%");
                }
                if self.core_installing
                    && let Some(progress) = components::progress()
                {
                    self.message = format!("Установка NVIDIA/TAG runtime: {progress}%");
                }
                if self.benchmark {
                    if self.ticks == 8 {
                        self.usage = (Instant::now(), engine::usage().0);
                    }
                    if [24, 44, 60].contains(&self.ticks) {
                        let (cpu, memory) = engine::usage();
                        let name = match self.ticks {
                            24 => "open_stopped",
                            44 => "tray_stopped",
                            _ => "reopened_stopped",
                        };
                        self.measurements.push_str(&format!(
                            "{name},{:.3},{:.2}\n",
                            (cpu - self.usage.1) as f64
                                / 100_000.0
                                / self.usage.0.elapsed().as_secs_f64(),
                            memory as f64 / 1048576.0
                        ));
                        self.usage = (Instant::now(), cpu);
                        if self.ticks == 60 {
                            let path = self
                                .settings
                                .path
                                .parent()
                                .unwrap()
                                .join("results/rust-ui-resources.csv");
                            if let Err(e) = std::fs::write(path, &self.measurements) {
                                self.message = e.to_string();
                            }
                            return Task::batch([self.update(Msg::Quit), timer(false)]);
                        }
                        let action = if self.ticks == 24 {
                            Msg::Hide
                        } else {
                            Msg::Show
                        };
                        return Task::batch([self.update(action), timer(false)]);
                    }
                }
                while let Some(reply) = self.engine.reply() {
                    match reply {
                        Reply::Started(result) => {
                            self.busy = false;
                            self.monitor_all = false;
                            self.engine.controls(self.controls);
                            if let Err(e) = result {
                                self.message = e;
                                self.snapshot.state = 5;
                            } else if self.effects_monitor || self.boost_monitor {
                                self.engine.monitor(self.monitor_mode());
                            }
                        }
                        Reply::Quit => {
                            telemetry::record_blocking(
                                self.settings.path.parent().unwrap(),
                                "session-end",
                                serde_json::json!({
                                    "update": self.apply_after_quit,
                                    "underruns": self.snapshot.underruns,
                                    "drops": self.snapshot.drops,
                                    "process_ms": self.snapshot.process_ms,
                                    "queue_ms": self.snapshot.queue_ms,
                                    "rvc_latency_ms": self.snapshot.rvc_latency_ms,
                                }),
                            );
                            if self.apply_after_quit
                                && let Err(error) = updater::apply_and_restart()
                            {
                                self.quitting = false;
                                self.busy = false;
                                self.apply_after_quit = false;
                                self.message = format!("Обновление: {error}");
                            } else {
                                return iced::exit();
                            }
                        }
                        Reply::Saved(result) => {
                            if let Err(e) = result {
                                self.message = format!("Настройки не сохранены: {e}");
                            }
                        }
                        Reply::Monitor(result) => {
                            if let Err(e) = result {
                                self.message = format!("Прослушивание: {e}");
                            }
                        }
                        Reply::Headphones(result) => {
                            self.headphone_busy = false;
                            if let Err(e) = result {
                                self.headphone_message = e;
                                self.headphone_state = 3;
                            }
                            self.engine.refresh();
                        }
                        Reply::Rvc(result) => {
                            if let Err(e) = result {
                                self.controls.rvc = false;
                                self.engine.controls(self.controls);
                                self.message = format!("Ошибка RVC: {e}");
                                self.dirty = Some(Instant::now());
                            }
                        }
                        Reply::Devices(result) => match result {
                            Ok((i, o)) => {
                                let input_id = self
                                    .input
                                    .as_ref()
                                    .map(|d| d.id.as_str())
                                    .or(self.settings.get("audio", "input"));
                                let output_id = self
                                    .output
                                    .as_ref()
                                    .map(|d| d.id.as_str())
                                    .or(self.settings.get("audio", "output"))
                                    .unwrap_or("TAG");
                                self.input = i
                                    .iter()
                                    .find(|d| {
                                        input_id.map_or(d.name.contains("HyperX"), |id| d.id == id)
                                    })
                                    .cloned();
                                self.output = o.iter().find(|d| d.id == output_id).cloned();
                                self.inputs = i;
                                if self.headphone_output.is_none() {
                                    self.headphone_output = o
                                        .iter()
                                        .find(|d| {
                                            Some(d.id.as_str())
                                                == self.settings.get("headphones", "output")
                                        })
                                        .cloned();
                                }
                                self.outputs = o;
                                self.auto_start();
                            }
                            Err(e) => self.message = e,
                        },
                    }
                }
                let (headphone_state, headphone_error) = self.engine.headphone_state();
                if !self.headphone_busy {
                    if headphone_state != 0 || self.headphone_state != 3 {
                        self.headphone_state = headphone_state;
                    }
                    if !headphone_error.is_empty() {
                        self.headphone_message = headphone_error;
                    }
                }
                let (snapshot, error) = self.engine.snapshot(self.ui_active());
                self.snapshot = snapshot;
                (self.phrase_state, self.phrase_seconds) = self.engine.phrase();
                (
                    self.discord_state,
                    self.discord_source,
                    self.discord_message,
                ) = self.engine.discord_state();
                let (monitor, monitor_message) = self.engine.monitor_state();
                if monitor == 3 && self.monitor != 3 {
                    self.message = format!("Прослушивание: {monitor_message}");
                }
                self.monitor = monitor;
                self.monitor_message = monitor_message;
                if snapshot.state == 5 && !error.is_empty() {
                    self.message = error;
                }
                if self.ui_active() {
                    self.peak = snapshot.output_peak.max(self.peak * 0.80);
                }
                if snapshot.captured_key != 0 && self.binding.is_some() {
                    if snapshot.captured_key == u32::MAX {
                        return Task::batch([
                            self.update(Msg::CancelBind),
                            timer(self.ui_active()),
                        ]);
                    }
                    self.candidate = snapshot.captured_key;
                }
                let events = self.engine.events();
                if events & 16 != 0 {
                    self.tray_ok = false;
                    self.message =
                        "Не удалось создать значок трея. Окно останется доступным.".into();
                }
                if events & 8 != 0 && !self.busy && matches!(self.snapshot.state, 2 | 3) {
                    let _ = self.update(Msg::Monitor);
                }
                if restart_requested(events) {
                    RESTART.store(true, Ordering::Relaxed);
                }
                if events & (EXIT_EVENT | RESTART_EVENT) != 0 && !self.quitting {
                    self.quitting = true;
                    self.save();
                    self.engine.quit();
                }
                if self
                    .dirty
                    .is_some_and(|t| t.elapsed() > Duration::from_millis(400))
                {
                    self.save();
                }
                let next = timer(self.ui_active() && self.running());
                if events & 17 != 0 {
                    return Task::batch([next, self.update(Msg::Show)]);
                }
                if self.capture_path.is_some() && !self.capture_started && self.ticks >= 12 {
                    self.capture_started = true;
                    if let Some(id) = self.window {
                        return Task::batch([next, window::screenshot(id).map(Msg::Screenshot)]);
                    }
                }
                return next;
            }
            Msg::Opened(id) => {
                self.window = Some(id);
            }
            Msg::WindowFocus(id, focused) => {
                if self.window == Some(id) {
                    self.window_focused = focused;
                }
            }
            Msg::Hide => {
                if !self.tray_ok {
                    return Task::none();
                }
                self.engine.capture(false);
                self.binding = None;
                if !self.hint_shown {
                    self.engine.hint();
                    self.hint_shown = true;
                    self.dirty = Some(Instant::now());
                }
                if let Some(id) = self.window.take() {
                    self.window_focused = false;
                    // Keep the native window alive while audio continues in the tray.
                    self.hidden_window = Some(id);
                    return window::set_mode(id, window::Mode::Hidden);
                }
            }
            Msg::Show => {
                if let Some(id) = self.window {
                    return Task::batch([window::minimize(id, false), window::gain_focus(id)]);
                }
                if let Some(id) = self.hidden_window.take() {
                    self.window = Some(id);
                    return window::set_mode(id, window::Mode::Windowed)
                        .chain(window::minimize(id, false))
                        .chain(window::gain_focus(id));
                }
                let (id, t) = Self::open(self.qa_scale);
                self.window = Some(id);
                return t;
            }
            Msg::Minimize => {
                self.window_focused = false;
                if let Some(id) = self.window {
                    return window::minimize(id, true);
                }
            }
            Msg::Drag => {
                if let Some(id) = self.window {
                    return window::drag(id);
                }
            }
            Msg::Quit => {
                if !self.quitting {
                    self.quitting = true;
                    self.busy = true;
                    self.save();
                    self.engine.quit();
                }
            }
            Msg::UpdateCheck => {
                if !self.update_checking {
                    self.update_checking = true;
                    self.update_status = "Проверяем обновления…".into();
                    return Task::perform(
                        async { updater::check_and_download() },
                        Msg::UpdateChecked,
                    );
                }
            }
            Msg::UpdateChecked(status) => {
                self.update_checking = false;
                match status {
                    updater::Status::Current => {
                        self.update_ready = false;
                        self.update_status = "Установлена актуальная версия".into();
                    }
                    updater::Status::Ready(version) => {
                        self.update_ready = true;
                        self.update_status = format!("Версия {version} скачана и готова");
                    }
                    updater::Status::Unavailable(error) => {
                        self.update_ready = false;
                        self.update_status =
                            if error.contains("locat") || error.contains("manifest") {
                                "Обновления доступны после установки через Setup.exe".into()
                            } else {
                                format!("Не удалось проверить обновления: {error}")
                            };
                    }
                }
            }
            Msg::ApplyUpdate => {
                if self.update_ready && !self.quitting {
                    self.apply_after_quit = true;
                    return self.update(Msg::Quit);
                }
            }
            Msg::Monitor => {
                self.focus = focus::effects::MONITOR;
                if !self.busy && !self.quitting && matches!(self.snapshot.state, 2 | 3) {
                    self.message.clear();
                    self.monitor_all = !(self.monitor_all && matches!(self.monitor, 1 | 2));
                    self.engine.monitor(self.monitor_mode());
                }
            }
            Msg::EffectsMonitor(enabled) | Msg::BoostMonitor(enabled) => {
                if matches!(msg, Msg::BoostMonitor(_)) {
                    self.boost_monitor = enabled;
                    self.focus = focus::effects::BOOST_MONITOR;
                } else {
                    self.effects_monitor = enabled;
                    self.focus = focus::effects::EFFECTS_MONITOR;
                }
                self.dirty = Some(Instant::now());
                if !matches!(self.monitor, 1 | 2) {
                    self.monitor_all = false;
                }
                if !self.monitor_all && !self.busy && !self.quitting && self.running() {
                    self.message.clear();
                    self.engine.monitor(self.monitor_mode());
                }
            }
            Msg::Settings => {
                return self.update(Msg::Page(if self.details { 0 } else { 2 }));
            }
            Msg::Page(page) => {
                if self.binding.is_some() {
                    return Task::none();
                }
                self.headphone_page = page == 3;
                self.details = page == 2;
                self.rvc_page = page == 1;
                self.focus = focus::NONE;
                return iced::widget::operation::snap_to(
                    "body",
                    iced::widget::scrollable::RelativeOffset::START,
                );
            }
            Msg::HeadphoneToggle => {
                if !self.headphone_busy {
                    let enabled = !matches!(self.headphone_state, 1 | 2);
                    if enabled && self.headphone_output.is_none() {
                        self.headphone_message = "Выберите физические наушники.".into();
                    } else {
                        self.engine.headphone_controls(
                            self.headphone_intensity,
                            self.headphone_volume,
                            self.headphone_pitch,
                            self.headphone_muted,
                        );
                        self.headphone_busy = true;
                        self.headphone_message.clear();
                        self.engine.headphones(
                            enabled,
                            self.headphone_output
                                .as_ref()
                                .map(|d| d.id.clone())
                                .unwrap_or_default(),
                            self.headphone_denoise,
                        );
                    }
                }
                self.focus = focus::headphones::TOGGLE;
            }
            Msg::HeadphoneOutput(d) => {
                if !self.headphone_busy && self.headphone_outputs().contains(&d) {
                    let restart = matches!(self.headphone_state, 1 | 2);
                    self.headphone_output = Some(d.clone());
                    self.dirty = Some(Instant::now());
                    if restart {
                        self.headphone_busy = true;
                        self.headphone_message.clear();
                        self.engine.headphones(true, d.id, self.headphone_denoise);
                    }
                }
                self.focus = focus::headphones::OUTPUT;
            }
            Msg::HeadphoneNoise(value) => {
                if !self.headphone_busy && !matches!(self.headphone_state, 1 | 2) {
                    self.headphone_denoise = value;
                    self.dirty = Some(Instant::now());
                }
                self.focus = focus::headphones::NOISE;
            }
            Msg::HeadphoneIntensity(v) => {
                self.headphone_intensity = (v / 100.0).clamp(0.0, 2.0);
                self.headphone_changed();
                self.focus = focus::headphones::INTENSITY;
            }
            Msg::HeadphoneVolume(v) => {
                self.headphone_volume = (v / 100.0).clamp(0.0, 1.0);
                self.headphone_changed();
                self.focus = focus::headphones::VOLUME;
            }
            Msg::HeadphonePitch(v) => {
                self.headphone_pitch = (v.round() as i32).clamp(-12, 12);
                self.headphone_changed();
                self.focus = focus::headphones::PITCH;
            }
            Msg::HeadphoneMute => {
                self.headphone_muted = !self.headphone_muted;
                self.headphone_changed();
                self.focus = focus::headphones::MUTE;
            }
            Msg::Refresh => {
                if !self.running() && !self.busy {
                    self.auto_started = false;
                    self.engine.refresh();
                }
                self.focus = focus::settings::REFRESH;
            }
            Msg::Autostart(enabled) => {
                self.focus = focus::settings::AUTOSTART;
                match set_tag_host_autostart(enabled) {
                    Ok(()) => self.autostart = enabled,
                    Err(e) => self.message = format!("Автозапуск не изменён: {e}"),
                }
            }
            Msg::Input(d) => {
                if !self.busy && self.inputs.contains(&d) {
                    let restart = self.running();
                    self.input = Some(d);
                    self.focus = if self.details {
                        focus::settings::INPUT
                    } else {
                        focus::effects::INPUT
                    };
                    self.changed();
                    if restart {
                        if let Some(config) = self.config() {
                            self.busy = true;
                            self.snapshot.state = 1;
                            self.message.clear();
                            self.save();
                            self.engine.start(config);
                        }
                    } else {
                        self.auto_started = false;
                        self.auto_start();
                    }
                }
            }
            Msg::Output(d) => {
                if !self.running() && !self.busy {
                    self.output = Some(d);
                    self.focus = focus::settings::OUTPUT;
                    self.changed();
                    self.auto_started = false;
                    self.auto_start();
                }
            }
            Msg::Version(v) => {
                if !self.running() && !self.busy {
                    self.version = v;
                    self.focus = focus::settings::VERSION;
                    self.changed();
                }
            }
            Msg::Buffer(b) => {
                if !self.running() && !self.busy {
                    self.buffer = b;
                    self.focus = focus::settings::BUFFER;
                    self.changed();
                }
            }
            Msg::Intensity(v) => {
                self.controls.intensity = (v / 100.0).clamp(0.0, 2.0);
                self.focus = focus::effects::INTENSITY;
                self.changed();
            }
            Msg::AlternateIntensity(v) => {
                self.controls.alternate_intensity = (v / 100.0).clamp(0.0, 2.0);
                self.focus = focus::effects::ALT_INTENSITY;
                self.changed();
            }
            Msg::Boost(v) => {
                self.controls.boost = v / 100.0;
                self.focus = focus::effects::BOOST;
                self.changed();
            }
            Msg::Overload(v) => {
                self.controls.overload = v;
                self.focus = focus::effects::OVERLOAD;
                self.changed();
            }
            Msg::DiscordVolume(v) => {
                self.controls.discord_volume = v / 100.0;
                self.focus = focus::effects::DISCORD_VOLUME;
                self.changed();
            }
            Msg::Pitch(v) => {
                self.controls.pitch = v as i32;
                self.focus = focus::effects::PITCH;
                self.changed();
            }
            Msg::Slow(v) => {
                self.controls.slow = v / 100.0;
                self.focus = focus::effects::SLOW;
                self.changed();
            }
            Msg::Fast(v) => {
                self.controls.fast = v / 100.0;
                self.focus = focus::effects::FAST;
                self.changed();
            }
            Msg::Rvc(v) => {
                if v && !self.rvc_runtime_installed {
                    return self.update(Msg::RvcInstall);
                }
                if v && !self
                    .rvc_models
                    .iter()
                    .any(|m| m.slot == self.controls.rvc_options.slot)
                {
                    self.message = "Выберите установленную модель RVC".into();
                    return Task::none();
                }
                self.controls.rvc = v;
                self.focus = focus::rvc::ENABLE;
                // Stop sending audio before tearing down the server process.
                self.changed();
                self.engine.rvc(v, self.controls.rvc_options);
            }
            Msg::RvcInstall => {
                if self.core_installing {
                    self.rvc_import_note = "Дождитесь установки основного runtime".into();
                } else if !self.rvc_runtime_installed && !self.rvc_runtime_installing {
                    self.rvc_runtime_installing = true;
                    self.rvc_import_note = "Подготовка загрузки RVC runtime…".into();
                    let root = self.component_root.clone();
                    return Task::perform(
                        async move { components::install_rvc(&root) },
                        Msg::RvcInstalled,
                    );
                }
            }
            Msg::RvcInstalled(result) => {
                self.rvc_runtime_installing = false;
                match result {
                    Ok(version) => {
                        self.rvc_runtime_installed = true;
                        self.rvc_import_note =
                            format!("RVC runtime {version} установлен. Теперь импортируйте голос.");
                    }
                    Err(error) => self.rvc_import_note = format!("RVC runtime: {error}"),
                }
            }
            Msg::CoreInstalled(result) => {
                self.core_installing = false;
                match result {
                    Ok(version) => {
                        self.runtime_root = self.component_root.clone();
                        unsafe { std::env::set_var("MNR_RUNTIME_ROOT", &self.runtime_root) };
                        self.rvc_runtime_installed = components::rvc_installed(&self.runtime_root);
                        self.message = format!("Основной runtime {version} установлен");
                        self.engine.refresh();
                    }
                    Err(error) => self.message = format!("Основной runtime: {error}"),
                }
            }
            Msg::RvcModel(model) => {
                self.controls.rvc_options.slot = model.slot;
                self.rvc_name = model.name.clone();
                self.rvc_delete_confirm = false;
                if !model.has_index {
                    self.controls.rvc_options.index = 0;
                }
                self.focus = focus::rvc::MODEL;
                self.changed();
            }
            Msg::RvcPitch(v) => {
                self.controls.rvc_options.pitch = v.round().clamp(-24.0, 24.0) as i32;
                self.focus = focus::rvc::PITCH;
                self.changed();
            }
            Msg::RvcIndex(v) => {
                self.controls.rvc_options.index = v.round().clamp(0.0, 100.0) as u32;
                self.focus = focus::rvc::INDEX;
                self.changed();
            }
            Msg::RvcGain(v) => {
                self.controls.rvc_options.gain = v.round().clamp(50.0, 300.0) as u32;
                self.focus = focus::rvc::GAIN;
                self.changed();
            }
            Msg::RvcChunk(v) => {
                if rvc::CHUNKS.contains(&v) {
                    self.controls.rvc_options.chunk = v;
                    self.changed();
                }
                self.focus = focus::rvc::CHUNK;
            }
            Msg::RvcAdvanced => {
                self.rvc_advanced = !self.rvc_advanced;
                self.focus = focus::rvc::ADVANCED;
            }
            Msg::RvcRefresh => {
                match rvc::models(&self.runtime_root) {
                    Ok(models) => {
                        self.rvc_models = models;
                        self.rvc_name = self
                            .rvc_models
                            .iter()
                            .find(|model| model.slot == self.controls.rvc_options.slot)
                            .map(|model| model.name.clone())
                            .unwrap_or_default();
                        self.rvc_delete_confirm = false;
                    }
                    Err(e) => self.message = format!("Модели RVC: {e}"),
                }
                self.focus = focus::rvc::REFRESH;
            }
            Msg::RvcImport => {
                if self.rvc_importing {
                    return Task::none();
                }
                self.rvc_importing = true;
                self.focus = focus::rvc::IMPORT;
                self.message.clear();
                self.rvc_import_note =
                    "Выберите .pth и, при наличии, его .index через Ctrl + щелчок".into();
                let root = self.runtime_root.clone();
                return Task::perform(async move { rvc::import(&root) }, Msg::RvcImported);
            }
            Msg::RvcImported(result) => {
                self.rvc_importing = false;
                match result {
                    Ok(Some((models, slot))) => {
                        self.rvc_models = models;
                        if !self.controls.rvc {
                            self.controls.rvc_options.slot = slot;
                            self.controls.rvc_options.index = 0;
                            self.changed();
                        }
                        self.rvc_name = self
                            .rvc_models
                            .iter()
                            .find(|model| model.slot == self.controls.rvc_options.slot)
                            .map(|model| model.name.clone())
                            .unwrap_or_default();
                        self.rvc_import_note = if self.controls.rvc {
                            "Модель импортирована и доступна в списке".into()
                        } else {
                            "Модель импортирована и выбрана".into()
                        };
                    }
                    Ok(None) => self.rvc_import_note.clear(),
                    Err(e) => {
                        self.rvc_import_note.clear();
                        self.message = format!("Импорт модели: {e}");
                    }
                }
            }
            Msg::RvcName(name) => {
                self.rvc_name = name.chars().take(80).collect();
                self.rvc_delete_confirm = false;
                self.focus = focus::rvc::NAME;
            }
            Msg::RvcRename => {
                self.focus = focus::rvc::RENAME;
                if !self.rvc_can_manage() {
                    self.message = "Выключите RVC и дождитесь остановки модели".into();
                } else {
                    let slot = self.controls.rvc_options.slot;
                    match rvc::rename(&self.runtime_root, slot, &self.rvc_name) {
                        Ok(name) => {
                            if let Some(model) =
                                self.rvc_models.iter_mut().find(|model| model.slot == slot)
                            {
                                model.name = name.clone();
                            }
                            self.rvc_name = name;
                            self.rvc_import_note = "Модель переименована".into();
                            self.message.clear();
                        }
                        Err(e) => self.message = format!("Переименование модели: {e}"),
                    }
                }
            }
            Msg::RvcDelete => {
                self.focus = focus::rvc::DELETE;
                if !self.rvc_can_manage() {
                    self.message = "Выключите RVC и дождитесь остановки модели".into();
                } else if !self.rvc_delete_confirm {
                    self.rvc_delete_confirm = true;
                    self.rvc_import_note = "Нажмите «Удалить ещё раз» для подтверждения".into();
                } else {
                    let slot = self.controls.rvc_options.slot;
                    match rvc::delete(&self.runtime_root, slot) {
                        Ok(()) => {
                            self.rvc_models.retain(|model| model.slot != slot);
                            if let Some(model) = self.rvc_models.first() {
                                self.controls.rvc_options.slot = model.slot;
                                if !model.has_index {
                                    self.controls.rvc_options.index = 0;
                                }
                                self.rvc_name = model.name.clone();
                            } else {
                                self.controls.rvc_options.slot = 0;
                                self.controls.rvc_options.index = 0;
                                self.rvc_name.clear();
                            }
                            self.rvc_delete_confirm = false;
                            self.rvc_import_note = "Модель удалена".into();
                            self.message.clear();
                            self.changed();
                        }
                        Err(e) => self.message = format!("Удаление модели: {e}"),
                    }
                }
            }
            Msg::CancelPhrase => {
                self.engine.cancel_phrase();
                self.focus = focus::effects::CANCEL_PHRASE;
            }
            Msg::Bind(i) => {
                self.binding = Some(i);
                self.candidate = 0;
                self.focus = focus::bind::ACCEPT;
                self.engine.capture(true);
            }
            Msg::CancelBind => {
                self.binding = None;
                self.engine.capture(false);
                self.focus = focus::NONE;
            }
            Msg::ClearBind => {
                if let Some(i) = self.binding {
                    self.keys[i] = 0;
                    self.engine.bindings(self.keys);
                    self.changed();
                }
                return self.update(Msg::CancelBind);
            }
            Msg::AcceptBind => {
                if let Some(i) = self.binding
                    && self.candidate != 0
                    && !self
                        .keys
                        .iter()
                        .enumerate()
                        .any(|(j, &k)| j != i && k == self.candidate)
                {
                    self.keys[i] = self.candidate;
                    self.engine.bindings(self.keys);
                    self.changed();
                    return self.update(Msg::CancelBind);
                }
            }
            Msg::Key(key, mods, repeat) => return self.key(key, mods, repeat),
            Msg::Screenshot(shot) => {
                if let Some(path) = &self.capture_path {
                    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                        let file = std::fs::File::create(path)?;
                        let mut encoder =
                            png::Encoder::new(file, shot.size.width, shot.size.height);
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder.write_header()?.write_image_data(&shot.rgba)?;
                        Ok(())
                    })();
                    if let Err(e) = result {
                        self.message = e.to_string();
                    } else {
                        return self.update(Msg::Quit);
                    }
                }
            }
            Msg::Noop => {}
        }
        Task::none()
    }
    fn rvc_has_index(&self) -> bool {
        self.rvc_models
            .iter()
            .any(|m| m.slot == self.controls.rvc_options.slot && m.has_index)
    }
    fn rvc_can_manage(&self) -> bool {
        !self.controls.rvc
            && self.snapshot.rvc_state == 0
            && self
                .rvc_models
                .iter()
                .any(|model| model.slot == self.controls.rvc_options.slot)
    }
    fn key(&mut self, key: keyboard::Key, mods: keyboard::Modifiers, repeat: bool) -> Task<Msg> {
        use keyboard::{Key, key::Named};
        if self.window.is_none() {
            return Task::none();
        }
        if self.binding.is_some() && self.candidate == 0 {
            if key == Key::Named(Named::Escape) {
                return self.update(Msg::CancelBind);
            }
            return Task::none();
        }
        if key == Key::Named(Named::Tab) {
            // Visual order of the page tabs: microphone, headphones, voice changer, settings.
            let tabs = [
                focus::TAB_BASE,
                focus::TAB_BASE + 3,
                focus::TAB_BASE + 1,
                focus::TAB_BASE + 2,
            ];
            let order = if self.binding.is_some() {
                use focus::bind::*;
                vec![ACCEPT, CLEAR, CANCEL]
            } else if self.headphone_page {
                use focus::headphones::*;
                let mut items = vec![OUTPUT, TOGGLE, MUTE, NOISE, INTENSITY, VOLUME, PITCH];
                items.extend(tabs);
                items
            } else if self.details {
                use focus::settings::*;
                let mut items = if self.running() || self.busy {
                    vec![AUTOSTART, UPDATE, QUIT, DONE]
                } else {
                    vec![
                        INPUT, OUTPUT, VERSION, BUFFER, AUTOSTART, REFRESH, UPDATE, QUIT, DONE,
                    ]
                };
                if self.update_ready {
                    items.insert(items.len() - 2, APPLY_UPDATE);
                }
                items.extend(tabs);
                items
            } else if self.rvc_page {
                use focus::rvc::*;
                let mut items = if self.rvc_runtime_installed {
                    vec![ENABLE, MODEL]
                } else {
                    vec![INSTALL]
                };
                if !self.rvc_importing {
                    items.push(IMPORT);
                }
                if self.rvc_can_manage() {
                    items.extend([NAME, RENAME, DELETE]);
                }
                items.push(PITCH);
                items.push(ADVANCED);
                if self.rvc_advanced {
                    if self.rvc_has_index() {
                        items.push(INDEX);
                    }
                    items.extend([GAIN, CHUNK, REFRESH]);
                }
                items.extend(tabs);
                items
            } else {
                use focus::effects::*;
                let discord = |effect| DISCORD_BIND_BASE + effect;
                let mut items = vec![
                    INPUT,
                    INTENSITY,
                    NOISE_BIND,
                    ALT_INTENSITY,
                    OVERLOAD,
                    BOOST,
                    BOOST_BIND,
                    discord(0),
                    PITCH,
                    PITCH_BIND,
                    discord(1),
                    SLOW,
                    SLOW_BIND,
                    discord(2),
                    FAST,
                    FAST_BIND,
                    discord(3),
                    REVERSE_BIND,
                    discord(4),
                ];
                if self.phrase_state != 0 {
                    items.push(CANCEL_PHRASE);
                }
                items.extend([
                    MONITOR,
                    MONITOR_BIND,
                    REPLAY_BIND,
                    DISCORD_VOLUME,
                    EFFECTS_MONITOR,
                    BOOST_MONITOR,
                ]);
                items.extend(tabs);
                items
            };
            self.focus = match order.iter().position(|&v| v == self.focus) {
                Some(i) => {
                    order[(i + if mods.shift() { order.len() - 1 } else { 1 }) % order.len()]
                }
                None => {
                    if mods.shift() {
                        *order.last().unwrap()
                    } else {
                        order[0]
                    }
                }
            };
            return Task::batch([
                view::reveal_focus(),
                iced::widget::operation::focus(if self.focus == focus::rvc::NAME {
                    "rvc-name"
                } else {
                    "no-text-input"
                }),
            ]);
        }
        if key == Key::Named(Named::Escape) {
            if self.binding.is_some() {
                return self.update(Msg::CancelBind);
            }
            if self.details || self.rvc_page {
                return self.update(Msg::Page(0));
            }
        }
        let activate = matches!(key, Key::Named(Named::Enter | Named::Space)) && !repeat;
        if activate
            && self.binding.is_none()
            && (focus::TAB_BASE..focus::TAB_BASE + 4).contains(&self.focus)
        {
            return self.update(Msg::Page((self.focus - focus::TAB_BASE) as u8));
        }
        let delta = match key {
            Key::Named(Named::ArrowLeft | Named::ArrowDown) => -1,
            Key::Named(Named::ArrowRight | Named::ArrowUp) => 1,
            _ => 0,
        };
        let message = if self.binding.is_some() {
            if activate {
                match self.focus {
                    focus::bind::ACCEPT => Msg::AcceptBind,
                    focus::bind::CLEAR => Msg::ClearBind,
                    _ => Msg::CancelBind,
                }
            } else {
                Msg::Noop
            }
        } else if self.headphone_page {
            use focus::headphones::*;
            match self.focus {
                OUTPUT if delta != 0 || activate => {
                    let devices = self.headphone_outputs();
                    if devices.is_empty() {
                        Msg::Noop
                    } else {
                        let i = self
                            .headphone_output
                            .as_ref()
                            .and_then(|d| devices.iter().position(|v| v == d))
                            .unwrap_or(0);
                        Msg::HeadphoneOutput(
                            devices[(i as i32 + if delta == 0 { 1 } else { delta })
                                .rem_euclid(devices.len() as i32)
                                as usize]
                                .clone(),
                        )
                    }
                }
                TOGGLE if activate => Msg::HeadphoneToggle,
                NOISE if activate => Msg::HeadphoneNoise(!self.headphone_denoise),
                INTENSITY if delta != 0 => {
                    Msg::HeadphoneIntensity(self.headphone_intensity * 100.0 + delta as f32)
                }
                VOLUME if delta != 0 => {
                    Msg::HeadphoneVolume(self.headphone_volume * 100.0 + delta as f32)
                }
                PITCH if delta != 0 => {
                    Msg::HeadphonePitch(self.headphone_pitch as f32 + delta as f32)
                }
                MUTE if activate => Msg::HeadphoneMute,
                _ => Msg::Noop,
            }
        } else if self.details {
            use focus::settings::*;
            match self.focus {
                INPUT | OUTPUT if delta != 0 || activate => {
                    let items = if self.focus == INPUT {
                        &self.inputs
                    } else {
                        &self.outputs
                    };
                    let selected = if self.focus == INPUT {
                        &self.input
                    } else {
                        &self.output
                    };
                    if items.is_empty() {
                        Msg::Noop
                    } else {
                        let i = selected
                            .as_ref()
                            .and_then(|d| items.iter().position(|x| x == d))
                            .unwrap_or(0);
                        let d = items[(i as i32 + if delta == 0 { 1 } else { delta })
                            .rem_euclid(items.len() as i32)
                            as usize]
                            .clone();
                        if self.focus == INPUT {
                            Msg::Input(d)
                        } else {
                            Msg::Output(d)
                        }
                    }
                }
                VERSION if delta != 0 || activate => {
                    Msg::Version(if self.version == 1 { 2 } else { 1 })
                }
                BUFFER if delta != 0 || activate => {
                    let values = [10, 20, 30, 40, 60, 80];
                    let i = values.iter().position(|&v| v == self.buffer).unwrap_or(3);
                    Msg::Buffer(
                        values[(i as i32 + if delta == 0 { 1 } else { delta }).rem_euclid(6)
                            as usize],
                    )
                }
                REFRESH if activate => Msg::Refresh,
                UPDATE if activate => Msg::UpdateCheck,
                APPLY_UPDATE if activate => Msg::ApplyUpdate,
                DONE if activate => Msg::Settings,
                QUIT if activate => Msg::Quit,
                AUTOSTART if activate => Msg::Autostart(!self.autostart),
                _ => Msg::Noop,
            }
        } else {
            use focus::effects::*;
            match self.focus {
                INPUT if (delta != 0 || activate) && !self.inputs.is_empty() => {
                    let i = self
                        .input
                        .as_ref()
                        .and_then(|d| self.inputs.iter().position(|v| v == d))
                        .unwrap_or(0);
                    Msg::Input(
                        self.inputs[(i as i32 + if delta == 0 { 1 } else { delta })
                            .rem_euclid(self.inputs.len() as i32)
                            as usize]
                            .clone(),
                    )
                }
                MONITOR if activate => Msg::Monitor,
                EFFECTS_MONITOR if activate => Msg::EffectsMonitor(!self.effects_monitor),
                BOOST_MONITOR if activate => Msg::BoostMonitor(!self.boost_monitor),
                MONITOR_BIND if activate => Msg::Bind(10),
                REPLAY_BIND if activate => Msg::Bind(11),
                focus::rvc::INSTALL if activate => Msg::RvcInstall,
                focus::rvc::ENABLE if activate => Msg::Rvc(!self.controls.rvc),
                focus::rvc::MODEL if (delta != 0 || activate) && !self.rvc_models.is_empty() => {
                    let i = self
                        .rvc_models
                        .iter()
                        .position(|m| m.slot == self.controls.rvc_options.slot)
                        .unwrap_or(0);
                    Msg::RvcModel(
                        self.rvc_models[(i as i32 + if delta == 0 { 1 } else { delta })
                            .rem_euclid(self.rvc_models.len() as i32)
                            as usize]
                            .clone(),
                    )
                }
                focus::rvc::PITCH if delta != 0 => {
                    Msg::RvcPitch((self.controls.rvc_options.pitch + delta) as f32)
                }
                focus::rvc::INDEX if delta != 0 && self.rvc_has_index() => {
                    Msg::RvcIndex(self.controls.rvc_options.index as f32 + delta as f32)
                }
                focus::rvc::GAIN if delta != 0 => {
                    Msg::RvcGain(self.controls.rvc_options.gain as f32 + delta as f32 * 5.0)
                }
                focus::rvc::CHUNK if delta != 0 || activate => {
                    let i = rvc::CHUNKS
                        .iter()
                        .position(|&v| v == self.controls.rvc_options.chunk)
                        .unwrap_or(2);
                    Msg::RvcChunk(
                        rvc::CHUNKS[(i as i32 + if delta == 0 { 1 } else { delta }).rem_euclid(5)
                            as usize],
                    )
                }
                focus::rvc::REFRESH if activate => Msg::RvcRefresh,
                focus::rvc::IMPORT if activate => Msg::RvcImport,
                focus::rvc::RENAME if activate => Msg::RvcRename,
                focus::rvc::DELETE if activate => Msg::RvcDelete,
                focus::rvc::ADVANCED if activate => Msg::RvcAdvanced,
                SLOW if delta != 0 => {
                    Msg::Slow((self.controls.slow * 100.0 + delta as f32 * 5.0).clamp(50.0, 95.0))
                }
                SLOW_BIND if activate => Msg::Bind(2),
                FAST if delta != 0 => {
                    Msg::Fast((self.controls.fast * 100.0 + delta as f32 * 5.0).clamp(105.0, 200.0))
                }
                FAST_BIND if activate => Msg::Bind(3),
                CANCEL_PHRASE if activate => Msg::CancelPhrase,
                REVERSE_BIND if activate => Msg::Bind(4),
                f if activate && (DISCORD_BIND_BASE..DISCORD_BIND_BASE + 5).contains(&f) => {
                    Msg::Bind(5 + f - DISCORD_BIND_BASE)
                }
                OVERLOAD if activate => Msg::Overload(!self.controls.overload),
                DISCORD_VOLUME if delta != 0 => Msg::DiscordVolume(
                    (self.controls.discord_volume * 100.0 + delta as f32).clamp(0.0, 100.0),
                ),
                INTENSITY if delta != 0 => Msg::Intensity(
                    (self.controls.intensity * 100.0 + delta as f32).clamp(0.0, 200.0),
                ),
                ALT_INTENSITY if delta != 0 => Msg::AlternateIntensity(
                    (self.controls.alternate_intensity * 100.0 + delta as f32).clamp(0.0, 200.0),
                ),
                NOISE_BIND if activate => Msg::Bind(12),
                BOOST if delta != 0 => Msg::Boost(
                    (self.controls.boost * 100.0 + delta as f32 * 10.0).clamp(100.0, 2000.0),
                ),
                BOOST_BIND if activate => Msg::Bind(0),
                PITCH if delta != 0 => {
                    Msg::Pitch((self.controls.pitch + delta).clamp(-12, 12) as f32)
                }
                PITCH_BIND if activate => Msg::Bind(1),
                _ => Msg::Noop,
            }
        };
        self.update(message)
    }
    fn subscription(&self) -> Subscription<Msg> {
        Subscription::batch([
            window::close_requests().map(|_| Msg::Hide),
            iced::event::listen_with(|event, _, id| {
                match event {
                    iced::Event::Window(window::Event::Focused) => {
                        return Some(Msg::WindowFocus(id, true));
                    }
                    iced::Event::Window(window::Event::Unfocused) => {
                        return Some(Msg::WindowFocus(id, false));
                    }
                    _ => {}
                }
                if let iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    repeat,
                    ..
                }) = event
                {
                    Some(Msg::Key(key, modifiers, repeat))
                } else {
                    None
                }
            }),
        ])
    }
}
fn main() {
    velopack::VelopackApp::build().run();
    let root = paths::Paths::resolve().ok().map(|p| p.data);
    let mut result = (|| -> Result<(), String> {
        let Some((app, task)) = App::new()? else {
            return Ok(());
        };
        let state = std::cell::RefCell::new(Some((app, task)));
        iced::daemon(
            move || state.borrow_mut().take().expect("boot once"),
            App::update,
            App::view,
        )
        .title("Mic Noize")
        .theme(|_: &App, _: window::Id| {
            Theme::custom(
                "Graphite",
                iced::theme::Palette {
                    background: view::BG,
                    text: view::INK,
                    primary: view::ORANGE,
                    success: view::GREEN,
                    danger: view::RED,
                    warning: view::ORANGE,
                },
            )
        })
        .default_font(Font::with_name("Segoe UI"))
        .subscription(App::subscription)
        .scale_factor(|s: &App, _| s.qa_scale)
        .run()
        .map_err(|e| e.to_string())
    })();
    if result.is_ok() && RESTART.swap(false, Ordering::Relaxed) {
        result = std::env::current_exe()
            .and_then(|exe| Command::new(exe).spawn())
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    if let Err(e) = result {
        if let Some(root) = root {
            let _ = std::fs::create_dir_all(root.join("Logs"));
            let _ = std::fs::write(root.join("Logs/rust-ui-error.log"), &e);
        }
        eprintln!("{e}");
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;
    #[test]
    fn noise_presets_and_hotkey_roundtrip() {
        use keyboard::{Key, Modifiers, key::Named};
        let (mut app, _) = App::from_settings(Settings::for_test("[audio]\nintensity=105"))
            .unwrap()
            .unwrap();
        assert_eq!(app.controls.intensity, 1.05);
        assert_eq!(app.controls.alternate_intensity, 0.15);
        assert_eq!(app.keys[12], 0);
        app.window = Some(App::open(1.0).0);
        app.focus = 37;
        let _ = app.key(Key::Named(Named::ArrowRight), Modifiers::empty(), false);
        assert_eq!(app.controls.alternate_intensity, 0.16);
        assert_eq!(app.controls.intensity, 1.05);
        let _ = app.update(Msg::AlternateIntensity(250.0));
        assert_eq!(app.controls.alternate_intensity, 2.0);
        let _ = app.update(Msg::AlternateIntensity(-1.0));
        assert_eq!(app.controls.alternate_intensity, 0.0);
        let _ = app.update(Msg::AlternateIntensity(15.0));
        app.focus = 38;
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert_eq!(app.binding, Some(12));
        app.candidate = app.keys[11];
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.binding, Some(12));
        app.candidate = 120 | 256;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[12], 120 | 256);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".tmp/noise-preset-test");
        std::fs::create_dir_all(&dir).unwrap();
        app.settings.path = dir.join("settings.ini");
        app.benchmark = false;
        app.save();
        let mut saved = false;
        for _ in 0..100 {
            if let Some(Reply::Saved(result)) = app.engine.reply() {
                result.unwrap();
                saved = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(saved, "Settings write did not finish");
        let (restored, _) = App::from_settings(Settings::load(&dir).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(restored.controls.intensity, 1.05);
        assert_eq!(restored.controls.alternate_intensity, 0.15);
        assert_eq!(restored.keys[12], 120 | 256);
        std::fs::remove_file(dir.join("settings.ini")).unwrap();
    }
    #[test]
    fn background_window_stops_meter_updates_and_restores_on_focus() {
        let (mut app, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        let id = App::open(1.0).0;
        app.window = Some(id);
        assert!(!app.ui_active());
        let _ = app.update(Msg::WindowFocus(id, true));
        assert!(app.ui_active());
        let _ = app.update(Msg::WindowFocus(id, false));
        assert!(!app.ui_active());
        app.peak = 0.75;
        let _ = app.update(Msg::Tick);
        assert_eq!(app.peak, 0.75); // No meter animation behind another app.
        let _ = app.update(Msg::WindowFocus(id, true));
        let _ = app.update(Msg::Tick);
        assert!((app.peak - 0.6).abs() < 0.0001);
        let _ = app.update(Msg::Minimize);
        assert!(!app.ui_active());
        let _ = app.update(Msg::WindowFocus(id, true));
        assert!(app.ui_active());
        app.tray_ok = true;
        app.hint_shown = true;
        let _ = app.update(Msg::Hide);
        let _ = app.update(Msg::WindowFocus(id, true));
        assert!(!app.ui_active()); // Late focus event cannot wake the hidden UI.
        let _ = app.update(Msg::Show);
        assert!(!app.ui_active());
        let _ = app.update(Msg::WindowFocus(id, true));
        assert!(app.ui_active());
    }
    #[test]
    fn boost_monitor_is_independent_and_defaults_off() {
        use keyboard::{Key, Modifiers, key::Named};
        let (mut app, _) = App::from_settings(Settings::for_test("[effects]\nmonitor_effects=1"))
            .unwrap()
            .unwrap();
        assert!(app.effects_monitor && !app.boost_monitor);
        assert_eq!(app.monitor_mode(), 2);
        app.window = Some(App::open(1.0).0);
        app.focus = 31;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 36);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert_eq!(app.monitor_mode(), 4);
        let _ = app.update(Msg::EffectsMonitor(false));
        assert!(app.boost_monitor);
        assert_eq!(app.monitor_mode(), 3);
        app.monitor_all = true;
        assert_eq!(app.monitor_mode(), 1);
        app.monitor_all = false;
        let _ = app.update(Msg::BoostMonitor(false));
        assert_eq!(app.monitor_mode(), 0);
        let _ = app.update(Msg::EffectsMonitor(true));
        assert!(!app.boost_monitor);
        let (saved, _) = App::from_settings(Settings::for_test(
            "[effects]\nmonitor_effects=0\nmonitor_boost=1",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(saved.monitor_mode(), 3);
    }
    #[test]
    fn route_device_selection_is_validated_and_keyboard_accessible() {
        use keyboard::{Key, Modifiers, key::Named};
        let (mut app, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        let mic = Device {
            id: "test-mic".into(),
            name: "Test microphone".into(),
        };
        let headphones = Device {
            id: "test-out".into(),
            name: "Headphones".into(),
        };
        let virtual_out = Device {
            id: "TAG".into(),
            name: "Thin Audio Gateway".into(),
        };
        app.inputs = vec![mic.clone()];
        app.outputs = vec![headphones.clone(), virtual_out.clone()];
        app.window = Some(App::open(1.0).0);
        app.focus = 35;
        let _ = app.key(Key::Named(Named::Enter), Modifiers::empty(), false);
        assert_eq!(app.input, Some(mic.clone()));
        assert_eq!(app.focus, 35);
        let _ = app.update(Msg::Input(headphones.clone()));
        assert_eq!(app.input, Some(mic));
        let _ = app.update(Msg::Page(3));
        app.focus = 50;
        let _ = app.key(Key::Named(Named::Enter), Modifiers::empty(), false);
        assert_eq!(app.headphone_output, Some(headphones.clone()));
        let _ = app.update(Msg::HeadphoneOutput(virtual_out));
        assert_eq!(app.headphone_output, Some(headphones));
        app.headphone_busy = true;
        app.headphone_output = None;
        let _ = app.key(Key::Named(Named::Enter), Modifiers::empty(), false);
        assert!(app.headphone_output.is_none());
        app.busy = true;
        app.input = None;
        let _ = app.update(Msg::Input(app.inputs[0].clone()));
        assert!(app.input.is_none());
    }
    #[test]
    fn headphones_are_off_and_independent() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let (mut app, _) = App::from_settings(Settings::for_test(
            "[headphones]\nvolume=65\npitch=2\nintensity=175",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(app.headphone_state, 0);
        assert_eq!(app.headphone_intensity, 1.75);
        assert!(!app.headphone_busy);
        assert_eq!(app.engine.headphone_state().0, 0);
        let microphone = app.controls.intensity;
        let _ = app.update(Msg::Page(3));
        assert!(app.headphone_page && !app.details && !app.rvc_page);
        let _ = app.update(Msg::HeadphoneIntensity(30.0));
        assert_eq!(app.headphone_intensity, 0.3);
        let _ = app.update(Msg::HeadphoneIntensity(250.0));
        assert_eq!(app.headphone_intensity, 2.0);
        assert_eq!(app.controls.intensity, microphone);
        let _ = app.update(Msg::HeadphoneMute);
        assert!(app.headphone_muted && !app.controls.muted);
        app.window = Some(App::open(1.0).0);
        app.focus = 55;
        let _ = app.key(Key::Named(Named::ArrowRight), Modifiers::empty(), false);
        assert_eq!(app.headphone_pitch, 3);
        app.headphone_state = 2;
        let _ = app.update(Msg::HeadphoneNoise(false));
        assert!(app.headphone_denoise);
    }
    #[test]
    fn restart_event_does_not_override_session_exit() {
        assert!(restart_requested(RESTART_EVENT));
        assert!(!restart_requested(EXIT_EVENT));
        assert!(!restart_requested(EXIT_EVENT | RESTART_EVENT));
    }

    #[test]
    fn keyboard_monitor_and_independent_bindings() {
        assert_eq!(std::mem::size_of::<Snapshot>(), 64);
        let (mut app, _) = App::from_settings(Settings::for_test(
            "[effects]\nboost_key=256\npitch_key=119\nslow_key=119\nfast_key=2047",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(&app.keys[..4], &[0, 119, 0, 0]);
        assert_eq!(app.keys[11], 119 | 256);
        assert!(
            app.engine.snapshot(false).0.epoch > 0,
            "Native bindings rejected the sanitized set"
        );
        assert!(app.window.is_none());
        assert!(app.benchmark);
        // A second controller must not contend for the live application's mutex.
        let (other, _) = App::from_settings(Settings::for_test("")).unwrap().unwrap();
        drop(other);
        // Keyboard handlers require a window ID; do not execute the window-open task.
        app.window = Some(App::open(1.0).0);
        app.keys = [0; 13];
        use keyboard::{Key, Modifiers, key::Named};
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 35);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 34);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 38);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 37);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 21);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 2);
        app.focus = 20;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 9);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert_eq!(app.engine.monitor_state().0, 0); // Disabled while stopped.
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 23);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert_eq!(app.binding, Some(10));
        app.candidate = 200;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[10], 200);
        app.focus = 23;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 32);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert_eq!(app.binding, Some(11));
        app.candidate = 119 | 256;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[11], 119 | 256);
        app.focus = 22;
        app.controls.discord_volume = 0.5;
        let _ = app.key(Key::Named(Named::ArrowLeft), Modifiers::empty(), false);
        assert!((app.controls.discord_volume - 0.49).abs() < 0.0001);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 31);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 36);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 40);
        let _ = app.update(Msg::Page(1));
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 24);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 25);
        app.rvc_models = vec![
            rvc::Model {
                slot: 0,
                name: "First".into(),
                has_index: false,
            },
            rvc::Model {
                slot: 7,
                name: "Second".into(),
                has_index: true,
            },
        ];
        app.controls.rvc_options.slot = 0;
        let _ = app.key(Key::Named(Named::ArrowRight), Modifiers::empty(), false);
        assert_eq!(app.controls.rvc_options.slot, 7);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 39);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 44);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 45);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 46);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 26);
        app.rvc_importing = true;
        let _ = app.update(Msg::RvcImported(Ok(None)));
        assert!(!app.rvc_importing);
        let models = app.rvc_models.clone();
        app.controls.rvc = false;
        let _ = app.update(Msg::RvcImported(Ok(Some((models.clone(), 0)))));
        assert_eq!(app.controls.rvc_options.slot, 0);
        app.controls.rvc = true;
        let _ = app.update(Msg::RvcImported(Ok(Some((models, 7)))));
        assert_eq!(app.controls.rvc_options.slot, 0); // Preserve the active voice.
        app.controls.rvc = false;
        let _ = app.update(Msg::RvcImported(Err("test error".into())));
        assert!(app.message.contains("test error"));
        let _ = app.update(Msg::RvcPitch(100.0));
        assert_eq!(app.controls.rvc_options.pitch, 24);
        let _ = app.update(Msg::RvcIndex(-10.0));
        assert_eq!(app.controls.rvc_options.index, 0);
        let _ = app.update(Msg::RvcGain(999.0));
        assert_eq!(app.controls.rvc_options.gain, 300);
        let _ = app.update(Msg::RvcChunk(150));
        let _ = app.update(Msg::RvcChunk(123));
        assert_eq!(app.controls.rvc_options.chunk, 150);
        let options = app.controls.rvc_options;
        options.save(&mut app.settings);
        assert_eq!(rvc::Options::load(&app.settings), options);
        let _ = app.update(Msg::RvcAdvanced);
        app.focus = 30;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 40);
        let _ = app.key(Key::Named(Named::Enter), Modifiers::empty(), false);
        assert!(!app.rvc_page && !app.details);
        let _ = app.update(Msg::Page(1));
        app.controls.rvc_options.slot = 0;
        app.focus = 26;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 33);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 28); // No index control for this model.
        let _ = app.key(Key::Named(Named::Escape), Modifiers::empty(), false);
        assert!(!app.rvc_page && !app.details);
        let _ = app.update(Msg::Bind(0));
        app.candidate = 119;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[0], 119);
        assert!(app.binding.is_none());
        let _ = app.update(Msg::Bind(1));
        app.candidate = 119;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.binding, Some(1));
        assert_eq!(app.keys[1], 0);
        app.candidate = 120;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys, [119, 120, 0, 0, 0, 0, 0, 0, 0, 0, 200, 375, 0]);
        let _ = app.update(Msg::Bind(0));
        let _ = app.update(Msg::ClearBind);
        assert_eq!(app.keys, [0, 120, 0, 0, 0, 0, 0, 0, 0, 0, 200, 375, 0]);
        let _ = app.update(Msg::Bind(2));
        app.candidate = 120;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.binding, Some(2));
        app.candidate = 121;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[2], 121);
        let _ = app.update(Msg::Bind(3));
        app.candidate = 121;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.binding, Some(3));
        app.candidate = 122;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[3], 122);
        app.focus = 13;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 19);
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 15);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert_eq!(app.binding, Some(4));
        app.candidate = 122;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.binding, Some(4));
        app.candidate = 123;
        let _ = app.update(Msg::AcceptBind);
        assert_eq!(app.keys[4], 123);
        for i in 5..10 {
            app.focus = 11 + i;
            let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
            assert_eq!(app.binding, Some(i));
            app.candidate = 120;
            let _ = app.update(Msg::AcceptBind);
            assert_eq!(app.binding, Some(i));
            app.candidate = 124 + i as u32;
            let _ = app.update(Msg::AcceptBind);
            assert_eq!(app.keys[i], 124 + i as u32);
        }
        app.details = false;
        let _ = app.update(Msg::Intensity(200.0));
        let _ = app.key(Key::Named(Named::ArrowRight), Modifiers::empty(), false);
        assert_eq!(app.controls.intensity, 2.0);
        let _ = app.key(Key::Named(Named::ArrowLeft), Modifiers::empty(), false);
        assert!((app.controls.intensity - 1.99).abs() < 0.0001);
        app.details = false;
        app.controls.overload = false;
        app.focus = 37;
        let _ = app.key(Key::Named(Named::Tab), Modifiers::empty(), false);
        assert_eq!(app.focus, 21);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert!(app.controls.overload);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), true);
        assert!(app.controls.overload);
        let _ = app.key(Key::Named(Named::Space), Modifiers::empty(), false);
        assert!(!app.controls.overload);
    }
}

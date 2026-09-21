use std::path::{Path, PathBuf};

const APP_DIR: &str = "MicNoiseReducer";

pub struct Paths {
    pub app: PathBuf,
    pub data: PathBuf,
    pub components: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let app = exe
            .parent()
            .ok_or("Executable has no parent directory")?
            .to_path_buf();
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("LOCALAPPDATA is not set")?;
        let data = local.join(APP_DIR);
        let components = data.join("Components");
        std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;

        // One-time migration from the old repository-local layout.
        let legacy = app
            .parent()
            .filter(|_| app.file_name().is_some_and(|n| n == "bin"));
        let settings = data.join("settings.ini");
        if !settings.exists()
            && let Some(old) = legacy.map(|p| p.join("settings.ini"))
            && old.is_file()
        {
            std::fs::copy(old, &settings).map_err(|e| e.to_string())?;
        }
        Ok(Self {
            app,
            data,
            components,
        })
    }

    pub fn runtime_root(&self) -> &Path {
        if self.components.join("vendor").is_dir() {
            &self.components
        } else {
            self.app.parent().unwrap_or(&self.app)
        }
    }
}

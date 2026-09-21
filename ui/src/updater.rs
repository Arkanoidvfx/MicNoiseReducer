use velopack::{UpdateCheck, UpdateManager, sources::GithubSource};

const REPOSITORY: &str = "https://github.com/Arkanoidvfx/MicNoize";

#[derive(Clone, Debug)]
pub enum Status {
    Current,
    Ready(String),
    Unavailable(String),
}

fn manager() -> Result<UpdateManager, String> {
    UpdateManager::new(GithubSource::new(REPOSITORY, None, false), None, None)
        .map_err(|e| e.to_string())
}

pub fn check_and_download() -> Status {
    let manager = match manager() {
        Ok(manager) => manager,
        // Development/portable builds have no Velopack locator and must remain usable.
        Err(error) => return Status::Unavailable(error),
    };
    match manager.check_for_updates() {
        Ok(UpdateCheck::UpdateAvailable(update)) => {
            let version = update.TargetFullRelease.Version.to_string();
            manager
                .download_updates(&update, None)
                .map(|_| Status::Ready(version))
                .unwrap_or_else(|e| Status::Unavailable(e.to_string()))
        }
        Ok(_) => Status::Current,
        Err(error) => Status::Unavailable(error.to_string()),
    }
}

pub fn apply_and_restart() -> Result<(), String> {
    let manager = manager()?;
    let update = manager
        .get_update_pending_restart()
        .ok_or("Скачанное обновление не найдено")?;
    manager
        .apply_updates_and_restart(update)
        .map_err(|e| e.to_string())
}

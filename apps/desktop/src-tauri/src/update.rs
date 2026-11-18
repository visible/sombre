use std::sync::OnceLock;
use std::time::Duration;

use sombre_core::Roots;
use sombre_harness::config::Config;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

use crate::live::Live;

const PAUSE: Duration = Duration::from_secs(60 * 60);

static READY: OnceLock<String> = OnceLock::new();

pub fn start(app: AppHandle) {
    if option_env!("SOMBRE_RELEASE").is_none() {
        return;
    }
    std::thread::spawn(move || {
        loop {
            let allowed = Roots::detect()
                .and_then(|roots| Config::load(&roots.sombre).ok())
                .is_none_or(|config| config.update != Some(false));
            if allowed {
                match tauri::async_runtime::block_on(install(&app)) {
                    Ok(Some(version)) => {
                        let _ = READY.set(version.clone());
                        let _ = app.emit("updated", version);
                        return;
                    }
                    Ok(None) => {}
                    Err(error) => eprintln!("sombre couldn't update itself: {error}"),
                }
            }
            std::thread::sleep(PAUSE);
        }
    });
}

async fn install(app: &AppHandle) -> Result<Option<String>, tauri_plugin_updater::Error> {
    let Some(update) = app.updater()?.check().await? else {
        return Ok(None);
    };
    update.download_and_install(|_, _| {}, || {}).await?;
    Ok(Some(update.version))
}

#[tauri::command]
#[specta::specta]
pub fn ready() -> Option<String> {
    READY.get().cloned()
}

#[tauri::command]
#[specta::specta]
pub fn restart(app: AppHandle, live: State<'_, Live>) -> Result<(), String> {
    if live.working() {
        return Err(
            "a chat is still working. restart when it's done, or stop it first".to_string(),
        );
    }
    app.restart()
}

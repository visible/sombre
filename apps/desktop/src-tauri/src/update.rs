use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::Serialize;
use sombre_core::Roots;
use sombre_harness::config::Config;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::live::Live;

const PAUSE: Duration = Duration::from_secs(60 * 60);

static READY: OnceLock<String> = OnceLock::new();
static HELD: Mutex<Option<(Update, Vec<u8>)>> = Mutex::new(None);
static CHECKING: Mutex<()> = Mutex::new(());

#[derive(Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Checked {
    Source,
    Newest,
    Ready { version: String },
}

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
                match fetch(&app) {
                    Ok(Some(_)) => return,
                    Ok(None) => {}
                    Err(error) => eprintln!("sombre couldn't update itself: {error}"),
                }
            }
            std::thread::sleep(PAUSE);
        }
    });
}

fn fetch(app: &AppHandle) -> Result<Option<String>, tauri_plugin_updater::Error> {
    let _checking = CHECKING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(version) = READY.get() {
        return Ok(Some(version.clone()));
    }
    let found = tauri::async_runtime::block_on(install(app))?;
    if let Some(version) = &found {
        let _ = READY.set(version.clone());
        let _ = app.emit("updated", version);
    }
    Ok(found)
}

async fn install(app: &AppHandle) -> Result<Option<String>, tauri_plugin_updater::Error> {
    let Some(update) = app.updater()?.check().await? else {
        return Ok(None);
    };
    let bytes = update.download(|_, _| {}, || {}).await?;
    let version = update.version.clone();
    if cfg!(windows) {
        if let Ok(mut held) = HELD.lock() {
            *held = Some((update, bytes));
        }
    } else {
        update.install(bytes)?;
    }
    Ok(Some(version))
}

#[tauri::command]
#[specta::specta]
pub async fn check(app: AppHandle) -> Result<Checked, String> {
    if option_env!("SOMBRE_RELEASE").is_none() {
        return Ok(Checked::Source);
    }
    let found = crate::blocking(move || fetch(&app))
        .await?
        .map_err(|error| {
            eprintln!("sombre couldn't check for updates: {error}");
            "couldn't reach github to check for updates. check your connection and try again"
                .to_string()
        })?;
    Ok(found.map_or(Checked::Newest, |version| Checked::Ready { version }))
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
    let held = HELD.lock().ok().and_then(|mut held| held.take());
    live.close();
    app.state::<crate::term::Terms>().close();
    if let Some((update, bytes)) = held {
        update
            .install(bytes)
            .map_err(|error| format!("couldn't install the update: {error}"))?;
    }
    app.restart()
}

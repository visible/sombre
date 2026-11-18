use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

static LOCK: Mutex<Option<File>> = Mutex::new(None);
static STANDING: AtomicBool = AtomicBool::new(false);
static ASKED: AtomicBool = AtomicBool::new(false);

fn folder() -> Option<PathBuf> {
    sombre_core::Roots::detect().map(|roots| roots.sombre)
}

fn shared() -> Option<PathBuf> {
    folder().map(|folder| folder.join("pair.json"))
}

fn wanted() -> Option<PathBuf> {
    folder().map(|folder| folder.join("pair.want"))
}

fn opened() -> Option<File> {
    let folder = folder()?;
    fs::create_dir_all(&folder).ok()?;
    File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(folder.join("pair.lock"))
        .ok()
}

fn kept(file: Option<File>) {
    if let Ok(mut held) = LOCK.lock() {
        *held = file;
    }
}

pub fn lock() {
    let Some(file) = opened() else {
        return;
    };
    if file.try_lock().is_err() {
        if let Some(want) = wanted() {
            let _ = fs::write(&want, b"");
        }
        for _ in 0..50 {
            thread::sleep(Duration::from_millis(100));
            if file.try_lock().is_ok() {
                break;
            }
        }
        if let Some(want) = wanted() {
            let _ = fs::remove_file(want);
        }
    }
    kept(Some(file));
}

pub fn standing() -> bool {
    STANDING.load(Ordering::SeqCst)
}

pub fn asked() {
    if standing() {
        ASKED.store(true, Ordering::SeqCst);
    }
}

pub fn stand() -> Option<String> {
    if standing() {
        if ASKED.swap(false, Ordering::SeqCst) {
            return super::followed();
        }
        if wanted().is_some_and(|want| want.exists()) {
            step();
        }
        return None;
    }
    if super::live().is_some() || super::place().is_some_and(|own| own.exists()) {
        return None;
    }
    let text = shared().and_then(|path| fs::read_to_string(path).ok())?;
    if wanted().is_some_and(|want| want.exists()) {
        return None;
    }
    let file = opened()?;
    file.try_lock().ok()?;
    STANDING.store(true, Ordering::SeqCst);
    if !matches!(super::restore_from(&text), Ok(true)) {
        STANDING.store(false, Ordering::SeqCst);
        return None;
    }
    let saved: Value = serde_json::from_str(&text).unwrap_or_default();
    if let Some(chat) = saved["follow"].as_str() {
        super::follow(chat);
    }
    kept(Some(file));
    super::post(&json!({"kind": "standby"}));
    None
}

fn step() {
    super::drop_live();
    STANDING.store(false, Ordering::SeqCst);
    ASKED.store(false, Ordering::SeqCst);
    kept(None);
}

pub fn take() {
    if !standing() {
        return;
    }
    STANDING.store(false, Ordering::SeqCst);
    super::save();
    if let Some(path) = shared() {
        let _ = fs::remove_file(path);
    }
    kept(None);
}

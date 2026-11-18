use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

static LOCK: Mutex<Option<File>> = Mutex::new(None);
static STANDING: AtomicBool = AtomicBool::new(false);
static ASKED: AtomicBool = AtomicBool::new(false);

const WANT: &str = "pair.want";
const STANDBY: &str = "pair.standby";

fn folder() -> Option<PathBuf> {
    sombre_core::Roots::detect().map(|roots| roots.sombre)
}

fn shared() -> Option<PathBuf> {
    folder().map(|folder| folder.join("pair.json"))
}

fn wanted() -> Option<PathBuf> {
    folder().map(|folder| folder.join(WANT))
}

fn mark(standing: bool) {
    if let Some(beacon) = folder().map(|folder| folder.join(STANDBY)) {
        let _ = if standing {
            fs::write(beacon, b"")
        } else {
            fs::remove_file(beacon)
        };
    }
}

fn opened(folder: &Path) -> Option<File> {
    fs::create_dir_all(folder).ok()?;
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
    if let Some(folder) = folder() {
        kept(claimed(&folder));
    }
}

fn claimed(folder: &Path) -> Option<File> {
    let file = opened(folder)?;
    if file.try_lock().is_ok() {
        let _ = fs::remove_file(folder.join(STANDBY));
        return Some(file);
    }
    if folder.join(STANDBY).exists() {
        let want = folder.join(WANT);
        let _ = fs::write(&want, b"");
        for _ in 0..50 {
            thread::sleep(Duration::from_millis(100));
            if file.try_lock().is_ok() {
                break;
            }
        }
        let _ = fs::remove_file(want);
    }
    Some(file)
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
    let file = opened(&folder()?)?;
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
    mark(true);
    super::post(&json!({"kind": "standby"}));
    None
}

fn step() {
    super::drop_live();
    STANDING.store(false, Ordering::SeqCst);
    ASKED.store(false, Ordering::SeqCst);
    mark(false);
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
    mark(false);
    kept(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn folder() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"))
    }

    fn holder(place: &Path) -> File {
        let file = opened(place).unwrap_or_else(|| panic!("couldn't open the lock"));
        assert!(file.try_lock().is_ok());
        file
    }

    #[test]
    fn a_chat_starts_right_away_when_another_chat_holds_the_pairing() {
        let place = folder();
        let _other = holder(place.path());
        let started = Instant::now();
        assert!(claimed(place.path()).is_some());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_chat_waits_for_the_desktop_app_to_step_down_from_standby() {
        let place = folder();
        let desktop = holder(place.path());
        fs::write(place.path().join("pair.standby"), b"").unwrap_or_else(|error| panic!("{error}"));
        let stepping = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            drop(desktop);
        });
        let started = Instant::now();
        let file = claimed(place.path()).unwrap_or_else(|| panic!("no lock file"));
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(stepping.join().is_ok());
        assert!(opened(place.path()).is_some_and(|other| other.try_lock().is_err()));
        drop(file);
    }

    #[test]
    fn a_chat_that_gets_the_lock_clears_a_standby_left_behind() {
        let place = folder();
        let beacon = place.path().join("pair.standby");
        fs::write(&beacon, b"").unwrap_or_else(|error| panic!("{error}"));
        assert!(claimed(place.path()).is_some());
        assert!(!beacon.exists());
    }
}

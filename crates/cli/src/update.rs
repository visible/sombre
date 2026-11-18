use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use sombre_core::Roots;
use sombre_harness::config::Config;

const RELEASES: &str = match option_env!("SOMBRE_RELEASES") {
    Some(url) => url,
    None => "https://github.com/visible/sombre/releases",
};
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const LIMIT: u64 = 128 * 1024 * 1024;
const BINARY: &str = if cfg!(windows) {
    "sombre.exe"
} else {
    "sombre"
};
const ARCHIVE: &str = if cfg!(windows) { "zip" } else { "tar.gz" };

static INSTALLED: OnceLock<String> = OnceLock::new();
static SWAPPED: Mutex<Option<String>> = Mutex::new(None);

pub enum Checked {
    Source,
    Newest,
    Installed(String),
}

pub fn released() -> bool {
    option_env!("SOMBRE_RELEASE").is_some()
}

pub fn check() -> Result<Checked, String> {
    if !released() {
        return Ok(Checked::Source);
    }
    let mut swapped = SWAPPED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(version) = swapped.as_ref() {
        return Ok(Checked::Installed(version.clone()));
    }
    Ok(match install()? {
        Some(version) => {
            *swapped = Some(version.clone());
            Checked::Installed(version)
        }
        None => Checked::Newest,
    })
}

pub fn said(checked: &Checked) -> String {
    match checked {
        Checked::Source => "this sombre was built from source, so it doesn't update itself. install it from sombre.si to get updates".to_string(),
        Checked::Newest => format!("sombre {} is the newest", env!("CARGO_PKG_VERSION")),
        Checked::Installed(version) => {
            format!("sombre {version} is installed. it starts the next time you run sombre")
        }
    }
}

pub fn background() {
    if !released() || env::var_os("CI").is_some() || !io::stdout().is_terminal() {
        return;
    }
    let Some(roots) = Roots::detect() else {
        return;
    };
    if Config::load(&roots.sombre).is_ok_and(|config| config.update == Some(false)) {
        return;
    }
    thread::spawn(move || {
        clean();
        if due(&roots.sombre.join("checked"))
            && let Ok(Checked::Installed(version)) = check()
        {
            let _ = INSTALLED.set(version);
        }
    });
}

pub fn notice() {
    if let Some(version) = INSTALLED.get() {
        println!("{}", said(&Checked::Installed(version.clone())));
    }
}

pub fn run() -> ExitCode {
    if released() {
        println!("checking for a newer sombre");
    }
    match check() {
        Ok(checked) => {
            println!("{}", said(&checked));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("sombre: {error}");
            ExitCode::FAILURE
        }
    }
}

fn due(stamp: &Path) -> bool {
    let fresh = fs::metadata(stamp)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .is_some_and(|since| since < DAY);
    if fresh {
        return false;
    }
    stamp
        .parent()
        .is_some_and(|folder| fs::create_dir_all(folder).is_ok())
        && fs::write(stamp, b"").is_ok()
}

fn install() -> Result<Option<String>, String> {
    let feed = fetch(&format!("{RELEASES}/latest/download/latest.json"))?;
    let Some(latest) = newest(&feed) else {
        return Err("the latest release doesn't say its version. try again later".to_string());
    };
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    if latest <= current {
        return Ok(None);
    }
    let target = target().ok_or("there's no sombre build for this computer yet")?;
    let name = format!("sombre-{target}.{ARCHIVE}");
    let base = format!("{RELEASES}/download/v{latest}");
    let archive = fetch(&format!("{base}/{name}"))?;
    let signature = fetch(&format!("{base}/{name}.sig"))?;
    if !signed(sombre_core::release::KEY, &archive, &signature, &latest) {
        return Err("the download isn't signed by sombre, so nothing was installed".to_string());
    }
    let exe = env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("couldn't find where sombre is installed: {error}"))?;
    let folder = exe
        .parent()
        .ok_or("couldn't find the folder sombre is installed in")?;
    let work = folder.join(format!(".sombre-update-{}", std::process::id()));
    let result = fs::create_dir_all(&work)
        .map_err(|error| format!("couldn't write to {}: {error}", folder.display()))
        .and_then(|()| unpack(&work, &name, &archive))
        .and_then(|binary| swap(&binary, &exe));
    let _ = fs::remove_dir_all(&work);
    result.map(|()| Some(latest.to_string()))
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .map_err(|_| {
            "couldn't reach github to check for updates. check your connection and try again"
                .to_string()
        })?
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|_| "the update download stopped partway. try again".to_string())
}

fn newest(feed: &[u8]) -> Option<Version> {
    let feed: serde_json::Value = serde_json::from_slice(feed).ok()?;
    let text = feed["version"].as_str()?;
    Version::parse(text.strip_prefix('v').unwrap_or(text)).ok()
}

fn target() -> Option<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

fn signed(key: &str, bytes: &[u8], signature: &[u8], version: &Version) -> bool {
    let decode = |text: &[u8]| {
        STANDARD
            .decode(text.trim_ascii())
            .ok()
            .and_then(|raw| String::from_utf8(raw).ok())
    };
    let key = decode(key.as_bytes()).and_then(|text| PublicKey::decode(&text).ok());
    let signature = decode(signature).and_then(|text| Signature::decode(&text).ok());
    match (key, signature) {
        (Some(key), Some(signature)) => {
            key.verify(bytes, &signature, false).is_ok()
                && signature
                    .trusted_comment()
                    .split('\t')
                    .any(|part| part == format!("version:{version}"))
        }
        _ => false,
    }
}

fn unpack(work: &Path, name: &str, archive: &[u8]) -> Result<PathBuf, String> {
    let path = work.join(name);
    fs::write(&path, archive).map_err(|error| format!("couldn't save the update: {error}"))?;
    let unpacked = Command::new(tar())
        .arg("-xf")
        .arg(&path)
        .arg("-C")
        .arg(work)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    let binary = work.join(BINARY);
    if unpacked && binary.is_file() {
        Ok(binary)
    } else {
        Err("couldn't unpack the update".to_string())
    }
}

#[cfg(windows)]
fn tar() -> PathBuf {
    env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join(r"System32\tar.exe")
}

#[cfg(not(windows))]
fn tar() -> PathBuf {
    PathBuf::from("tar")
}

#[cfg(windows)]
fn swap(binary: &Path, exe: &Path) -> Result<(), String> {
    let old = exe.with_extension("old.exe");
    let _ = fs::remove_file(&old);
    fs::rename(exe, &old).map_err(|error| format!("couldn't replace sombre: {error}"))?;
    fs::rename(binary, exe).map_err(|error| {
        let _ = fs::rename(&old, exe);
        format!("couldn't replace sombre: {error}")
    })
}

#[cfg(not(windows))]
fn swap(binary: &Path, exe: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755))
        .and_then(|()| fs::rename(binary, exe))
        .map_err(|error| format!("couldn't replace sombre: {error}"))
}

fn clean() {
    if cfg!(windows)
        && let Ok(exe) = env::current_exe()
    {
        let _ = fs::remove_file(exe.with_extension("old.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ5MDY4OEMzQ0M3REQzMDAKUldRQTAzM013NGdHU1NqM1daSmJaeXRJN0FXb3RqLzUxZWNFMmdBYlpPS256aHlPZk9LNVpKMGsK";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRQTAzM013NGdHU1JUQnZtSHJkOUh3Tk5wZVNyUUxqYU03bkttZ0xCQk9ZejBkOE91S0NvTE1qc3RhQmJXL2U3UW1NcXJBaU4xb2Zpc2FSODIzeVRvYW8yaGcrUXJQZUFNPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0OTc5MjA1CWZpbGU6Zml4dHVyZQl2ZXJzaW9uOjAuMS4yCmt6UlQwMmQwZ2liWHNTZW0veGVlWlZKQ3VHWXlMSDJKY2o4RXRXQkxpaVRUakMvZXVveXNwcktQQ2xJT042dVl2VU1qOCtJQnJDUjhqcEFLcE5tc0R3PT0K";

    #[test]
    fn only_a_signature_from_the_sombre_key_for_that_version_installs() {
        let version = Version::new(0, 1, 2);
        let signature = SIGNATURE.as_bytes();
        assert!(signed(KEY, b"sombre update test", signature, &version));
        assert!(!signed(KEY, b"sombre update tesT", signature, &version));
        assert!(!signed(
            KEY,
            b"sombre update test",
            signature,
            &Version::new(0, 1, 3)
        ));
        assert!(!signed(
            sombre_core::release::KEY,
            b"sombre update test",
            signature,
            &version
        ));
        assert!(!signed(
            KEY,
            b"sombre update test",
            b"not a signature",
            &version
        ));
    }

    #[test]
    fn the_release_key_is_a_minisign_public_key() {
        let text = STANDARD
            .decode(sombre_core::release::KEY)
            .ok()
            .and_then(|raw| String::from_utf8(raw).ok())
            .unwrap_or_default();
        assert!(PublicKey::decode(&text).is_ok());
    }

    #[test]
    fn the_feed_version_reads_with_or_without_a_v() {
        assert_eq!(
            newest(br#"{"version":"v0.1.2","platforms":{}}"#),
            Version::parse("0.1.2").ok()
        );
        assert_eq!(
            newest(br#"{"version":"0.2.0"}"#),
            Version::parse("0.2.0").ok()
        );
        assert_eq!(newest(br#"{"notes":"no version"}"#), None);
        assert_eq!(newest(b"<html>"), None);
        assert!(newest(br#"{"version":"0.1.10"}"#) > Version::parse("0.1.9").ok());
    }

    #[test]
    fn a_check_runs_at_most_once_a_day() {
        let folder = tempfile::tempdir().ok();
        let stamp = folder
            .as_ref()
            .map(|folder| folder.path().join("nested").join("checked"))
            .unwrap_or_default();
        assert!(due(&stamp));
        assert!(!due(&stamp));
    }

    #[test]
    fn an_update_unpacks_and_replaces_the_binary() {
        let folder = tempfile::tempdir().ok();
        let root = folder
            .as_ref()
            .map(|folder| folder.path().to_path_buf())
            .unwrap_or_default();
        let source = root.join("source");
        let work = root.join("work");
        assert!(fs::create_dir_all(&source).is_ok() && fs::create_dir_all(&work).is_ok());
        assert!(fs::write(source.join(BINARY), b"new sombre").is_ok());
        let name = format!("sombre-test.{ARCHIVE}");
        let packed = Command::new(tar())
            .arg("-a")
            .arg("-cf")
            .arg(root.join(&name))
            .arg("-C")
            .arg(&source)
            .arg(BINARY)
            .status()
            .is_ok_and(|status| status.success());
        assert!(packed);
        let archive = fs::read(root.join(&name)).unwrap_or_default();
        let exe = root.join(BINARY);
        assert!(fs::write(&exe, b"old sombre").is_ok());
        let binary = unpack(&work, &name, &archive);
        assert!(binary.is_ok(), "{binary:?}");
        let swapped = binary.and_then(|binary| swap(&binary, &exe));
        assert!(swapped.is_ok(), "{swapped:?}");
        assert_eq!(fs::read(&exe).unwrap_or_default(), b"new sombre");
        assert!(unpack(&work, &name, b"not an archive").is_err());
    }
}

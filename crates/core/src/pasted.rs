use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

#[cfg(not(test))]
use crate::Roots;

#[cfg(not(test))]
fn folder() -> Option<PathBuf> {
    Roots::detect().map(|roots| roots.sombre.join("images").join("pasted"))
}

#[cfg(test)]
thread_local! {
    pub static FOLDER: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn folder() -> Option<PathBuf> {
    FOLDER.with(|folder| folder.borrow().clone())
}

fn ending(media: &str) -> Option<&'static str> {
    match media {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn named(name: &str, ending: &str) -> Option<PathBuf> {
    let safe: String = name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect();
    (!safe.is_empty()).then(|| folder().map(|folder| folder.join(format!("{safe}.{ending}"))))?
}

fn kept(path: PathBuf, write: impl FnOnce(&Path) -> Option<()>) -> Option<String> {
    if !path.exists() {
        fs::create_dir_all(path.parent()?).ok()?;
        write(&path)?;
    }
    Some(path.display().to_string())
}

pub fn keep(name: &str, media: &str, encoded: &str) -> Option<String> {
    let path = named(name, ending(media)?)?;
    kept(path, |path| {
        let bytes = STANDARD.decode(encoded.trim()).ok()?;
        fs::write(path, bytes).ok()
    })
}

pub fn url(name: &str, data: &str) -> Option<String> {
    let (head, encoded) = data.strip_prefix("data:")?.split_once(',')?;
    let media = head.strip_suffix(";base64")?;
    keep(name, media, encoded)
}

pub fn copy(name: &str, source: &Path) -> Option<String> {
    let found = source
        .extension()
        .and_then(|ending| ending.to_str())
        .map(str::to_lowercase)?;
    let media = match found.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    };
    let path = named(name, ending(media)?)?;
    kept(path, |path| fs::copy(source, path).ok().map(|_| ()))
}

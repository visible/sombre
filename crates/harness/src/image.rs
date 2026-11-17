use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub const LIMIT: u64 = 20 * 1024 * 1024;
pub const ENDINGS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

pub fn kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 11 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub fn load(path: &Path) -> Result<(&'static str, String), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("couldn't read {}: {error}", path.display()))?;
    let media = kind(&bytes)
        .ok_or_else(|| format!("{} isn't a png, jpeg, gif, or webp image", path.display()))?;
    Ok((media, STANDARD.encode(bytes)))
}

pub fn url(path: &Path) -> Result<String, String> {
    load(path).map(|(media, data)| format!("data:{media};base64,{data}"))
}

pub fn store(bytes: &[u8], root: &Path) -> Result<PathBuf, String> {
    let media = kind(bytes).ok_or("that isn't a png, jpeg, gif, or webp image")?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > LIMIT {
        return Err("that image is over 20 mb".to_string());
    }
    let folder = root.join("images");
    fs::create_dir_all(&folder)
        .map_err(|error| format!("couldn't create {}: {error}", folder.display()))?;
    let ending = media.trim_start_matches("image/").replace("jpeg", "jpg");
    let kept = folder.join(format!("{}.{ending}", sombre_core::clock::uuid()));
    fs::write(&kept, bytes)
        .map_err(|error| format!("couldn't save {}: {error}", kept.display()))?;
    Ok(kept)
}

pub fn keep(path: &Path, root: &Path) -> Result<PathBuf, String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("couldn't open {}: {error}", path.display()))?
        .len();
    if size > LIMIT {
        return Err(format!("{} is over 20 mb", path.display()));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("couldn't read {}: {error}", path.display()))?;
    if kind(&bytes).is_none() {
        return Err(format!(
            "{} isn't a png, jpeg, gif, or webp image",
            path.display()
        ));
    }
    store(&bytes, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn knows_images_by_their_first_bytes() {
        assert_eq!(kind(&PNG), Some("image/png"));
        assert_eq!(kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(kind(b"GIF89a"), Some("image/gif"));
        assert_eq!(kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(kind(b"%PDF-1.7"), None);
    }

    #[test]
    fn copies_an_image_so_later_edits_dont_change_it() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let shot = dir.path().join("Screenshot 1.png");
        fs::write(&shot, PNG).unwrap_or_else(|error| panic!("{error}"));
        let kept = keep(&shot, dir.path()).unwrap_or_else(|error| panic!("{error}"));
        assert!(kept.starts_with(dir.path().join("images")));
        assert_eq!(
            kept.extension().and_then(|ending| ending.to_str()),
            Some("png")
        );
        fs::write(&shot, b"changed").unwrap_or_else(|error| panic!("{error}"));
        assert!(url(&kept).is_ok_and(|url| url.starts_with("data:image/png;base64,")));
        let text = dir.path().join("notes.txt");
        fs::write(&text, "hi").unwrap_or_else(|error| panic!("{error}"));
        assert!(keep(&text, dir.path()).is_err());
    }
}

use std::ops::Range;
use std::path::{Path, PathBuf};

pub use sombre_harness::image::ENDINGS;

pub fn tokens(text: &str) -> Vec<(Range<usize>, String)> {
    let mut found = Vec::new();
    let mut current = String::new();
    let mut start: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        match (character, quote) {
            ('\\', None) if !cfg!(windows) => {
                start.get_or_insert(index);
                if let Some((_, next)) = characters.next() {
                    current.push(next);
                }
            }
            ('"' | '\'', None) if start.is_none() => {
                start = Some(index);
                quote = Some(character);
            }
            (closing, Some(open)) if closing == open => quote = None,
            (space, None) if space.is_whitespace() => {
                if let Some(begin) = start.take() {
                    found.push((begin..index, std::mem::take(&mut current)));
                }
            }
            (other, _) => {
                start.get_or_insert(index);
                current.push(other);
            }
        }
    }
    if let Some(begin) = start {
        found.push((begin..text.len(), current));
    }
    found
}

fn place(token: &str) -> PathBuf {
    let token = token.strip_prefix('@').unwrap_or(token);
    let token = token
        .strip_prefix("file://")
        .map_or_else(|| token.to_string(), |rest| rest.replace("%20", " "));
    match (token.strip_prefix("~/"), sombre_harness::process::home()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(token),
    }
}

pub fn images(text: &str) -> Vec<(Range<usize>, PathBuf)> {
    tokens(text)
        .into_iter()
        .filter_map(|(range, token)| {
            let path = place(&token);
            let ending = path.extension()?.to_str()?.to_lowercase();
            (ENDINGS.contains(&ending.as_str()) && path.is_file()).then_some((range, path))
        })
        .collect()
}

pub fn shift(text: &str, offset: usize) -> String {
    let mut shifted = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[Image #") {
        let (before, after) = rest.split_at(start);
        shifted.push_str(before);
        let digits: String = after["[Image #".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let end = "[Image #".len() + digits.len();
        match (digits.parse::<usize>(), after[end..].starts_with(']')) {
            (Ok(number), true) => {
                shifted.push_str(&format!("[Image #{}]", number + offset));
                rest = &after[end + 1..];
            }
            _ => {
                shifted.push_str("[Image #");
                rest = &after["[Image #".len()..];
            }
        }
    }
    shifted.push_str(rest);
    shifted
}

pub fn gather(text: &str, held: Vec<PathBuf>, root: &Path) -> (String, Vec<PathBuf>, Vec<String>) {
    let mut kept = held;
    let mut problems = Vec::new();
    let mut swaps: Vec<(Range<usize>, String)> = Vec::new();
    for (range, path) in images(text) {
        match sombre_harness::image::keep(&path, root) {
            Ok(copy) => {
                kept.push(copy);
                swaps.push((range, format!("[Image #{}]", kept.len())));
            }
            Err(problem) => problems.push(problem),
        }
    }
    let mut message = text.to_string();
    for (range, label) in swaps.into_iter().rev() {
        message.replace_range(range, &label);
    }
    let attached = kept
        .into_iter()
        .enumerate()
        .filter(|(index, _)| message.contains(&format!("[Image #{}]", index + 1)))
        .map(|(_, path)| path)
        .collect();
    (message, attached, problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn renumbers_images_after_ones_already_queued() {
        assert_eq!(
            shift("look at [Image #1] and [Image #2]", 3),
            "look at [Image #4] and [Image #5]"
        );
        assert_eq!(shift("no images", 2), "no images");
        assert_eq!(shift("[Image #x] [Image #1", 1), "[Image #x] [Image #1");
    }

    #[cfg(windows)]
    #[test]
    fn keeps_windows_backslashes_and_quotes() {
        let found: Vec<String> = tokens(r#"see C:\shots\a.png and "C:\My Shots\b.png""#)
            .into_iter()
            .map(|(_, token)| token)
            .collect();
        assert_eq!(
            found,
            ["see", r"C:\shots\a.png", "and", r"C:\My Shots\b.png"]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn splits_like_a_shell_with_escapes_and_quotes() {
        let found: Vec<String> = tokens(r#"what's /a/Screen\ Shot.png and "b c.jpg" 'd'"#)
            .into_iter()
            .map(|(_, token)| token)
            .collect();
        assert_eq!(
            found,
            ["what's", "/a/Screen Shot.png", "and", "b c.jpg", "d"]
        );
    }

    #[test]
    fn swaps_dragged_image_paths_for_placeholders() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let shot = dir.path().join("Screen Shot 1.png");
        fs::write(&shot, PNG).unwrap_or_else(|error| panic!("{error}"));
        fs::write(dir.path().join("notes.txt"), "hi").unwrap_or_else(|error| panic!("{error}"));
        let dragged = if cfg!(windows) {
            format!("\"{}\"", shot.display())
        } else {
            shot.display().to_string().replace(' ', "\\ ")
        };
        let text = format!(
            "what's wrong here {dragged} and file://{} but not {}/notes.txt or /missing.png",
            shot.display().to_string().replace(' ', "%20"),
            dir.path().display()
        );
        let (message, attached, problems) = gather(&text, Vec::new(), dir.path());
        assert!(message.starts_with("what's wrong here [Image #1] and [Image #2] but not "));
        assert!(message.ends_with("notes.txt or /missing.png"));
        assert_eq!(attached.len(), 2);
        assert!(
            attached
                .iter()
                .all(|path| path.starts_with(dir.path().join("images")))
        );
        assert!(problems.is_empty());
    }

    #[test]
    fn drops_pasted_images_whose_placeholder_was_deleted() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let held = vec![dir.path().join("a.png"), dir.path().join("b.png")];
        let (message, attached, _) = gather("only [Image #2] stays", held, dir.path());
        assert_eq!(message, "only [Image #2] stays");
        assert_eq!(attached, [dir.path().join("b.png")]);
    }
}

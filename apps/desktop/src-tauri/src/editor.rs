use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Editor {
    pub id: String,
    pub name: String,
}

const KNOWN: [(&str, &str, &str); 7] = [
    ("cursor", "Cursor", "cursor"),
    ("code", "Visual Studio Code", "code"),
    ("zed", "Zed", "zed"),
    ("windsurf", "Windsurf", "windsurf"),
    ("sublime", "Sublime Text", "subl"),
    ("nova", "Nova", "nova"),
    ("xcode", "Xcode", "xed"),
];

#[cfg(target_os = "macos")]
fn present(app: &str, _: &str) -> bool {
    let name = format!("{app}.app");
    let mut places = vec![std::path::PathBuf::from("/Applications")];
    if let Some(home) = std::env::home_dir() {
        places.push(home.join("Applications"));
    }
    places.iter().any(|place| place.join(&name).exists())
}

#[cfg(not(target_os = "macos"))]
fn present(_: &str, cli: &str) -> bool {
    sombre_harness::process::find(cli).is_some()
}

pub fn found() -> Vec<Editor> {
    KNOWN
        .iter()
        .filter(|(_, app, cli)| present(app, cli))
        .map(|(id, app, _)| Editor {
            id: (*id).to_string(),
            name: (*app).to_string(),
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn command(path: &Path, known: Option<&(&str, &str, &str)>) -> (Command, bool) {
    let mut command = Command::new("open");
    match known {
        Some((_, app, _)) => command.args(["-a", app]),
        None if path.is_dir() => command.arg("-R"),
        None => command.arg("-t"),
    };
    command.arg(path);
    (command, true)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command(path: &Path, known: Option<&(&str, &str, &str)>) -> (Command, bool) {
    if let Some(program) = known.and_then(|(_, _, cli)| sombre_harness::process::find(cli)) {
        let mut command = Command::new(program);
        command.arg(path);
        return (command, true);
    }
    let mut command = Command::new("xdg-open");
    let folder = if path.is_dir() {
        Some(path)
    } else {
        path.parent()
    };
    command.arg(folder.unwrap_or(path));
    (command, true)
}

#[cfg(windows)]
fn command(path: &Path, known: Option<&(&str, &str, &str)>) -> (Command, bool) {
    if let Some(program) = known.and_then(|(_, _, cli)| sombre_harness::process::find(cli)) {
        let mut command = Command::new(program);
        command.arg(path);
        return (command, true);
    }
    let mut command = Command::new(if path.is_dir() { "explorer" } else { "notepad" });
    command.arg(path);
    (command, !path.is_dir())
}

pub fn open(path: &Path, editor: Option<&str>) -> Result<(), String> {
    let known = editor.and_then(|id| KNOWN.iter().find(|(key, ..)| *key == id));
    let (mut launch, checked) = command(path, known);
    let status = launch
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("couldn't open {}: {error}", path.display()))?;
    if status.success() || !checked {
        Ok(())
    } else {
        Err(format!(
            "couldn't open {}. is {} still installed?",
            path.display(),
            known.map_or("its default app", |(_, app, _)| *app)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spelled(command: &Command) -> Vec<String> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|part| part.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn without_an_editor_files_open_as_text_and_folders_are_only_shown() {
        let folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let file = folder.path().join("notes.md&calc");
        std::fs::write(&file, "hi").unwrap_or_else(|error| panic!("{error}"));
        let (opened, _) = command(&file, None);
        let (shown, _) = command(folder.path(), None);
        let opened = spelled(&opened);
        let shown = spelled(&shown);
        assert!(!opened.iter().any(|part| part == "cmd" || part == "start"));
        if cfg!(target_os = "macos") {
            assert_eq!(opened[..2], ["open", "-t"]);
            assert_eq!(shown[..2], ["open", "-R"]);
        } else if cfg!(windows) {
            assert_eq!(opened[0], "notepad");
            assert_eq!(shown[0], "explorer");
        } else {
            assert_eq!(
                opened,
                ["xdg-open".to_string(), folder.path().display().to_string()]
            );
        }
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(opened.last(), Some(&file.display().to_string()));
        }
    }

    #[test]
    fn every_editor_has_a_unique_id() {
        let mut ids: Vec<&str> = KNOWN.iter().map(|(id, ..)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN.len());
    }
}

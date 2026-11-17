use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

const LINES: usize = 50;
const SKIP: [&str; 5] = [".git", "node_modules", "target", ".next", ".DS_Store"];

fn found(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn missing() -> Result<String, String> {
    Err("searching needs ripgrep on windows. install it with `winget install BurntSushi.ripgrep.MSVC`, or search with the shell tool".to_string())
}

fn run(command: &mut Command) -> Result<String, String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("couldn't search: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Ok("no matches".to_string());
    }
    let mut shown = lines[..lines.len().min(LINES)].join("\n");
    if lines.len() > LINES {
        shown.push_str(&format!(
            "\n… {} more. narrow the pattern or add a glob to see them",
            lines.len() - LINES
        ));
    }
    Ok(shown)
}

pub fn grep(input: &Value, cwd: &Path) -> Result<String, String> {
    let pattern = input["pattern"]
        .as_str()
        .filter(|pattern| !pattern.is_empty())
        .ok_or_else(|| "pattern is missing".to_string())?;
    let place = input["path"].as_str().unwrap_or(".");
    if found("rg") {
        let mut command = Command::new("rg");
        command
            .args([
                "--line-number",
                "--no-heading",
                "--color",
                "never",
                "--max-columns",
                "300",
            ])
            .current_dir(cwd);
        if let Some(glob) = input["glob"].as_str() {
            command.args(["--glob", glob]);
        }
        command.arg("--").arg(pattern).arg(place);
        return run(&mut command);
    }
    if cfg!(windows) {
        return missing();
    }
    let mut command = Command::new("grep");
    command.args([
        "-rnI",
        "--exclude-dir=.git",
        "--exclude-dir=node_modules",
        "-e",
    ]);
    command.arg(pattern).arg(place).current_dir(cwd);
    run(&mut command)
}

pub fn glob(input: &Value, cwd: &Path) -> Result<String, String> {
    let pattern = input["pattern"]
        .as_str()
        .filter(|pattern| !pattern.is_empty())
        .ok_or_else(|| "pattern is missing".to_string())?;
    let place = crate::tools::resolve(cwd, input["path"].as_str().unwrap_or("."));
    if found("rg") {
        let mut command = Command::new("rg");
        command
            .args(["--files", "--color", "never", "--glob", pattern, "--"])
            .arg(&place)
            .current_dir(cwd);
        return run(&mut command);
    }
    if cfg!(windows) {
        return missing();
    }
    let mut command = Command::new("find");
    command
        .arg(&place)
        .args([
            "-not",
            "-path",
            "*/.git/*",
            "-not",
            "-path",
            "*/node_modules/*",
            "-name",
        ])
        .arg(pattern)
        .current_dir(cwd);
    run(&mut command)
}

fn entries(folder: &Path) -> Result<Vec<(String, bool)>, String> {
    let mut found: Vec<(String, bool)> = fs::read_dir(folder)
        .map_err(|error| format!("couldn't list {}: {error}", folder.display()))?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (!SKIP.contains(&name.as_str()))
                .then(|| (name, entry.file_type().is_ok_and(|kind| kind.is_dir())))
        })
        .collect();
    found.sort_by_key(|(name, folder)| (!folder, name.to_lowercase()));
    Ok(found)
}

pub fn list(folder: &Path) -> Result<String, String> {
    let mut lines: Vec<String> = Vec::new();
    for (name, inner) in entries(folder)? {
        lines.push(if inner {
            format!("{name}/")
        } else {
            name.clone()
        });
        if inner {
            let children = entries(&folder.join(&name)).unwrap_or_default();
            for (child, deeper) in children.iter().take(20) {
                lines.push(format!("  {child}{}", if *deeper { "/" } else { "" }));
            }
            if children.len() > 20 {
                lines.push(format!("  … {} more", children.len() - 20));
            }
        }
    }
    if lines.is_empty() {
        return Ok("the folder is empty".to_string());
    }
    let total = lines.len();
    lines.truncate(LINES * 4);
    if total > lines.len() {
        lines.push(format!(
            "… {} more. read a subfolder to see it",
            total - lines.len()
        ));
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_cannot_turn_into_a_find_action() {
        let project = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let kept = project.path().join("keep.rs");
        fs::write(&kept, "fn main() {}").unwrap_or_else(|error| panic!("{error}"));
        let _ = glob(
            &serde_json::json!({"pattern": "*.rs", "path": "-delete"}),
            project.path(),
        );
        assert!(kept.exists());
    }
    use serde_json::json;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        fs::create_dir_all(dir.path().join("src")).unwrap_or_else(|error| panic!("{error}"));
        fs::create_dir_all(dir.path().join("node_modules"))
            .unwrap_or_else(|error| panic!("{error}"));
        fs::write(dir.path().join("src/app.ts"), "const answer = 42\n")
            .unwrap_or_else(|error| panic!("{error}"));
        fs::write(dir.path().join("README.md"), "hello\n")
            .unwrap_or_else(|error| panic!("{error}"));
        dir
    }

    #[test]
    fn lists_folders_first_and_skips_noise() {
        let dir = project();
        assert_eq!(
            list(dir.path()).unwrap_or_default(),
            "src/\n  app.ts\nREADME.md"
        );
    }

    #[test]
    fn greps_with_line_numbers() {
        let dir = project();
        if cfg!(windows) && !found("rg") {
            assert!(
                grep(&json!({"pattern": "answer"}), dir.path())
                    .is_err_and(|error| error.contains("winget"))
            );
            return;
        }
        let text = grep(&json!({"pattern": "answer"}), dir.path()).unwrap_or_default();
        assert!(text.contains("app.ts:1:const answer = 42"), "{text}");
        assert_eq!(
            grep(&json!({"pattern": "missing-word"}), dir.path()).unwrap_or_default(),
            "no matches"
        );
    }

    #[test]
    fn globs_files_by_name() {
        let dir = project();
        if cfg!(windows) && !found("rg") {
            assert!(
                glob(&json!({"pattern": "*.ts"}), dir.path())
                    .is_err_and(|error| error.contains("winget"))
            );
            return;
        }
        let text = glob(&json!({"pattern": "*.ts"}), dir.path()).unwrap_or_default();
        assert!(text.contains("app.ts"), "{text}");
        assert!(!text.contains("README"), "{text}");
    }

    #[test]
    fn explains_missing_input() {
        let dir = project();
        assert!(grep(&json!({}), dir.path()).is_err());
        assert!(glob(&json!({"pattern": ""}), dir.path()).is_err());
    }
}

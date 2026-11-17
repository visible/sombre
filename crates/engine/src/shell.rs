use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use sombre_harness::{process, sandbox};

const KEPT: usize = 4_000;

#[derive(Debug, PartialEq, Eq)]
pub struct Ran {
    pub output: String,
    pub code: Option<i32>,
    pub stopped: bool,
}

pub fn run(
    command: &str,
    cwd: &Path,
    stop: &AtomicBool,
    fenced: bool,
    mut on: impl FnMut(&str),
) -> Result<Ran, String> {
    stop.store(false, Ordering::SeqCst);
    let inner = process::shell(command);
    let wrapped = fenced.then(|| sandbox::wrap(cwd, &inner)).flatten();
    let (mut shell, detached) = match wrapped {
        Some(wrapped) => (wrapped, true),
        None => (inner, false),
    };
    let started = if detached {
        process::detach(&mut shell)
    } else {
        process::group(&mut shell)
    };
    let mut child = started
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("couldn't start your shell: {error}"))?;
    let _tracked = process::Tracked::new(child.id());
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(sombre_core::scrub::scrub(&line)).is_err() {
                    break;
                }
            }
        });
    }
    let mut output = String::new();
    let mut stopped = false;
    loop {
        if stop.swap(false, Ordering::SeqCst) && !stopped {
            stopped = true;
            process::interrupt(child.id());
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                on(&line);
                output.push_str(&line);
                output.push('\n');
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let code = child.wait().ok().and_then(|status| status.code());
    Ok(Ran {
        code,
        output: output.trim_end().to_string(),
        stopped,
    })
}

pub fn note(command: &str, ran: &Ran) -> String {
    let count = ran.output.chars().count();
    let mut output: String = ran.output.chars().take(KEPT).collect();
    if count > KEPT {
        output.push_str(&format!("\n… {} more characters", count - KEPT));
    }
    let status = match (ran.stopped, ran.code) {
        (true, _) => " (stopped)".to_string(),
        (false, Some(0)) => String::new(),
        (false, Some(code)) => format!(" (exit code {code})"),
        (false, None) => " (killed)".to_string(),
    };
    format!("$ {command}{status}\n{output}")
}

pub fn attach(message: &str, notes: &[String]) -> String {
    if notes.is_empty() {
        return message.to_string();
    }
    format!(
        "{message}\n\nfor context, i ran this in my terminal:\n```\n{}\n```",
        notes.join("\n\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn ran(command: &str) -> (Ran, Vec<String>) {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let stop = AtomicBool::new(false);
        let mut seen = Vec::new();
        let ran = run(command, dir.path(), &stop, false, |line| {
            seen.push(line.to_string())
        })
        .unwrap_or_else(|error| panic!("{error}"));
        (ran, seen)
    }

    #[cfg(unix)]
    #[test]
    fn output_is_scrubbed_of_terminal_escapes() {
        let (ran, seen) = ran("printf '\\033]52;c;aGk=\\007safe\\n'");
        assert_eq!(seen, ["safe"]);
        assert_eq!(ran.output, "safe");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_fenced_check_cannot_write_outside_the_project() {
        let project = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let home = sombre_harness::process::home().unwrap_or_else(|| panic!("no home"));
        let outside = tempfile::tempdir_in(&home).unwrap_or_else(|error| panic!("{error}"));
        let target = outside.path().join("escaped");
        let stop = AtomicBool::new(false);
        let command = format!("echo x > '{}'; echo y > kept", target.display());
        run(&command, project.path(), &stop, true, |_| {})
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!target.exists());
        assert!(project.path().join("kept").exists());
    }

    #[cfg(unix)]
    #[test]
    fn streams_output_and_reports_success() {
        let (ran, seen) = ran("echo one; echo two");
        assert_eq!(seen, ["one", "two"]);
        assert_eq!(ran.output, "one\ntwo");
        assert_eq!(ran.code, Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn keeps_errors_and_exit_codes() {
        let (ran, _) = ran("echo broken >&2; exit 4");
        assert_eq!(ran.output, "broken");
        assert_eq!(ran.code, Some(4));
        assert_eq!(note("make", &ran), "$ make (exit code 4)\nbroken");
    }

    #[test]
    fn attaches_shell_notes_after_the_message() {
        assert_eq!(attach("fix it", &[]), "fix it");
        let text = attach("fix it", &["$ ls\na.txt".to_string()]);
        assert!(text.starts_with("fix it\n\nfor context"));
        assert!(text.contains("$ ls\na.txt"));
    }

    #[test]
    fn caps_long_output() {
        let ran = Ran {
            code: Some(0),
            output: "x".repeat(KEPT + 10),
            stopped: false,
        };
        assert!(note("seq", &ran).ends_with("… 10 more characters"));
    }
}

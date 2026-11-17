#[cfg(any(target_os = "linux", test))]
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;

#[cfg(any(target_os = "macos", target_os = "linux", test))]
const CACHES: [&str; 8] = [
    ".cache",
    "Library/Caches",
    ".bun",
    ".npm",
    ".cargo",
    ".rustup",
    ".local/share",
    ".config",
];

#[cfg(any(target_os = "macos", test))]
pub fn profile(cwd: &Path) -> String {
    let mut places: Vec<PathBuf> = vec![
        real(cwd),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/private/var/folders"),
    ];
    if let Some(home) = crate::process::home() {
        places.extend(CACHES.iter().map(|cache| home.join(cache)));
    }
    let allowed: String = places
        .iter()
        .map(|place| format!(" (subpath \"{}\")", quote(&place.display().to_string())))
        .collect();
    format!(
        "(version 1)(allow default)(deny file-write*)(allow file-write*{allowed} (regex #\"^/dev/\"))"
    )
}

#[cfg(target_os = "macos")]
pub fn wrap(cwd: &Path, inner: &Command) -> Option<Command> {
    let mut launch = Command::new("/usr/bin/sandbox-exec");
    launch
        .arg("-p")
        .arg(profile(cwd))
        .arg(inner.get_program())
        .args(inner.get_args());
    Some(launch)
}

#[cfg(target_os = "linux")]
pub fn wrap(cwd: &Path, inner: &Command) -> Option<Command> {
    let mut launch = Command::new(bubblewrap()?);
    launch
        .args(binds(cwd, crate::process::home().as_deref()))
        .arg("--")
        .arg(inner.get_program())
        .args(inner.get_args());
    Some(launch)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn wrap(_: &Path, _: &Command) -> Option<Command> {
    None
}

#[cfg(target_os = "linux")]
fn bubblewrap() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let program = crate::process::find("bwrap")?;
            Command::new(&program)
                .args(binds(Path::new("/"), None))
                .args(["--", "true"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok()
                .filter(std::process::ExitStatus::success)
                .map(|_| program)
        })
        .as_deref()
}

#[cfg(any(target_os = "linux", test))]
pub fn binds(cwd: &Path, home: Option<&Path>) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--ro-bind",
        "/",
        "/",
        "--dev-bind",
        "/dev",
        "/dev",
        "--proc",
        "/proc",
        "--bind",
        "/tmp",
        "/tmp",
        "--bind-try",
        "/var/tmp",
        "/var/tmp",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    let mut writable: Vec<PathBuf> = home
        .map(|home| CACHES.iter().map(|cache| home.join(cache)).collect())
        .unwrap_or_default();
    writable.push(real(cwd));
    for place in writable {
        args.extend([
            OsString::from("--bind-try"),
            place.clone().into_os_string(),
            place.into_os_string(),
        ]);
    }
    args.extend(
        ["--unshare-pid", "--new-session", "--die-with-parent"]
            .iter()
            .map(OsString::from),
    );
    args
}

pub fn real(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(any(target_os = "macos", test))]
fn quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn blocked(output: &str) -> bool {
    let output = output.to_lowercase();
    output.contains("operation not permitted") || output.contains("read-only file system")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_writes_only_in_safe_places() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let text = profile(dir.path());
        assert!(text.starts_with("(version 1)(allow default)(deny file-write*)"));
        assert!(text.contains(&format!(
            "(subpath \"{}\")",
            quote(&real(dir.path()).display().to_string())
        )));
        assert!(text.contains("(subpath \"/private/tmp\")"));
    }

    #[test]
    fn quotes_paths_safely() {
        assert_eq!(quote(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn spots_sandbox_refusals() {
        assert!(blocked("zsh:1: operation not permitted: /etc/x"));
        assert!(blocked(
            "sh: 1: cannot create /etc/x: Read-only file system"
        ));
        assert!(!blocked("all good"));
    }

    #[test]
    fn bubblewrap_mounts_everything_read_only_then_opens_safe_places() {
        let args: Vec<String> = binds(Path::new("/work/app"), Some(Path::new("/home/me")))
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[..3], ["--ro-bind", "/", "/"]);
        let opened = |place: &str| {
            args.windows(3).any(|window| {
                window[0].starts_with("--bind") && window[1] == place && window[2] == place
            })
        };
        assert!(opened("/tmp"));
        assert!(opened("/work/app"));
        assert!(opened("/home/me/.cache"));
        assert!(!opened("/home/me"));
        assert!(args.ends_with(&[
            "--unshare-pid".to_string(),
            "--new-session".to_string(),
            "--die-with-parent".to_string()
        ]));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_keeps_writes_inside_the_project() {
        if bubblewrap().is_none() {
            return;
        }
        let project = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let home = crate::process::home().unwrap_or_else(|| PathBuf::from("/root"));
        let outside = tempfile::tempdir_in(&home).unwrap_or_else(|error| panic!("{error}"));
        let script = format!(
            "echo ok > '{}/kept'; echo no > '{}/escaped'; true",
            project.path().display(),
            outside.path().display()
        );
        let mut inner = Command::new("sh");
        inner.arg("-c").arg(script);
        let status = wrap(project.path(), &inner)
            .and_then(|mut launch| launch.status().ok())
            .is_some_and(|status| status.success());
        assert!(status);
        assert!(project.path().join("kept").exists());
        assert!(!outside.path().join("escaped").exists());
    }
}

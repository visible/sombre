use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn group(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0200);
    }
    command
}

pub fn detach(command: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                nix::unistd::setsid()
                    .map(|_| ())
                    .map_err(std::io::Error::from)
            });
        }
        command
    }
    #[cfg(not(unix))]
    group(command)
}

pub fn interrupt(pid: u32) {
    signal(pid, false);
}

pub fn kill(pid: u32) {
    signal(pid, true);
}

#[cfg(unix)]
fn signal(pid: u32, hard: bool) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    if let Ok(id) = i32::try_from(pid) {
        let kind = if hard {
            Signal::SIGKILL
        } else {
            Signal::SIGINT
        };
        let _ = killpg(Pid::from_raw(id), kind);
    }
}

#[cfg(not(unix))]
fn signal(pid: u32, hard: bool) {
    let mut taskkill = Command::new("taskkill");
    taskkill.args(["/T", "/PID", &pid.to_string()]);
    if hard {
        taskkill.arg("/F");
    }
    let _ = taskkill
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn shell(script: &str) -> Command {
    if cfg!(windows) {
        let mut command = Command::new("powershell");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!("& {{ {script} }} 2>&1"));
        command
    } else {
        let mut command = Command::new(env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()));
        command.arg("-lc").arg(format!("{{ {script}\n}} 2>&1"));
        command
    }
}

const MARK: &str = "\u{0}sombre-environment\u{0}";

pub fn environment(output: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(output);
    let Some((_, listed)) = text.split_once(MARK) else {
        return Vec::new();
    };
    listed
        .split('\0')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(name, _)| !name.is_empty() && !name.contains('\n'))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

pub fn login() -> Vec<(String, String)> {
    if cfg!(windows) {
        return Vec::new();
    }
    let program = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let Ok(child) = Command::new(program)
        .args(["-ilc", "printf '\\0sombre-environment\\0'; env -0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    let id = child.id();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(output)) => environment(&output.stdout),
        _ => {
            kill(id);
            Vec::new()
        }
    }
}

pub fn home() -> Option<PathBuf> {
    env::home_dir().filter(|home| !home.as_os_str().is_empty())
}

pub fn find(program: &str) -> Option<PathBuf> {
    let endings: Vec<String> = if cfg!(windows) {
        env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string())
            .split(';')
            .map(str::to_lowercase)
            .chain([String::new()])
            .collect()
    } else {
        vec![String::new()]
    };
    let mut places: Vec<PathBuf> = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    if let Some(home) = home() {
        places.push(home.join(".local").join("bin"));
        places.push(home.join(".claude").join("local"));
        places.push(home.join(".bun").join("bin"));
    }
    places.iter().find_map(|place| {
        endings
            .iter()
            .map(|ending| place.join(format!("{program}{ending}")))
            .find(|candidate| runnable(candidate))
    })
}

fn runnable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub fn quiet(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

static HUNG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn hung_up(signal: i32) {
    let flag = if signal == nix::libc::SIGHUP {
        &HUNG
    } else {
        &ENDED
    };
    flag.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(unix)]
pub fn hangups() {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
    let action = SigAction::new(
        SigHandler::Handler(hung_up),
        SaFlags::empty(),
        SigSet::empty(),
    );
    for signal in [Signal::SIGHUP, Signal::SIGTERM] {
        let _ = unsafe { sigaction(signal, &action) };
    }
}

#[cfg(not(unix))]
pub fn hangups() {}

static LIVE: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

pub fn track(pid: u32) {
    if let Ok(mut live) = LIVE.lock() {
        live.push(pid);
    }
}

pub fn untrack(pid: u32) {
    if let Ok(mut live) = LIVE.lock() {
        live.retain(|known| *known != pid);
    }
}

pub struct Tracked(u32);

impl Tracked {
    pub fn new(pid: u32) -> Self {
        track(pid);
        Self(pid)
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        untrack(self.0);
    }
}

pub fn bail() -> ! {
    let live = LIVE.lock().map(|live| live.clone()).unwrap_or_default();
    for pid in live {
        kill(pid);
    }
    std::process::exit(0)
}

#[cfg(unix)]
pub fn gone() -> bool {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use std::os::fd::AsFd;
    let input = std::io::stdin();
    let mut watched = [PollFd::new(input.as_fd(), PollFlags::empty())];
    let hung = poll(&mut watched, PollTimeout::ZERO).is_ok()
        && watched[0]
            .revents()
            .is_some_and(|events| events.intersects(PollFlags::POLLHUP | PollFlags::POLLNVAL));
    hung || HUNG.load(std::sync::atomic::Ordering::SeqCst)
        || std::fs::File::open("/dev/tty").is_err()
}

#[cfg(not(unix))]
pub fn gone() -> bool {
    HUNG.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn hung() -> bool {
    ENDED.load(std::sync::atomic::Ordering::SeqCst) || gone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_detached_command_runs_in_its_own_session() {
        use nix::unistd::{Pid, getsid};
        let mut command = Command::new("sleep");
        command.arg("2");
        let mut child = detach(&mut command)
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        let pid = Pid::from_raw(i32::try_from(child.id()).unwrap_or_default());
        let theirs = getsid(Some(pid)).unwrap_or_else(|error| panic!("{error}"));
        let ours = getsid(None).unwrap_or_else(|error| panic!("{error}"));
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(theirs, pid);
        assert_ne!(theirs, ours);
    }

    #[test]
    fn reads_the_login_environment_after_the_mark() {
        let output = b"welcome back\n\0sombre-environment\0PATH=/opt/homebrew/bin:/usr/bin\0EMPTY=\0NOTE=a=b\0";
        assert_eq!(
            environment(output),
            vec![
                ("PATH".to_string(), "/opt/homebrew/bin:/usr/bin".to_string()),
                ("EMPTY".to_string(), String::new()),
                ("NOTE".to_string(), "a=b".to_string()),
            ]
        );
        assert!(environment(b"no mark here").is_empty());
    }

    #[test]
    fn finds_programs_on_the_path() {
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(find(shell).is_some());
        assert_eq!(find("sombre-no-such-program"), None);
    }

    #[test]
    fn knows_the_home_folder() {
        assert!(home().is_some_and(|home| home.is_dir()));
    }

    #[cfg(unix)]
    #[test]
    fn runs_scripts_in_the_users_shell_with_errors_merged() {
        let output = shell("echo out; echo err >&2").output().ok();
        let text = output
            .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
            .unwrap_or_default();
        assert_eq!(text, "out\nerr\n");
    }
}

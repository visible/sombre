use std::path::Path;
use std::process::Child;
use std::sync::Mutex;

static AWAKE: Mutex<Option<Child>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lid {
    Sleeps,
    Running,
    Stuck,
    Missing,
}

pub fn disabled(settings: &str) -> bool {
    settings.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.next() == Some("SleepDisabled") && words.next() == Some("1")
    })
}

pub fn reading(battery: &str) -> Option<String> {
    let source = if battery.contains("'AC Power'") {
        "plugged in"
    } else if battery.contains("'Battery Power'") {
        "on battery"
    } else {
        return None;
    };
    let level = battery
        .split(|character: char| character.is_whitespace() || character == ';')
        .find(|word| word.len() > 1 && word.ends_with('%'));
    Some(level.map_or_else(|| source.to_string(), |level| format!("{source}, {level}")))
}

#[cfg(target_os = "linux")]
static LID: Mutex<Option<Child>> = Mutex::new(None);

fn alive(held: &mut Option<Child>) -> bool {
    held.as_mut()
        .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
}

pub fn awakened() -> bool {
    AWAKE.lock().is_ok_and(|mut held| alive(&mut held))
}

#[cfg(any(target_os = "linux", windows))]
fn keep(slot: &Mutex<Option<Child>>, on: bool, start: impl FnOnce() -> Option<Child>) {
    let Ok(mut held) = slot.lock() else {
        return;
    };
    if on && !alive(&mut held) {
        *held = start();
    } else if !on && let Some(mut child) = held.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(any(target_os = "linux", test))]
const WATCH: &str = r#"while kill -0 "$1" 2>/dev/null; do sleep 5; done"#;

#[cfg(any(target_os = "linux", test))]
const GUARD: &str = r#"while kill -0 "$1" 2>/dev/null; do
  for battery in /sys/class/power_supply/BAT*; do
    [ "$(cat "$battery/status" 2>/dev/null)" = Discharging ] && [ "$(cat "$battery/capacity" 2>/dev/null || echo 100)" -lt 20 ] && exit 0
  done
  sleep 5
done"#;

#[cfg(any(target_os = "linux", test))]
fn inhibitor(what: &str, pid: u32) -> Vec<String> {
    let watch = if what == "handle-lid-switch" {
        GUARD
    } else {
        WATCH
    };
    [
        &format!("--what={what}"),
        "--who=sombre",
        "--why=sombre is open",
        "--mode=block",
        "sh",
        "-c",
        watch,
        "sh",
        &pid.to_string(),
    ]
    .map(str::to_string)
    .to_vec()
}

#[cfg(any(target_os = "linux", test))]
fn supplies(root: &Path) -> Option<String> {
    let read = |path: std::path::PathBuf| {
        std::fs::read_to_string(path).map(|text| text.trim().to_string())
    };
    let mut plugged = false;
    let mut charge: Option<(String, String)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        match read(path.join("type")).as_deref() {
            Ok("Battery") => {
                charge = Some((
                    read(path.join("status")).unwrap_or_default(),
                    read(path.join("capacity")).unwrap_or_default(),
                ));
            }
            Ok("Mains" | "USB") => {
                plugged |= read(path.join("online")).is_ok_and(|online| online == "1")
            }
            _ => {}
        }
    }
    match charge {
        Some((status, capacity)) => {
            let source = if plugged || status != "Discharging" {
                "plugged in"
            } else {
                "on battery"
            };
            Some(if capacity.is_empty() {
                source.to_string()
            } else {
                format!("{source}, {capacity}%")
            })
        }
        None => plugged.then(|| "plugged in".to_string()),
    }
}

#[cfg(any(windows, test))]
const KEEPALIVE: &str = "Add-Type -Namespace Sombre -Name Power -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern uint SetThreadExecutionState(uint flags);'; [Sombre.Power]::SetThreadExecutionState(0x80000001) | Out-Null; while (Get-Process -Id PID -ErrorAction SilentlyContinue) { Start-Sleep -Seconds 5 }";

#[cfg(any(windows, test))]
const CHARGE: &str = "Get-CimInstance -ClassName Win32_Battery | Select-Object -First 1 | ForEach-Object { \"$($_.BatteryStatus) $($_.EstimatedChargeRemaining)\" }";

#[cfg(any(windows, test))]
const LIDS: &str = r"function Read-Lid { $found = @(powercfg /q SCHEME_CURRENT SUB_BUTTONS LIDACTION | Select-String -Pattern ':\s*0x([0-9a-fA-F]+)\s*$' | ForEach-Object { [Convert]::ToInt32($_.Matches[0].Groups[1].Value, 16) }); if ($found.Count -ge 2) { $found[-2..-1] } else { @(1, 1) } }
function Set-Lid($ac, $dc) { powercfg /setacvalueindex SCHEME_CURRENT SUB_BUTTONS LIDACTION $ac; powercfg /setdcvalueindex SCHEME_CURRENT SUB_BUTTONS LIDACTION $dc; powercfg /setactive SCHEME_CURRENT }
";

#[cfg(any(windows, test))]
const SENTRY: &str = r"$saved = Read-Lid
Set-Content -LiteralPath '__SAVED__' -Value ($saved -join ' ')
Set-Lid 0 0
while ((Get-Process -Id __OWNER__ -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath '__MARK__')) {
  $battery = Get-CimInstance -ClassName Win32_Battery | Select-Object -First 1
  if ($battery -and $battery.BatteryStatus -eq 1 -and $battery.EstimatedChargeRemaining -lt 20) { break }
  Start-Sleep -Seconds 5
}
Set-Lid $saved[0] $saved[1]
Remove-Item -LiteralPath '__MARK__', '__SAVED__' -Force -ErrorAction SilentlyContinue
";

#[cfg(any(windows, test))]
const RESTORE: &str = r"$saved = @((Get-Content -LiteralPath '__SAVED__' -Raw).Trim() -split '\s+' | ForEach-Object { [int]$_ })
Set-Lid $saved[0] $saved[1]
Remove-Item -LiteralPath '__SAVED__' -Force
";

#[cfg(any(windows, test))]
fn literal(path: &Path) -> String {
    path.display()
        .to_string()
        .chars()
        .flat_map(|character| {
            let quote = matches!(
                character,
                '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}'
            );
            std::iter::repeat_n(character, if quote { 2 } else { 1 })
        })
        .collect()
}

#[cfg(any(windows, test))]
fn watcher(owner: u32, mark: &Path) -> String {
    format!(
        "{LIDS}{}",
        SENTRY
            .replace("__OWNER__", &owner.to_string())
            .replace("__SAVED__", &literal(&saved(mark)))
            .replace("__MARK__", &literal(mark))
    )
}

#[cfg(any(windows, test))]
fn restorer(mark: &Path) -> String {
    format!(
        "{LIDS}{}",
        RESTORE.replace("__SAVED__", &literal(&saved(mark)))
    )
}

#[cfg(any(windows, test))]
fn elevated(script: &str) -> String {
    format!(
        "try {{ Start-Process -FilePath powershell -Verb RunAs -WindowStyle Hidden -ErrorAction Stop -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{}'; exit 0 }} catch {{ exit 1 }}",
        encoded(script)
    )
}

#[cfg(any(windows, test))]
fn saved(mark: &Path) -> std::path::PathBuf {
    mark.with_extension("lid")
}

#[cfg(any(windows, test))]
fn actions(text: &str) -> Option<(u32, u32)> {
    let found: Vec<u32> = text
        .lines()
        .filter_map(|line| {
            let (_, value) = line.trim_end().rsplit_once(':')?;
            u32::from_str_radix(value.trim().strip_prefix("0x")?, 16).ok()
        })
        .collect();
    match found.as_slice() {
        [.., ac, dc] => Some((*ac, *dc)),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn encoded(script: &str) -> String {
    use base64::Engine;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(any(windows, test))]
fn charged(text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    let status: u8 = words.next()?.parse().ok()?;
    let source = if matches!(status, 1 | 4 | 5) {
        "on battery"
    } else {
        "plugged in"
    };
    Some(match words.next() {
        Some(level) => format!("{source}, {level}%"),
        None => source.to_string(),
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::{Child, Command, Stdio};

    pub fn inhibit(what: &str) -> Option<Child> {
        Command::new("systemd-inhibit")
            .args(super::inhibitor(what, std::process::id()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    }

    pub fn present() -> bool {
        crate::process::find("systemd-inhibit").is_some()
    }
}

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::Duration;

    const HIDDEN: u32 = 0x0800_0000;

    fn powershell(script: &str) -> Command {
        let mut command = Command::new("powershell");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &super::encoded(script),
            ])
            .creation_flags(HIDDEN)
            .stdin(Stdio::null());
        command
    }

    pub fn hold() -> Option<Child> {
        powershell(&super::KEEPALIVE.replace("PID", &std::process::id().to_string()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    }

    pub fn battery() -> Option<String> {
        let output = powershell(super::CHARGE).output().ok()?;
        super::charged(&String::from_utf8_lossy(&output.stdout))
    }

    pub fn actions() -> Option<(u32, u32)> {
        let output = Command::new("powercfg")
            .args(["/q", "SCHEME_CURRENT", "SUB_BUTTONS", "LIDACTION"])
            .creation_flags(HIDDEN)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        super::actions(&String::from_utf8_lossy(&output.stdout))
    }

    pub fn managed(mark: &Path) -> bool {
        let Some(pid) = std::fs::read_to_string(mark)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
        else {
            return false;
        };
        if pid == std::process::id() {
            return true;
        }
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .creation_flags(HIDDEN)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
            })
    }

    pub fn elevate(script: &str) -> bool {
        powershell(&super::elevated(script))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    pub fn wait(done: impl Fn() -> bool) -> bool {
        for _ in 0..24 {
            if done() {
                return true;
            }
            thread::sleep(Duration::from_millis(500));
        }
        false
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    const HELPER: &str = r#"pmset -a disablesleep 1
while kill -0 "$1" 2>/dev/null && [ -e "$2" ]; do
  case "$(pmset -g batt)" in
    *discharging*)
      left=$(pmset -g batt | grep -o '[0-9]*%' | head -1 | tr -d %)
      [ "${left:-100}" -lt 20 ] && break ;;
  esac
  sleep 5
done
pmset -a disablesleep 0
rm -f "$2""#;

    pub fn pmset(args: &[&str]) -> String {
        Command::new("/usr/bin/pmset")
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    }

    pub fn alive(mark: &Path) -> bool {
        std::fs::read_to_string(mark)
            .ok()
            .and_then(|pid| pid.trim().parse::<i32>().ok())
            .is_some_and(|pid| {
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
            })
    }

    pub fn settle(off: bool) -> bool {
        for _ in 0..16 {
            if super::disabled(&pmset(&["-g"])) != off {
                return true;
            }
            thread::sleep(Duration::from_millis(500));
        }
        false
    }

    pub fn helper(mark: &Path) -> bool {
        let pid = std::process::id().to_string();
        Command::new("/usr/bin/sudo")
            .args(["-b", "/bin/sh", "-c", HELPER, "sombre", &pid])
            .arg(mark)
            .status()
            .is_ok_and(|status| status.success())
    }

    pub fn reset() -> bool {
        Command::new("/usr/bin/sudo")
            .args(["/usr/bin/pmset", "-a", "disablesleep", "0"])
            .status()
            .is_ok_and(|status| status.success())
    }
}

#[cfg(target_os = "macos")]
pub fn awake(on: bool) {
    use std::process::{Command, Stdio};
    let Ok(mut held) = AWAKE.lock() else {
        return;
    };
    if on && held.is_none() {
        *held = Command::new("/usr/bin/caffeinate")
            .args(["-i", "-s", "-w", &std::process::id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
    } else if !on && let Some(mut child) = held.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(target_os = "linux")]
pub fn awake(on: bool) {
    keep(&AWAKE, on, || linux::inhibit("idle:sleep"));
}

#[cfg(windows)]
pub fn awake(on: bool) {
    keep(&AWAKE, on, windows::hold);
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub const fn awake(_: bool) {}

#[cfg(target_os = "macos")]
pub fn battery() -> Option<String> {
    reading(&mac::pmset(&["-g", "batt"]))
}

#[cfg(target_os = "linux")]
pub fn battery() -> Option<String> {
    supplies(Path::new("/sys/class/power_supply"))
}

#[cfg(windows)]
pub fn battery() -> Option<String> {
    windows::battery()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub const fn battery() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
pub fn lid(mark: &Path) -> Lid {
    match (disabled(&mac::pmset(&["-g"])), mac::alive(mark)) {
        (false, _) => Lid::Sleeps,
        (true, true) => Lid::Running,
        (true, false) => Lid::Stuck,
    }
}

#[cfg(target_os = "linux")]
pub fn lid(_: &Path) -> Lid {
    if LID.lock().is_ok_and(|mut held| alive(&mut held)) {
        Lid::Running
    } else if linux::present() {
        Lid::Sleeps
    } else {
        Lid::Missing
    }
}

#[cfg(windows)]
pub fn lid(mark: &Path) -> Lid {
    let Some(found) = windows::actions() else {
        return Lid::Missing;
    };
    if windows::managed(mark) {
        return Lid::Running;
    }
    match (found == (0, 0), saved(mark).exists()) {
        (true, true) => Lid::Stuck,
        (true, false) => Lid::Running,
        (false, _) => Lid::Sleeps,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub const fn lid(_: &Path) -> Lid {
    Lid::Missing
}

#[cfg(target_os = "macos")]
pub fn hold(mark: &Path) -> Result<(), String> {
    std::fs::write(mark, std::process::id().to_string())
        .map_err(|error| format!("couldn't start it: {error}"))?;
    if mac::helper(mark) && mac::settle(false) {
        return Ok(());
    }
    let _ = std::fs::remove_file(mark);
    Err("that needs your mac password, so the lid setting is unchanged".to_string())
}

#[cfg(target_os = "linux")]
pub fn hold(_: &Path) -> Result<(), String> {
    keep(&LID, true, || linux::inhibit("handle-lid-switch"));
    std::thread::sleep(std::time::Duration::from_millis(400));
    if LID.lock().is_ok_and(|mut held| alive(&mut held)) {
        Ok(())
    } else {
        Err(
            "logind wouldn't let sombre hold the lid switch, so the lid setting is unchanged"
                .to_string(),
        )
    }
}

#[cfg(windows)]
pub fn hold(mark: &Path) -> Result<(), String> {
    std::fs::write(mark, std::process::id().to_string())
        .map_err(|error| format!("couldn't start it: {error}"))?;
    if windows::elevate(&watcher(std::process::id(), mark))
        && windows::wait(|| windows::actions() == Some((0, 0)))
    {
        return Ok(());
    }
    let _ = std::fs::remove_file(mark);
    Err("that needs administrator approval, so the lid setting is unchanged".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn hold(_: &Path) -> Result<(), String> {
    Err(
        "keeping it running with the lid closed only works on macos, linux, and windows"
            .to_string(),
    )
}

#[cfg(target_os = "macos")]
pub fn free(mark: &Path) -> Result<(), String> {
    let managed = mac::alive(mark);
    let _ = std::fs::remove_file(mark);
    if (managed && mac::settle(true)) || (mac::reset() && mac::settle(true)) {
        return Ok(());
    }
    Err("couldn't turn lid sleep back on. run sudo pmset -a disablesleep 0".to_string())
}

#[cfg(target_os = "linux")]
pub fn free(_: &Path) -> Result<(), String> {
    keep(&LID, false, || None);
    Ok(())
}

#[cfg(windows)]
pub fn free(mark: &Path) -> Result<(), String> {
    let managed = windows::managed(mark);
    let _ = std::fs::remove_file(mark);
    let back = || !saved(mark).exists();
    if managed && windows::wait(back) {
        return Ok(());
    }
    if back() || (windows::elevate(&restorer(mark)) && windows::wait(back)) {
        return Ok(());
    }
    Err("couldn't turn lid sleep back on without administrator approval".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn free(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_lid_sleep_turned_off() {
        assert!(disabled(
            " sleep 0 (sleep prevented by caffeinate)\n SleepDisabled\t\t1\n womp 1"
        ));
        assert!(!disabled(
            " sleep 0 (sleep prevented by caffeinate)\n womp 1"
        ));
        assert!(!disabled(" SleepDisabled\t\t0"));
    }

    #[test]
    fn reads_power_from_pmset() {
        assert_eq!(
            reading(
                "Now drawing from 'AC Power'\n -InternalBattery-0 (id=35520611)\t100%; charged; 0:00 remaining present: true"
            ),
            Some("plugged in, 100%".to_string())
        );
        assert_eq!(
            reading(
                "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t84%; discharging; 5:32 remaining present: true"
            ),
            Some("on battery, 84%".to_string())
        );
        assert_eq!(
            reading("Now drawing from 'AC Power'"),
            Some("plugged in".to_string())
        );
        assert_eq!(reading(""), None);
    }

    #[test]
    fn holds_sleep_off_on_linux_until_sombre_exits() {
        let awake = inhibitor("idle:sleep", 42);
        assert_eq!(awake[0], "--what=idle:sleep");
        assert_eq!(awake[6], WATCH);
        assert_eq!(awake.last().map(String::as_str), Some("42"));
        let lid = inhibitor("handle-lid-switch", 42);
        assert_eq!(lid[6], GUARD);
        assert!(GUARD.contains("Discharging") && GUARD.contains("-lt 20"));
    }

    #[test]
    fn reads_linux_power_supplies() {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let write = |name: &str, files: &[(&str, &str)]| {
            let folder = root.path().join(name);
            std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
            for (file, text) in files {
                std::fs::write(folder.join(file), text).unwrap_or_else(|error| panic!("{error}"));
            }
        };
        assert_eq!(supplies(root.path()), None);
        write("AC", &[("type", "Mains\n"), ("online", "0\n")]);
        write(
            "BAT0",
            &[
                ("type", "Battery\n"),
                ("status", "Discharging\n"),
                ("capacity", "84\n"),
            ],
        );
        assert_eq!(supplies(root.path()), Some("on battery, 84%".to_string()));
        write("AC", &[("online", "1\n")]);
        assert_eq!(supplies(root.path()), Some("plugged in, 84%".to_string()));
        assert_eq!(supplies(&root.path().join("missing")), None);
    }

    #[test]
    fn reads_windows_battery_status() {
        assert_eq!(charged("1 63\r\n"), Some("on battery, 63%".to_string()));
        assert_eq!(charged("2 100"), Some("plugged in, 100%".to_string()));
        assert_eq!(charged("6"), Some("plugged in".to_string()));
        assert_eq!(charged(""), None);
        assert!(KEEPALIVE.contains("0x80000001") && KEEPALIVE.contains("PID"));
        assert!(CHARGE.contains("Win32_Battery"));
        let script = "Write-Output 'hi'";
        let decoded = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(encoded(script))
                .unwrap_or_default()
        };
        let back: Vec<u16> = decoded
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16_lossy(&back), script);
    }

    const POWERCFG: &str = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)
  Subgroup GUID: 4f971e89-eebd-4455-a8de-9e59040e7347  (Power buttons and lid)
    Power Setting GUID: 5ca83367-6e45-459f-a27b-476b1d01c936  (Lid close action)
      Possible Setting Index: 000
      Possible Setting Friendly Name: Do nothing
      Possible Setting Index: 001
      Possible Setting Friendly Name: Sleep
    Current AC Power Setting Index: 0x00000001
    Current DC Power Setting Index: 0x00000003
";

    #[test]
    fn reads_the_lid_action_in_any_language() {
        assert_eq!(actions(POWERCFG), Some((1, 3)));
        let german = POWERCFG
            .replace(
                "Current AC Power Setting Index",
                "Index der aktuellen Wechselstromeinstellung",
            )
            .replace(
                "Current DC Power Setting Index",
                "Index der aktuellen Gleichstromeinstellung",
            );
        assert_eq!(actions(&german), Some((1, 3)));
        assert_eq!(actions("Power Scheme GUID: 381b4222"), None);
    }

    #[test]
    fn the_lid_helper_saves_watches_and_restores() {
        let mark = Path::new("/home/o'neil/.sombre/lid");
        let script = watcher(4242, mark);
        assert!(!script.contains("__"));
        assert!(script.contains("Get-Process -Id 4242"));
        assert!(script.contains("'/home/o''neil/.sombre/lid'"));
        assert!(script.contains("'/home/o''neil/.sombre/lid.lid'"));
        let save = script.find("Set-Content").unwrap_or(usize::MAX);
        let off = script.find("Set-Lid 0 0").unwrap_or(0);
        let back = script.rfind("Set-Lid $saved[0] $saved[1]").unwrap_or(0);
        assert!(save < off && off < back);
        assert!(script.contains("EstimatedChargeRemaining -lt 20"));
        assert!(
            restorer(mark).contains("Get-Content -LiteralPath '/home/o''neil/.sombre/lid.lid'")
        );
        assert!(elevated("Write-Output 'hi'").contains("-Verb RunAs"));
        assert_eq!(literal(Path::new("a\u{2019}b")), "a\u{2019}\u{2019}b");
    }

    #[cfg(windows)]
    #[test]
    fn keeps_running_with_the_lid_closed_on_windows() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let Some(before) = windows::actions() else {
            return;
        };
        let folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let mark = folder.path().join("lid");
        assert_eq!(hold(&mark), Ok(()));
        assert_eq!(lid(&mark), Lid::Running);
        assert_eq!(windows::actions(), Some((0, 0)));
        assert_eq!(free(&mark), Ok(()));
        assert_eq!(windows::actions(), Some(before));
        assert!(!saved(&mark).exists());
    }
}

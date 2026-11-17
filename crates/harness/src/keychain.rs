#[cfg(windows)]
use std::collections::HashMap;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::io::Write;
#[cfg(not(windows))]
use std::process::{Command, Stdio};

const SERVICE: &str = "sombre";
#[cfg(not(windows))]
const TOOL: &str = "/usr/bin/security";

pub fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "macos")]
pub fn save(account: &str, secret: &str) -> Result<(), String> {
    let mut child = Command::new(TOOL)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("couldn't open the keychain: {error}"))?;
    if let Some(mut input) = child.stdin.take() {
        writeln!(
            input,
            "add-generic-password -U -s {} -a {} -w {}",
            quote(SERVICE),
            quote(account),
            quote(secret)
        )
        .map_err(|error| format!("couldn't write to the keychain: {error}"))?;
    }
    child
        .wait()
        .map_err(|error| format!("the keychain didn't answer: {error}"))?;
    check(account, secret)
}

#[cfg(target_os = "linux")]
pub fn save(account: &str, secret: &str) -> Result<(), String> {
    let mut child = Command::new("secret-tool")
        .args([
            "store", "--label", "sombre", "service", SERVICE, "account", account,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "there's no keyring here (secret-tool isn't installed)".to_string())?;
    if let Some(mut input) = child.stdin.take() {
        write!(input, "{secret}")
            .map_err(|error| format!("couldn't write to the keyring: {error}"))?;
    }
    child
        .wait()
        .map_err(|error| format!("the keyring didn't answer: {error}"))?;
    check(account, secret)
}

#[cfg(windows)]
fn entry(account: &str) -> Option<keyring_core::Entry> {
    use keyring_core::api::CredentialStoreApi;
    let modifiers = HashMap::from([("persistence", "Local")]);
    windows_native_keyring_store::Store::new()
        .ok()?
        .build(SERVICE, account, Some(&modifiers))
        .ok()
}

#[cfg(windows)]
pub fn save(account: &str, secret: &str) -> Result<(), String> {
    entry(account)
        .ok_or("couldn't open the windows credential manager")?
        .set_password(secret)
        .map_err(|error| format!("couldn't save the key in the credential manager: {error}"))?;
    check(account, secret)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn save(_: &str, _: &str) -> Result<(), String> {
    Err("there's no keychain support on this system yet".to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn check(account: &str, secret: &str) -> Result<(), String> {
    if load(account).as_deref() == Some(secret) {
        Ok(())
    } else {
        Err("the keychain didn't keep the key".to_string())
    }
}

#[cfg(windows)]
pub fn load(account: &str) -> Option<String> {
    entry(account)?
        .get_password()
        .ok()
        .filter(|key| !key.trim().is_empty())
}

#[cfg(not(windows))]
pub fn load(account: &str) -> Option<String> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new(TOOL);
        command.args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"]);
        command
    } else if cfg!(target_os = "linux") {
        let mut command = Command::new("secret-tool");
        command.args(["lookup", "service", SERVICE, "account", account]);
        command
    } else {
        return None;
    };
    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!key.is_empty()).then_some(key)
}

#[cfg(windows)]
pub fn forget(account: &str) {
    if let Some(entry) = entry(account) {
        let _ = entry.delete_credential();
    }
}

#[cfg(not(windows))]
pub fn forget(account: &str) {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new(TOOL);
        command.args(["delete-generic-password", "-s", SERVICE, "-a", account]);
        command
    } else if cfg!(target_os = "linux") {
        let mut command = Command::new("secret-tool");
        command.args(["clear", "service", SERVICE, "account", account]);
        command
    } else {
        return;
    };
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn keeps_keys_in_the_windows_credential_manager() {
        let account = format!("test-{}", std::process::id());
        assert_eq!(save(&account, "sk-test-123"), Ok(()));
        assert_eq!(load(&account).as_deref(), Some("sk-test-123"));
        assert_eq!(save(&account, "sk-test-456"), Ok(()));
        assert_eq!(load(&account).as_deref(), Some("sk-test-456"));
        forget(&account);
        assert_eq!(load(&account), None);
    }

    #[test]
    fn quotes_keys_for_the_security_shell() {
        assert_eq!(quote("abc-123"), "\"abc-123\"");
        assert_eq!(quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }
}

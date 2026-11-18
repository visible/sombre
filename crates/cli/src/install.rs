use std::process::{Command, Stdio};

use serde_json::Value;
use sombre_core::Agent;
use sombre_harness::process;

use crate::view;

pub fn program(agent: Agent) -> Option<&'static str> {
    match agent {
        Agent::Claude => Some("claude"),
        Agent::Codex => Some("codex"),
        Agent::Sombre => None,
    }
}

pub fn installed(agent: Agent) -> bool {
    program(agent).is_none_or(|name| process::find(name).is_some())
}

pub fn recipe(
    agent: Agent,
    windows: bool,
    mac: bool,
    home: &str,
    has: impl Fn(&str) -> bool,
) -> Option<Vec<String>> {
    let words = |line: &[&str]| line.iter().map(|word| (*word).to_string()).collect();
    match agent {
        Agent::Claude if windows => Some(words(&[
            "powershell",
            "-NoProfile",
            "-Command",
            "irm https://claude.ai/install.ps1 | iex",
        ])),
        Agent::Claude => Some(words(&[
            "sh",
            "-c",
            "curl -fsSL https://claude.ai/install.sh | bash",
        ])),
        Agent::Codex if mac && has("brew") => Some(words(&["brew", "install", "--cask", "codex"])),
        Agent::Codex if has("npm") && windows => {
            Some(words(&["npm", "install", "-g", "@openai/codex"]))
        }
        Agent::Codex if has("npm") => Some(words(&[
            "npm",
            "install",
            "-g",
            "--prefix",
            &format!("{home}/.local"),
            "@openai/codex",
        ])),
        Agent::Codex if has("bun") => Some(words(&["bun", "add", "-g", "@openai/codex"])),
        Agent::Codex | Agent::Sombre => None,
    }
}

pub fn shown(recipe: &[String]) -> String {
    match recipe {
        [_, flag, script] if flag == "-c" => script.clone(),
        [_, _, flag, script] if flag == "-Command" => script.clone(),
        _ => recipe.join(" "),
    }
}

fn signed(agent: Agent) -> bool {
    let Some(path) = program(agent).and_then(process::find) else {
        return false;
    };
    match agent {
        Agent::Claude => Command::new(path)
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .and_then(|output| serde_json::from_slice::<Value>(&output.stdout).ok())
            .is_some_and(|status| status["loggedIn"] == true),
        Agent::Codex => process::quiet(Command::new(path).args(["login", "status"])),
        Agent::Sombre => true,
    }
}

fn run(line: &[String]) -> bool {
    let Some((first, rest)) = line.split_first() else {
        return false;
    };
    let program = process::find(first).map_or_else(|| first.into(), |path| path.into_os_string());
    crate::pane::release();
    let hushed = crate::dock::aloud();
    let worked = Command::new(program)
        .args(rest)
        .status()
        .is_ok_and(|status| status.success());
    if hushed {
        crate::dock::hush();
    }
    worked
}

pub fn ready(agent: Agent, name: &str) -> bool {
    if !installed(agent) {
        let home = process::home()
            .map(|home| home.display().to_string())
            .unwrap_or_default();
        let Some(line) = recipe(
            agent,
            cfg!(windows),
            cfg!(target_os = "macos"),
            &home,
            |tool| process::find(tool).is_some(),
        ) else {
            view::failed(&format!(
                "{name} isn't installed and there's no npm, bun, or homebrew to install it with. install node from nodejs.org, then try again"
            ));
            return false;
        };
        let agree = crate::menu::confirm(format!(
            "{name} isn't installed. install it now with `{}`?",
            shown(&line)
        ))
        .initial_value(true)
        .interact()
        .unwrap_or(false);
        if !agree {
            return false;
        }
        println!();
        if !run(&line) || !installed(agent) {
            view::failed(&format!(
                "{name} didn't finish installing, or it isn't on your PATH yet. open a new terminal and try again"
            ));
            return false;
        }
        view::note(&format!("installed {name}"));
    }
    if signed(agent) {
        return true;
    }
    let agree = crate::menu::confirm(format!("{name} needs you to sign in. do it now?"))
        .initial_value(true)
        .interact()
        .unwrap_or(false);
    if !agree {
        view::note(&format!("sign in later and {name} will work in sombre"));
        return true;
    }
    let line: Vec<String> = match agent {
        Agent::Claude => vec![
            "claude".to_string(),
            "auth".to_string(),
            "login".to_string(),
        ],
        _ => vec!["codex".to_string(), "login".to_string()],
    };
    println!();
    if run(&line) && signed(agent) {
        view::note(&format!("signed in to {name}"));
    } else {
        view::failed(&format!(
            "{name} still isn't signed in. run `{}` to try again",
            line.join(" ")
        ));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_claude_code_with_the_official_script() {
        let unix = recipe(Agent::Claude, false, true, "/home/me", |_| false).unwrap_or_default();
        assert_eq!(
            shown(&unix),
            "curl -fsSL https://claude.ai/install.sh | bash"
        );
        let windows = recipe(Agent::Claude, true, false, "/home/me", |_| false).unwrap_or_default();
        assert_eq!(windows[0], "powershell");
        assert_eq!(shown(&windows), "irm https://claude.ai/install.ps1 | iex");
    }

    #[test]
    fn picks_the_best_way_to_install_codex() {
        let all = |_: &str| true;
        assert_eq!(
            recipe(Agent::Codex, false, true, "/home/me", all).map(|line| shown(&line)),
            Some("brew install --cask codex".to_string())
        );
        assert_eq!(
            recipe(Agent::Codex, false, false, "/home/me", all).map(|line| shown(&line)),
            Some("npm install -g --prefix /home/me/.local @openai/codex".to_string())
        );
        assert_eq!(
            recipe(Agent::Codex, true, false, "/home/me", |tool| tool == "bun")
                .map(|line| shown(&line)),
            Some("bun add -g @openai/codex".to_string())
        );
        assert_eq!(
            recipe(Agent::Codex, false, false, "/home/me", |_| false),
            None
        );
    }

    #[test]
    fn sombre_is_always_installed() {
        assert!(installed(Agent::Sombre));
        assert_eq!(program(Agent::Sombre), None);
    }
}

pub mod claude;
pub mod clock;
pub mod codex;
pub mod convert;
pub mod event;
pub mod handoff;
mod index;
pub mod ledger;
pub mod pasted;
pub mod purge;
pub mod scrub;
pub mod server;
mod session;
pub mod sombre;

use std::env;
use std::path::PathBuf;

pub use event::Event;
pub use session::{Agent, Lookup, Model, Session, ago, find, here, latest, named, now, title};

#[derive(Clone, Debug)]
pub struct Roots {
    pub claude: PathBuf,
    pub codex: PathBuf,
    pub sombre: PathBuf,
}

impl Roots {
    pub fn detect() -> Option<Self> {
        let home = env::home_dir().filter(|home| !home.as_os_str().is_empty())?;
        Some(Self {
            claude: env::var_os("CLAUDE_CONFIG_DIR")
                .map_or_else(|| home.join(".claude"), PathBuf::from),
            codex: env::var_os("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from),
            sombre: env::var_os("SOMBRE_HOME").map_or_else(|| home.join(".sombre"), PathBuf::from),
        })
    }

    pub fn models(&self, agent: Agent) -> Vec<Model> {
        match agent {
            Agent::Claude => claude::models(),
            Agent::Codex => codex::models(&self.codex),
            Agent::Sombre => Vec::new(),
        }
    }

    pub fn model(&self, agent: Agent) -> Option<String> {
        match agent {
            Agent::Claude => claude::model(&self.claude),
            Agent::Codex => codex::model(&self.codex),
            Agent::Sombre => None,
        }
    }
}

pub fn transcript(session: &Session) -> Vec<Event> {
    match session.agent {
        Agent::Claude => claude::transcript(&session.path),
        Agent::Codex => codex::transcript(&session.path),
        Agent::Sombre => sombre::transcript(&session.path),
    }
}

const WINDOW: u64 = 4 * 1024 * 1024;

pub fn recent(session: &Session, prompts: usize) -> (Vec<Event>, bool) {
    if session.agent == Agent::Sombre {
        return (transcript(session), true);
    }
    let mut window = WINDOW;
    loop {
        let (lines, whole) = session::tail(&session.path, window);
        let events = match session.agent {
            Agent::Codex => codex::events(&session.path, lines.into_iter()),
            Agent::Claude | Agent::Sombre => claude::events(lines.into_iter()),
        };
        let asked = events
            .iter()
            .filter(|event| matches!(event, Event::Prompt(_)))
            .count();
        if whole || asked > prompts {
            return (events, whole);
        }
        window = window.saturating_mul(4);
    }
}

pub fn scan(roots: &Roots) -> Vec<Session> {
    let mut sessions = index::scan(roots);
    sessions.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_huge_chat_opens_from_its_end_with_the_same_recent_history() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("s1.jsonl");
        let long = "x".repeat(300_000);
        let lines: Vec<String> = (0..30)
            .flat_map(|turn| {
                [
                    format!(r#"{{"type":"user","cwd":"/work","message":{{"role":"user","content":"question {turn}"}}}}"#),
                    format!(r#"{{"type":"assistant","cwd":"/work","message":{{"role":"assistant","model":"claude-opus-5-5","content":[{{"type":"text","text":"answer {turn} {long}"}}]}}}}"#),
                ]
            })
            .collect();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap_or_else(|error| panic!("{error}"));
        let session = Session {
            agent: Agent::Claude,
            cwd: PathBuf::from("/work"),
            id: "s1".to_string(),
            path,
            title: String::new(),
            updated: 0,
        };
        let full = transcript(&session);
        let (tail, whole) = recent(&session, 3);
        assert!(!whole);
        let asked = tail
            .iter()
            .filter(|event| matches!(event, Event::Prompt(_)))
            .count();
        assert!((4..30).contains(&asked), "{asked} prompts");
        assert_eq!(&full[full.len() - tail.len()..], tail.as_slice());
        let (all, whole) = recent(&session, 100);
        assert!(whole);
        assert_eq!(all, full);
    }
}

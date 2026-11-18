pub mod claude;
pub mod clock;
pub mod codex;
pub mod convert;
pub mod event;
pub mod handoff;
mod index;
pub mod ledger;
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

pub fn scan(roots: &Roots) -> Vec<Session> {
    let mut sessions = index::scan(roots);
    sessions.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
    sessions
}

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Roots;
use crate::session::Agent;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct File {
    pub path: String,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Record {
    pub turn: usize,
    pub from: String,
    pub to: String,
    pub files: Vec<File>,
}

fn safe(session: &str) -> bool {
    !session.is_empty()
        && session.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

pub fn path(roots: &Roots, agent: Agent, session: &str) -> Option<PathBuf> {
    safe(session).then(|| {
        roots
            .sombre
            .join("changes")
            .join(format!("{}-{session}.jsonl", agent.name()))
    })
}

pub fn append(roots: &Roots, agent: Agent, session: &str, record: &Record) -> Result<(), String> {
    let file = path(roots, agent, session).ok_or("that chat id can't be saved")?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("couldn't save the changes: {error}"))?;
    }
    let line = serde_json::to_string(record).map_err(|error| error.to_string())?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .and_then(|mut handle| writeln!(handle, "{line}"))
        .map_err(|error| format!("couldn't save the changes: {error}"))
}

pub fn read(roots: &Roots, agent: Agent, session: &str) -> Vec<Record> {
    path(roots, agent, session)
        .and_then(|file| fs::read_to_string(file).ok())
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(dir: &std::path::Path) -> Roots {
        Roots {
            claude: dir.join("claude"),
            codex: dir.join("codex"),
            sombre: dir.join("sombre"),
        }
    }

    fn record(turn: usize) -> Record {
        Record {
            files: vec![File {
                added: Some(2),
                path: "src/app.ts".to_string(),
                removed: None,
            }],
            from: "a1".to_string(),
            to: "b2".to_string(),
            turn,
        }
    }

    #[test]
    fn writes_records_and_reads_them_back() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        assert!(read(&roots, Agent::Claude, "s1").is_empty());
        for turn in [1, 3] {
            append(&roots, Agent::Claude, "s1", &record(turn))
                .unwrap_or_else(|error| panic!("{error}"));
        }
        assert_eq!(read(&roots, Agent::Claude, "s1"), [record(1), record(3)]);
        assert!(read(&roots, Agent::Codex, "s1").is_empty());
    }

    #[test]
    fn refuses_ids_that_could_leave_the_folder() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        assert!(append(&roots, Agent::Claude, "../x", &record(1)).is_err());
        assert_eq!(path(&roots, Agent::Claude, ""), None);
    }
}

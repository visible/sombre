use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::session::{Agent, Session};
use crate::{Roots, claude, codex, sombre};

const VERSION: u32 = 2;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    files: HashMap<String, Entry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
    stamp: u64,
    size: u64,
    session: Option<Session>,
}

pub fn file(roots: &Roots) -> PathBuf {
    roots.sombre.join("index.json")
}

fn load(path: &Path) -> Index {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
        .filter(|index| index.version == VERSION)
        .unwrap_or_default()
}

fn save(path: &Path, index: &Index) {
    let Ok(bytes) = serde_json::to_vec(index) else {
        return;
    };
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let temporary = path.with_extension("json.tmp");
    if fs::write(&temporary, bytes).is_ok() {
        let _ = fs::rename(&temporary, path);
    }
}

pub fn scan(roots: &Roots) -> Vec<Session> {
    let path = file(roots);
    let old = load(&path);
    let listed: Vec<(Agent, PathBuf)> = claude::files(&roots.claude)
        .into_iter()
        .map(|file| (Agent::Claude, file))
        .chain(
            codex::files(&roots.codex)
                .into_iter()
                .map(|file| (Agent::Codex, file)),
        )
        .chain(
            sombre::files(&roots.sombre)
                .into_iter()
                .map(|file| (Agent::Sombre, file)),
        )
        .collect();
    let mut fresh = Index {
        files: HashMap::with_capacity(listed.len()),
        version: VERSION,
    };
    let mut prompts: Option<HashMap<String, String>> = None;
    let mut changed = false;
    let mut sessions = Vec::with_capacity(listed.len());
    for (agent, file) in listed {
        let Ok(meta) = fs::metadata(&file) else {
            continue;
        };
        let stamp = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|time| u64::try_from(time.as_nanos()).ok())
            .unwrap_or_default();
        let size = meta.len();
        let key = file.to_string_lossy().into_owned();
        let entry = match old.files.get(&key) {
            Some(entry) if entry.stamp == stamp && entry.size == size => entry.clone(),
            _ => {
                changed = true;
                let session = match (agent, old.files.get(&key)) {
                    (
                        Agent::Claude,
                        Some(Entry {
                            size: before,
                            session: Some(session),
                            ..
                        }),
                    ) if size > *before => claude::grown(&file, session, *before),
                    (Agent::Claude, _) => claude::read(&file),
                    (Agent::Codex, _) => {
                        let prompts = prompts.get_or_insert_with(|| codex::prompts(&roots.codex));
                        codex::read(&file, prompts)
                    }
                    (Agent::Sombre, _) => sombre::read(&file),
                };
                Entry {
                    session,
                    size,
                    stamp,
                }
            }
        };
        if let Some(mut session) = entry.session.clone() {
            session.path = file;
            sessions.push(session);
        }
        fresh.files.insert(key, entry);
    }
    if changed || fresh.files.len() != old.files.len() {
        save(&path, &fresh);
    }
    let names = codex::names(&roots.codex);
    for session in &mut sessions {
        if session.agent == Agent::Codex
            && let Some(name) = names.get(&session.id)
        {
            session.title.clone_from(name);
        }
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn roots(dir: &Path) -> Roots {
        Roots {
            claude: dir.join("claude"),
            codex: dir.join("codex"),
            sombre: dir.join("sombre"),
        }
    }

    fn chat(path: &Path, prompt: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
        }
        let mut file = fs::File::create(path).unwrap_or_else(|error| panic!("{error}"));
        writeln!(
            file,
            r#"{{"type":"user","cwd":"/work/atlas","message":{{"role":"user","content":"{prompt}"}}}}"#
        )
        .unwrap_or_else(|error| panic!("{error}"));
    }

    #[test]
    fn reads_new_chats_and_saves_an_index() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        chat(
            &roots.claude.join("projects/-work-atlas/a1.jsonl"),
            "first chat",
        );
        let found = scan(&roots);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "first chat");
        assert!(file(&roots).exists());
    }

    #[test]
    fn reuses_the_index_for_files_that_did_not_change() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let path = roots.claude.join("projects/-work-atlas/a1.jsonl");
        chat(&path, "first chat");
        scan(&roots);
        let mut index = load(&file(&roots));
        for entry in index.files.values_mut() {
            if let Some(session) = entry.session.as_mut() {
                session.title = "from the index".to_string();
            }
        }
        save(&file(&roots), &index);
        assert_eq!(scan(&roots)[0].title, "from the index");
        chat(&path, "short");
        assert_eq!(scan(&roots)[0].title, "short");
    }

    #[test]
    fn a_chat_that_grew_keeps_its_entry_and_picks_up_a_rename() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let path = roots.claude.join("projects/-work-atlas/a1.jsonl");
        chat(&path, "first chat");
        assert_eq!(scan(&roots)[0].title, "first chat");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap_or_else(|error| panic!("{error}"));
        writeln!(
            file,
            r#"{{"type":"assistant","cwd":"/work/atlas","message":{{"role":"assistant","content":"done"}}}}"#
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(scan(&roots)[0].title, "first chat");
        writeln!(file, r#"{{"type":"custom-title","customTitle":"renamed"}}"#)
            .unwrap_or_else(|error| panic!("{error}"));
        let found = scan(&roots);
        assert_eq!(found[0].title, "renamed");
        assert_eq!(found[0].path, path);
    }

    #[test]
    fn forgets_files_that_were_deleted() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let path = roots.claude.join("projects/-work-atlas/a1.jsonl");
        chat(&path, "first chat");
        scan(&roots);
        fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        assert!(scan(&roots).is_empty());
        assert!(load(&file(&roots)).files.is_empty());
    }

    #[test]
    fn starts_over_when_the_index_is_broken() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        chat(
            &roots.claude.join("projects/-work-atlas/a1.jsonl"),
            "first chat",
        );
        fs::create_dir_all(&roots.sombre).unwrap_or_else(|error| panic!("{error}"));
        fs::write(file(&roots), "not json").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(scan(&roots).len(), 1);
    }
}

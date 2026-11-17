use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::convert::{Item, Turn, turns};
use crate::session::Agent;
use crate::{Roots, claude, clock, codex, sombre};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Copy {
    pub agent: Agent,
    pub session: String,
    pub home: PathBuf,
    pub synced: usize,
    pub base: usize,
    pub archived: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub cwd: PathBuf,
    pub log: Vec<Item>,
    pub copies: Vec<Copy>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub session: String,
    pub path: PathBuf,
    pub carried: usize,
}

pub struct Versions {
    pub claude: String,
    pub codex: String,
    pub listed: bool,
}

pub type Shelf<'a> = &'a dyn Fn(bool, &str) -> Result<(), String>;

fn threads(roots: &Roots) -> PathBuf {
    roots.sombre.join("threads")
}

fn shelf(roots: &Roots, agent: Agent, session: &str) -> PathBuf {
    let name = format!("{session}.jsonl");
    match agent {
        Agent::Claude => roots.claude.join("sombre-archive").join(name),
        _ => roots.sombre.join("archive").join(agent.name()).join(name),
    }
}

fn locate(roots: &Roots, agent: Agent, session: &str) -> Option<PathBuf> {
    match agent {
        Agent::Claude => claude::find(&roots.claude, session),
        Agent::Codex => codex::find(&roots.codex, session),
        Agent::Sombre => Some(sombre::folder(&roots.sombre).join(format!("{session}.jsonl")))
            .filter(|path| path.is_file()),
    }
}

fn timeline(agent: Agent, path: &Path) -> Vec<Item> {
    match agent {
        Agent::Claude => claude::timeline(path),
        Agent::Codex => codex::timeline(path),
        Agent::Sombre => sombre::timeline(path),
    }
}

pub fn load(roots: &Roots, agent: Agent, session: &str) -> Option<Thread> {
    fs::read_dir(threads(roots))
        .ok()?
        .flatten()
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<Thread>(&bytes).ok())
        .find(|thread| {
            thread
                .copies
                .iter()
                .any(|copy| copy.agent == agent && copy.session == session)
        })
}

pub fn forget(
    roots: &Roots,
    folder: &Path,
    codex: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(threads(roots)) else {
        return Ok(());
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let Some(thread) = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Thread>(&bytes).ok())
        else {
            continue;
        };
        if thread.cwd != folder {
            continue;
        }
        for copy in thread.copies.iter().filter(|copy| copy.archived) {
            if copy.agent == Agent::Codex {
                codex(&copy.session)?;
            } else {
                let kept = shelf(roots, copy.agent, &copy.session);
                if kept.exists() {
                    fs::remove_file(&kept)
                        .map_err(|error| format!("couldn't delete {}: {error}", kept.display()))?;
                }
            }
        }
        fs::remove_file(&path)
            .map_err(|error| format!("couldn't delete {}: {error}", path.display()))?;
    }
    Ok(())
}

fn save(roots: &Roots, thread: &Thread) -> Result<(), String> {
    let folder = threads(roots);
    fs::create_dir_all(&folder)
        .map_err(|error| format!("couldn't save the chat's history: {error}"))?;
    let bytes = serde_json::to_vec(thread).map_err(|error| error.to_string())?;
    let path = folder.join(format!("{}.json", thread.id));
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)
        .and_then(|()| fs::rename(&temporary, &path))
        .map_err(|error| format!("couldn't save the chat's history: {error}"))
}

fn shift(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("couldn't make {}: {error}", parent.display()))?;
    }
    fs::rename(from, to).map_err(|error| format!("couldn't move {}: {error}", from.display()))
}

fn append(path: &Path, lines: &[String]) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("couldn't make {}: {error}", parent.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{}", lines.join("\n")))
        .map_err(|error| format!("couldn't write {}: {error}", path.display()))
}

fn opening(mut written: Vec<Turn>) -> Vec<Turn> {
    if matches!(written.first(), Some(Turn::Agent(_))) {
        written.insert(
            0,
            Turn::User("continuing this chat from another agent".to_string()),
        );
    }
    written
}

fn archive(roots: &Roots, copy: &Copy, codex: Shelf) -> Result<(), String> {
    match copy.agent {
        Agent::Codex => codex(true, &copy.session),
        agent => shift(&copy.home, &shelf(roots, agent, &copy.session)),
    }
}

fn restore(roots: &Roots, copy: &Copy, codex: Shelf) -> Result<PathBuf, String> {
    match copy.agent {
        Agent::Codex => {
            let live = roots.codex.join("sessions");
            if let Some(path) =
                codex::find(&roots.codex, &copy.session).filter(|path| path.starts_with(&live))
            {
                return Ok(path);
            }
            codex(false, &copy.session)?;
            codex::find(&roots.codex, &copy.session)
                .ok_or_else(|| "codex unarchived the chat but sombre couldn't find it".to_string())
        }
        agent => {
            let kept = shelf(roots, agent, &copy.session);
            if !kept.is_file() && copy.home.is_file() {
                return Ok(copy.home.clone());
            }
            if !kept.is_file() {
                return Err(format!("the archived {} copy is missing", agent.name()));
            }
            shift(&kept, &copy.home)?;
            Ok(copy.home.clone())
        }
    }
}

fn create(
    roots: &Roots,
    agent: Agent,
    cwd: &Path,
    log: &[Item],
    versions: &Versions,
) -> Result<(String, PathBuf), String> {
    let written = opening(turns(log));
    if written.is_empty() {
        return Err("there's nothing in this chat to move yet".to_string());
    }
    let session = clock::uuid();
    let (path, lines) = match agent {
        Agent::Claude => (
            claude::folder(&roots.claude, cwd).join(format!("{session}.jsonl")),
            claude::lines(
                &written,
                &session,
                cwd,
                &versions.claude,
                None,
                versions.listed,
            ),
        ),
        Agent::Codex => (
            codex::place(&roots.codex, &session, clock::now()),
            codex::lines(
                &written,
                &session,
                cwd,
                &versions.codex,
                true,
                versions.listed,
            ),
        ),
        Agent::Sombre => (
            sombre::folder(&roots.sombre).join(format!("{session}.jsonl")),
            sombre::lines(&written, &session, cwd, true),
        ),
    };
    append(&path, &lines)?;
    Ok((session, path))
}

fn extend(
    copy: &Copy,
    path: &Path,
    missing: &[Item],
    cwd: &Path,
    versions: &Versions,
) -> Result<(), String> {
    let written = turns(missing);
    let lines = match copy.agent {
        Agent::Claude => claude::lines(
            &written,
            &copy.session,
            cwd,
            &versions.claude,
            claude::last(path),
            versions.listed,
        ),
        Agent::Codex => codex::lines(
            &written,
            &copy.session,
            cwd,
            &versions.codex,
            false,
            versions.listed,
        ),
        Agent::Sombre => sombre::lines(&written, &copy.session, cwd, false),
    };
    append(path, &lines)
}

pub fn hand(
    roots: &Roots,
    from: Agent,
    session: &str,
    to: Agent,
    cwd: &Path,
    versions: &Versions,
    codex: Shelf,
) -> Result<Moved, String> {
    let source = locate(roots, from, session)
        .ok_or_else(|| format!("couldn't find this {} chat on disk", from.name()))?;
    let mut thread = load(roots, from, session).unwrap_or_else(|| Thread {
        copies: vec![Copy {
            agent: from,
            archived: false,
            base: 0,
            home: source.clone(),
            session: session.to_string(),
            synced: 0,
        }],
        cwd: cwd.to_path_buf(),
        id: clock::uuid(),
        log: Vec::new(),
    });
    let position = thread
        .copies
        .iter()
        .position(|copy| copy.agent == from && copy.session == session)
        .ok_or_else(|| "this chat's history doesn't list it".to_string())?;
    let items = timeline(from, &source);
    let fresh: Vec<Item> = items
        .get(thread.copies[position].base..)
        .unwrap_or_default()
        .to_vec();
    thread.log.extend(fresh);
    if thread.log.is_empty() {
        return Err("there's nothing in this chat to move yet".to_string());
    }
    thread.copies[position].home = source;
    thread.copies[position].base = items.len();
    let target = thread.copies.iter().position(|copy| copy.agent == to);
    let (session, path, carried) = match target {
        Some(index) => {
            let copy = thread.copies[index].clone();
            let path = restore(roots, &copy, codex)?;
            let missing: Vec<Item> = thread.log.get(copy.synced..).unwrap_or_default().to_vec();
            extend(&copy, &path, &missing, cwd, versions)?;
            (copy.session, path, missing.len())
        }
        None => {
            let (session, path) = create(roots, to, cwd, &thread.log, versions)?;
            thread.copies.push(Copy {
                agent: to,
                archived: false,
                base: 0,
                home: path.clone(),
                session: session.clone(),
                synced: 0,
            });
            let carried = thread.log.len();
            (session, path, carried)
        }
    };
    let length = timeline(to, &path).len();
    if let Some(copy) = thread.copies.iter_mut().find(|copy| copy.agent == to) {
        copy.archived = false;
        copy.base = length;
        copy.home = path.clone();
        copy.synced = thread.log.len();
    }
    save(roots, &thread)?;
    archive(roots, &thread.copies[position], codex)?;
    thread.copies[position].archived = true;
    thread.copies[position].synced = thread.log.len();
    save(roots, &thread)?;
    Ok(Moved {
        carried,
        path,
        session,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(dir: &Path) -> Roots {
        Roots {
            claude: dir.join("claude"),
            codex: dir.join("codex"),
            sombre: dir.join("sombre"),
        }
    }

    fn versions() -> Versions {
        Versions {
            claude: "2.1.322".to_string(),
            codex: "0.167.0".to_string(),
            listed: true,
        }
    }

    fn fake(roots: &Roots) -> impl Fn(bool, &str) -> Result<(), String> + '_ {
        move |away: bool, id: &str| {
            let path = codex::find(&roots.codex, id).ok_or("missing")?;
            let name = path.file_name().ok_or("no name")?.to_os_string();
            let to = if away {
                roots.codex.join("archived_sessions").join(name)
            } else {
                roots
                    .codex
                    .join("sessions")
                    .join("2026")
                    .join("11")
                    .join("10")
                    .join(name)
            };
            shift(&path, &to)
        }
    }

    fn start(roots: &Roots, cwd: &Path) -> String {
        let written = [
            Turn::User("remember cobalt".to_string()),
            Turn::Agent("got it".to_string()),
        ];
        let path = claude::folder(&roots.claude, cwd).join("c1.jsonl");
        append(
            &path,
            &claude::lines(&written, "c1", cwd, "2.1.322", None, true),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        "c1".to_string()
    }

    #[test]
    fn moves_a_chat_to_another_agent_and_archives_the_original() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let cwd = Path::new("/work/atlas");
        let claude = start(&roots, cwd);
        let codex = fake(&roots);
        let moved = hand(
            &roots,
            Agent::Claude,
            &claude,
            Agent::Codex,
            cwd,
            &versions(),
            &codex,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(moved.carried, 2);
        assert_eq!(codex::timeline(&moved.path).len(), 2);
        assert_eq!(claude::find(&roots.claude, &claude), None);
        assert!(shelf(&roots, Agent::Claude, &claude).is_file());
    }

    #[test]
    fn switching_back_restores_the_original_and_adds_only_what_is_new() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let cwd = Path::new("/work/atlas");
        let claude = start(&roots, cwd);
        let codex = fake(&roots);
        let there = hand(
            &roots,
            Agent::Claude,
            &claude,
            Agent::Codex,
            cwd,
            &versions(),
            &codex,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let later = [
            Turn::User("now add tests".to_string()),
            Turn::Agent("added three tests".to_string()),
        ];
        append(
            &there.path,
            &codex::lines(&later, &there.session, cwd, "0.167.0", false, true),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let back = hand(
            &roots,
            Agent::Codex,
            &there.session,
            Agent::Claude,
            cwd,
            &versions(),
            &codex,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(back.session, claude);
        assert_eq!(back.carried, 2);
        let items = claude::timeline(&back.path);
        assert_eq!(items.len(), 4);
        assert_eq!(
            items[0],
            Item::Prompt {
                text: "remember cobalt".to_string()
            }
        );
        assert_eq!(
            items[3],
            Item::Reply {
                text: "added three tests".to_string()
            }
        );
        assert!(
            codex::find(&roots.codex, &there.session)
                .is_some_and(|path| path.starts_with(roots.codex.join("archived_sessions")))
        );
        let again = hand(
            &roots,
            Agent::Claude,
            &claude,
            Agent::Codex,
            cwd,
            &versions(),
            &codex,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(again.session, there.session);
        assert_eq!(again.carried, 0);
        assert_eq!(codex::timeline(&again.path).len(), 4);
    }

    #[test]
    fn hands_off_to_sombres_own_agent() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let cwd = Path::new("/work/atlas");
        let claude = start(&roots, cwd);
        let codex = fake(&roots);
        let moved = hand(
            &roots,
            Agent::Claude,
            &claude,
            Agent::Sombre,
            cwd,
            &versions(),
            &codex,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(sombre::timeline(&moved.path).len(), 2);
        assert_eq!(
            sombre::read(&moved.path).map(|session| session.title),
            Some("remember cobalt".to_string())
        );
    }

    fn moved(roots: &Roots, cwd: &Path) -> Moved {
        let claude = start(roots, cwd);
        let there = hand(
            roots,
            Agent::Claude,
            &claude,
            Agent::Codex,
            cwd,
            &versions(),
            &fake(roots),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let later = [
            Turn::User("now add tests".to_string()),
            Turn::Agent("added three tests".to_string()),
        ];
        append(
            &there.path,
            &codex::lines(&later, &there.session, cwd, "0.167.0", false, true),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        there
    }

    fn stuck(roots: &Roots) -> impl Fn(bool, &str) -> Result<(), String> + '_ {
        let working = fake(roots);
        move |away: bool, id: &str| {
            if away {
                Err("codex archive failed".to_string())
            } else {
                working(away, id)
            }
        }
    }

    #[test]
    fn a_handoff_that_fails_to_archive_can_be_retried_without_duplicates() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let cwd = Path::new("/work/atlas");
        let there = moved(&roots, cwd);
        let failed = hand(
            &roots,
            Agent::Codex,
            &there.session,
            Agent::Claude,
            cwd,
            &versions(),
            &stuck(&roots),
        );
        assert!(failed.is_err());
        let back = hand(
            &roots,
            Agent::Codex,
            &there.session,
            Agent::Claude,
            cwd,
            &versions(),
            &fake(&roots),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(claude::timeline(&back.path).len(), 4);
        assert!(
            codex::find(&roots.codex, &there.session)
                .is_some_and(|path| path.starts_with(roots.codex.join("archived_sessions")))
        );
        let thread = load(&roots, Agent::Claude, &back.session)
            .unwrap_or_else(|| panic!("the thread should be saved"));
        assert_eq!(thread.log.len(), 4);
    }

    #[test]
    fn a_first_handoff_that_fails_late_does_not_make_a_second_chat() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let cwd = Path::new("/work/atlas");
        let there = moved(&roots, cwd);
        let failed = hand(
            &roots,
            Agent::Codex,
            &there.session,
            Agent::Sombre,
            cwd,
            &versions(),
            &stuck(&roots),
        );
        assert!(failed.is_err());
        let own = hand(
            &roots,
            Agent::Codex,
            &there.session,
            Agent::Sombre,
            cwd,
            &versions(),
            &fake(&roots),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let thread = load(&roots, Agent::Sombre, &own.session)
            .unwrap_or_else(|| panic!("the thread should be saved"));
        let copies = thread
            .copies
            .iter()
            .filter(|copy| copy.agent == Agent::Sombre)
            .count();
        assert_eq!(copies, 1);
        assert_eq!(sombre::timeline(&own.path).len(), 4);
        let made = fs::read_dir(sombre::folder(&roots.sombre))
            .map(|entries| entries.count())
            .unwrap_or_default();
        assert_eq!(made, 1);
    }

    #[test]
    fn refuses_chats_it_cannot_find() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let roots = roots(dir.path());
        let codex = fake(&roots);
        let result = hand(
            &roots,
            Agent::Claude,
            "missing",
            Agent::Codex,
            Path::new("/w"),
            &versions(),
            &codex,
        );
        assert!(result.is_err());
    }
}

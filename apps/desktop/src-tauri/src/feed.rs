use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Serialize;
use sombre_core::{Agent, Event, Session};

type Parsed = (PathBuf, Option<(u64, SystemTime)>, Arc<Vec<Event>>);

static PARSED: Mutex<Option<Parsed>> = Mutex::new(None);

pub fn transcript(session: &Session) -> Arc<Vec<Event>> {
    let stamp = std::fs::metadata(&session.path)
        .ok()
        .and_then(|meta| Some((meta.len(), meta.modified().ok()?)));
    let Ok(mut parsed) = PARSED.lock() else {
        return Arc::new(sombre_core::transcript(session));
    };
    if let Some((path, seen, events)) = parsed.as_ref()
        && *path == session.path
        && stamp.is_some()
        && *seen == stamp
    {
        return Arc::clone(events);
    }
    let events = Arc::new(sombre_core::transcript(session));
    *parsed = Some((session.path.clone(), stamp, Arc::clone(&events)));
    events
}

#[derive(Debug, PartialEq, Serialize, specta::Type)]
pub struct Chat {
    pub agent: Agent,
    pub id: String,
    pub title: String,
    pub project: String,
    pub place: String,
    pub folder: String,
    pub ago: String,
    pub updated: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct Task {
    pub text: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Item {
    User {
        text: String,
    },
    Text {
        text: String,
    },
    Step {
        step: String,
        label: String,
        detail: String,
        output: String,
        failed: bool,
        patch: Vec<String>,
    },
    Plan {
        tasks: Vec<Task>,
    },
    Note {
        text: String,
    },
    Failed {
        text: String,
    },
    Changes {
        from: String,
        to: String,
        files: Vec<sombre_core::ledger::File>,
    },
    Done {
        millis: Option<u64>,
        cost: Option<f64>,
    },
}

pub fn merge(items: Vec<Item>, mut records: Vec<sombre_core::ledger::Record>) -> Vec<Item> {
    records.sort_by_key(|record| record.turn);
    let mut queue = records.into_iter().peekable();
    let mut placed = Vec::new();
    let mut users = 0;
    let change = |record: sombre_core::ledger::Record| Item::Changes {
        files: record.files,
        from: record.from,
        to: record.to,
    };
    for item in items {
        if matches!(item, Item::User { .. }) {
            while let Some(record) = queue.next_if(|record| record.turn <= users) {
                placed.push(change(record));
            }
            users += 1;
        }
        placed.push(item);
    }
    placed.extend(queue.map(change));
    placed
}

#[derive(Debug, PartialEq, Serialize, specta::Type)]
pub struct Entry {
    pub id: usize,
    #[serde(flatten)]
    pub item: Item,
}

pub fn place(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub fn project(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub fn chats(sessions: &[Session], now: u64, home: Option<&Path>) -> Vec<Chat> {
    sessions
        .iter()
        .map(|session| Chat {
            agent: session.agent,
            ago: sombre_core::ago(session.updated, now),
            folder: session.cwd.display().to_string(),
            id: session.id.clone(),
            place: place(&session.cwd, home),
            project: project(&session.cwd),
            title: session.title.clone(),
            updated: session.updated,
        })
        .collect()
}

fn attach(
    items: &mut [Item],
    id: &str,
    apply: impl FnOnce(&mut String, &mut bool, &mut Vec<String>),
) {
    let found = items.iter_mut().rev().find_map(|item| match item {
        Item::Step {
            step,
            output,
            failed,
            patch,
            ..
        } if step == id => Some((output, failed, patch)),
        _ => None,
    });
    if let Some((output, failed, patch)) = found {
        apply(output, failed, patch);
    }
}

pub fn relative(text: &str, cwd: &Path) -> String {
    text.replace(&format!("{}/", cwd.display()), "")
}

pub fn story(
    events: Vec<Event>,
    cwd: &Path,
    records: Vec<sombre_core::ledger::Record>,
) -> Vec<Entry> {
    let mut items: Vec<Item> = Vec::new();
    for event in events {
        match event.scrubbed() {
            Event::Prompt(text) => items.push(Item::User { text }),
            Event::Text(text) => items.push(Item::Text { text }),
            Event::Delta(text) => match items.last_mut() {
                Some(Item::Text { text: last }) => last.push_str(&text),
                _ => items.push(Item::Text { text }),
            },
            Event::Step { id, label, detail } => items.push(Item::Step {
                detail: relative(&detail, cwd),
                failed: false,
                label,
                output: String::new(),
                patch: Vec::new(),
                step: id,
            }),
            Event::Output { id, text, failed } => {
                attach(&mut items, &id, |output, broke, _| {
                    *output = text;
                    *broke = failed;
                });
            }
            Event::Patch { id, lines } => attach(&mut items, &id, |_, _, patch| *patch = lines),
            Event::Image(path) => items.push(Item::Step {
                detail: project(Path::new(&path)),
                failed: false,
                label: "View".to_string(),
                output: String::new(),
                patch: Vec::new(),
                step: path,
            }),
            Event::Plan { steps, .. } => items.push(Item::Plan {
                tasks: steps
                    .into_iter()
                    .map(|(text, status)| Task { status, text })
                    .collect(),
            }),
            Event::Note(text) => items.push(Item::Note { text }),
            Event::Failed(text) => items.push(Item::Failed { text }),
            Event::Done { millis, cost, .. } => items.push(Item::Done { cost, millis }),
            _ => {}
        }
    }
    merge(items, records)
        .into_iter()
        .enumerate()
        .map(|(id, item)| Entry { id, item })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn a_chat_opened_twice_is_parsed_once_until_its_file_changes() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("chat.jsonl");
        std::fs::write(&path, "").unwrap_or_else(|error| panic!("{error}"));
        let session = Session {
            agent: Agent::Claude,
            id: "chat".to_string(),
            title: String::new(),
            cwd: dir.path().to_path_buf(),
            updated: 0,
            path: path.clone(),
        };
        let first = transcript(&session);
        assert!(Arc::ptr_eq(&first, &transcript(&session)));
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(b"\n"))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!Arc::ptr_eq(&first, &transcript(&session)));
    }

    fn step(id: &str, label: &str, detail: &str) -> Event {
        Event::Step {
            detail: detail.to_string(),
            id: id.to_string(),
            label: label.to_string(),
        }
    }

    #[test]
    fn puts_saved_changes_at_the_end_of_their_turn() {
        let record = |turn: usize| sombre_core::ledger::Record {
            files: Vec::new(),
            from: format!("f{turn}"),
            to: format!("t{turn}"),
            turn,
        };
        let user = |text: &str| Item::User {
            text: text.to_string(),
        };
        let reply = |text: &str| Item::Text {
            text: text.to_string(),
        };
        let placed = merge(
            vec![user("one"), reply("a"), user("two"), reply("b")],
            vec![record(2), record(1)],
        );
        let kinds: Vec<String> = placed
            .iter()
            .map(|item| match item {
                Item::User { text } | Item::Text { text } => text.clone(),
                Item::Changes { from, .. } => from.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(kinds, ["one", "a", "f1", "two", "b", "f2"]);
    }

    #[test]
    fn shortens_folders_under_home() {
        let home = Path::new("/Users/sam");
        assert_eq!(
            place(Path::new("/Users/sam/code/app"), Some(home)),
            "~/code/app"
        );
        assert_eq!(place(Path::new("/Users/sam"), Some(home)), "~");
        assert_eq!(place(Path::new("/srv/app"), Some(home)), "/srv/app");
        assert_eq!(place(Path::new("/srv/app"), None), "/srv/app");
    }

    #[test]
    fn lists_chats_with_project_and_age() {
        let session = Session {
            agent: Agent::Codex,
            cwd: PathBuf::from("/Users/sam/code/app"),
            id: "abc".to_string(),
            path: PathBuf::new(),
            title: "fix the build".to_string(),
            updated: 1_000,
        };
        assert_eq!(
            chats(&[session], 1_000 + 7_200, Some(Path::new("/Users/sam"))),
            vec![Chat {
                agent: Agent::Codex,
                ago: "2h ago".to_string(),
                folder: "/Users/sam/code/app".to_string(),
                id: "abc".to_string(),
                place: "~/code/app".to_string(),
                project: "app".to_string(),
                title: "fix the build".to_string(),
                updated: 1_000,
            }]
        );
    }

    #[test]
    fn folds_output_and_patches_into_their_step() {
        let entries = story(
            vec![
                Event::Prompt("run the tests".to_string()),
                step("1", "Run", "cargo test"),
                step("2", "Edit", "/work/src/main.rs"),
                Event::Output {
                    failed: true,
                    id: "1".to_string(),
                    text: "1 failed".to_string(),
                },
                Event::Patch {
                    id: "2".to_string(),
                    lines: vec!["+fixed".to_string()],
                },
                Event::Output {
                    failed: false,
                    id: "missing".to_string(),
                    text: "dropped".to_string(),
                },
            ],
            Path::new("/work"),
            Vec::new(),
        );
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[1].item,
            Item::Step {
                detail: "cargo test".to_string(),
                failed: true,
                label: "Run".to_string(),
                output: "1 failed".to_string(),
                patch: Vec::new(),
                step: "1".to_string(),
            }
        );
        assert!(
            matches!(&entries[2].item, Item::Step { patch, detail, .. } if patch == &vec!["+fixed".to_string()] && detail == "src/main.rs")
        );
    }

    #[test]
    fn joins_streamed_text_and_numbers_entries() {
        let entries = story(
            vec![
                Event::Delta("hel".to_string()),
                Event::Delta("lo".to_string()),
                Event::Model("ignored".to_string()),
                Event::Done {
                    cost: Some(0.5),
                    left: Some(40),
                    millis: Some(1_200),
                },
            ],
            Path::new("/work"),
            Vec::new(),
        );
        assert_eq!(
            entries,
            vec![
                Entry {
                    id: 0,
                    item: Item::Text {
                        text: "hello".to_string()
                    }
                },
                Entry {
                    id: 1,
                    item: Item::Done {
                        cost: Some(0.5),
                        millis: Some(1_200)
                    }
                },
            ]
        );
    }

    #[test]
    fn serializes_with_a_kind_tag() {
        let entries = story(
            vec![
                Event::Prompt("hi".to_string()),
                Event::Plan {
                    id: "p".to_string(),
                    steps: vec![("read".to_string(), "completed".to_string())],
                },
            ],
            Path::new("/work"),
            Vec::new(),
        );
        assert_eq!(
            serde_json::to_value(&entries).unwrap_or_default(),
            serde_json::json!([
                {"id": 0, "kind": "user", "text": "hi"},
                {"id": 1, "kind": "plan", "tasks": [{"text": "read", "status": "completed"}]},
            ])
        );
    }
}

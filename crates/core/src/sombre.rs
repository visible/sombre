use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::{self, Item, Turn};
use crate::event::Event;
use crate::session::{Agent, Session, seconds, title};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Record {
    Meta {
        id: String,
        cwd: PathBuf,
    },
    Message {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        message: Value,
    },
    Compact {
        summary: String,
    },
    Title {
        title: String,
    },
}

pub fn find(root: &Path, id: &str) -> Option<PathBuf> {
    files(root)
        .into_iter()
        .find(|path| path.file_stem().is_some_and(|stem| stem == id))
}

pub fn rename(path: &Path, title: &str) -> std::io::Result<()> {
    use std::io::Write;
    let line = serde_json::to_string(&Record::Title {
        title: title.to_string(),
    })
    .map_err(std::io::Error::other)?;
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    writeln!(file, "{line}")
}

pub fn folder(root: &Path) -> PathBuf {
    root.join("sessions")
}

pub fn files(root: &Path) -> Vec<PathBuf> {
    let Ok(files) = fs::read_dir(folder(root)) else {
        return Vec::new();
    };
    files
        .flatten()
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect()
}

pub fn records(path: &Path) -> Vec<Record> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

pub fn read(path: &Path) -> Option<Session> {
    let updated = seconds(fs::metadata(path).ok()?.modified().ok()?);
    let all = records(path);
    let (id, cwd) = all.iter().find_map(|record| match record {
        Record::Meta { id, cwd } => Some((id.clone(), cwd.clone())),
        _ => None,
    })?;
    let named = all.iter().rev().find_map(|record| match record {
        Record::Title { title: name } => title(name),
        _ => None,
    });
    let heading = named.or_else(|| {
        all.iter().find_map(|record| match record {
            Record::Message { message, .. } if message["role"] == "user" => {
                message["content"].as_str().and_then(title)
            }
            _ => None,
        })
    })?;
    Some(Session {
        agent: Agent::Sombre,
        cwd,
        id,
        path: path.to_path_buf(),
        title: heading,
        updated,
    })
}

pub fn transcript(path: &Path) -> Vec<Event> {
    let mut events = Vec::new();
    let mut last: Option<String> = None;
    for record in records(path) {
        let (model, message) = match record {
            Record::Message { model, message } => (model, message),
            Record::Compact { .. } => {
                events.push(Event::Note(
                    "earlier messages were compacted into a summary".to_string(),
                ));
                continue;
            }
            Record::Meta { .. } | Record::Title { .. } => continue,
        };
        match message["role"].as_str() {
            Some("user") => events.extend(
                message["content"]
                    .as_str()
                    .map(|text| Event::Prompt(text.trim_end().to_string())),
            ),
            Some("assistant") => {
                events.extend(
                    message["content"]
                        .as_str()
                        .filter(|text| !text.trim().is_empty())
                        .map(|text| Event::Text(text.to_string())),
                );
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let name = call["function"]["name"].as_str().unwrap_or_default();
                    let input: Value = call["function"]["arguments"]
                        .as_str()
                        .and_then(|text| serde_json::from_str(text).ok())
                        .unwrap_or_default();
                    events.push(Event::Step {
                        detail: describe(name, &input),
                        id: call["id"].as_str().unwrap_or_default().to_string(),
                        label: label(name).to_string(),
                    });
                }
                if model.is_some() {
                    last = model;
                }
            }
            _ => {}
        }
    }
    events.extend(last.map(Event::Model));
    events
}

pub fn label(tool: &str) -> &str {
    if tool.starts_with("mcp__") {
        return "MCP";
    }
    match tool {
        "bash" | "shell" => "Run",
        "read" => "Read",
        "write" | "edit" => "Edit",
        "grep" | "glob" => "Find",
        "list" => "List",
        "fetch" => "Web",
        "patch" => "Edit",
        "plan" => "Plan",
        "goal" => "Goal",
        other => other,
    }
}

pub fn describe(tool: &str, input: &Value) -> String {
    if let Some(rest) = tool.strip_prefix("mcp__") {
        return rest.replacen("__", " ", 1);
    }
    if tool == "patch" {
        return input["input"]
            .as_str()
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                [
                    "*** Add File: ",
                    "*** Update File: ",
                    "*** Delete File: ",
                    "*** Move to: ",
                ]
                .iter()
                .find_map(|head| line.strip_prefix(head))
            })
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(", ");
    }
    if tool == "plan" || tool == "goal" {
        return String::new();
    }
    let key = match tool {
        "bash" | "shell" => "command",
        "grep" | "glob" => "pattern",
        "fetch" => "url",
        _ => "path",
    };
    input[key]
        .as_str()
        .unwrap_or(if tool == "list" { "." } else { "" })
        .to_string()
}

pub fn timeline(path: &Path) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut calls: Vec<(String, usize)> = Vec::new();
    for record in records(path) {
        let Record::Message { message, .. } = record else {
            continue;
        };
        match message["role"].as_str() {
            Some("user") => {
                if let Some(text) = message["content"].as_str() {
                    items.push(Item::Prompt {
                        text: text.to_string(),
                    });
                }
            }
            Some("assistant") => {
                if let Some(text) = message["content"]
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                {
                    items.push(Item::Reply {
                        text: text.to_string(),
                    });
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let name = call["function"]["name"].as_str().unwrap_or_default();
                    let input: Value = call["function"]["arguments"]
                        .as_str()
                        .and_then(|text| serde_json::from_str(text).ok())
                        .unwrap_or_default();
                    calls.push((
                        call["id"].as_str().unwrap_or_default().to_string(),
                        items.len(),
                    ));
                    items.push(Item::Tool {
                        detail: describe(name, &input),
                        label: label(name).to_string(),
                        output: String::new(),
                    });
                }
            }
            Some("tool") => {
                let id = message["tool_call_id"].as_str().unwrap_or_default();
                if let Some((_, index)) = calls.iter().find(|(call, _)| call == id)
                    && let Some(Item::Tool { output, .. }) = items.get_mut(*index)
                {
                    *output = convert::cap(message["content"].as_str().unwrap_or_default());
                }
            }
            _ => {}
        }
    }
    items
}

pub fn lines(turns: &[Turn], id: &str, cwd: &Path, header: bool) -> Vec<String> {
    let mut written: Vec<Record> = Vec::new();
    if header {
        written.push(Record::Meta {
            cwd: cwd.to_path_buf(),
            id: id.to_string(),
        });
    }
    written.extend(turns.iter().map(|turn| {
        let (role, text) = match turn {
            Turn::User(text) => ("user", text),
            Turn::Agent(text) => ("assistant", text),
        };
        Record::Message {
            message: serde_json::json!({"role": role, "content": text}),
            model: None,
        }
    }));
    written
        .iter()
        .filter_map(|record| serde_json::to_string(record).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, lines: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
        }
        let mut file = File::create(path).unwrap_or_else(|error| panic!("{error}"));
        for line in lines {
            writeln!(file, "{line}").unwrap_or_else(|error| panic!("{error}"));
        }
    }

    const CHAT: [&str; 5] = [
        r#"{"type":"meta","id":"s1","cwd":"/work/atlas"}"#,
        r#"{"type":"message","message":{"role":"system","content":"you are sombre"}}"#,
        r#"{"type":"message","message":{"role":"user","content":"list the files"}}"#,
        r#"{"type":"message","model":"gateway:openai/gpt-oss-120b","message":{"role":"assistant","content":"Looking.","tool_calls":[{"id":"c1","type":"function","function":{"name":"bash","arguments":"{\"command\":\"ls -la\"}"}}]}}"#,
        r#"{"type":"message","message":{"role":"tool","tool_call_id":"c1","content":"a.txt"}}"#,
    ];

    #[test]
    fn renaming_appends_a_title_that_wins() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("s1.jsonl");
        write(&path, &CHAT);
        rename(&path, "deploy fix").unwrap_or_else(|error| panic!("{error}"));
        let session = read(&path).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.title, "deploy fix");
        assert!(
            transcript(&path)
                .iter()
                .all(|event| !matches!(event, Event::Prompt(text) if text == "deploy fix"))
        );
    }

    #[test]
    fn reads_a_session_from_its_records() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = folder(dir.path()).join("s1.jsonl");
        write(&path, &CHAT);
        let session = read(&path).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.agent, Agent::Sombre);
        assert_eq!(session.id, "s1");
        assert_eq!(session.title, "list the files");
        assert_eq!(session.cwd, PathBuf::from("/work/atlas"));
        assert_eq!(files(dir.path()).len(), 1);
    }

    #[test]
    fn skips_sessions_without_a_prompt() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = folder(dir.path()).join("empty.jsonl");
        write(&path, &CHAT[..2]);
        assert_eq!(read(&path), None);
    }

    #[test]
    fn replays_prompts_replies_and_tool_calls() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("s1.jsonl");
        write(&path, &CHAT);
        assert_eq!(
            transcript(&path),
            vec![
                Event::Prompt("list the files".to_string()),
                Event::Text("Looking.".to_string()),
                Event::Step {
                    detail: "ls -la".to_string(),
                    id: "c1".to_string(),
                    label: "Run".to_string(),
                },
                Event::Model("gateway:openai/gpt-oss-120b".to_string()),
            ]
        );
    }

    #[test]
    fn writes_records_that_read_back() {
        let record = Record::Message {
            message: serde_json::json!({"role": "user", "content": "hi"}),
            model: None,
        };
        let line = serde_json::to_string(&record).unwrap_or_default();
        assert!(line.starts_with(r#"{"type":"message""#));
        assert!(!line.contains("model"));
        assert_eq!(serde_json::from_str::<Record>(&line).ok(), Some(record));
    }

    #[test]
    fn reads_the_timeline_with_tool_results() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("s1.jsonl");
        write(&path, &CHAT);
        assert_eq!(
            timeline(&path),
            vec![
                Item::Prompt {
                    text: "list the files".to_string()
                },
                Item::Reply {
                    text: "Looking.".to_string()
                },
                Item::Tool {
                    detail: "ls -la".to_string(),
                    label: "Run".to_string(),
                    output: "a.txt".to_string()
                },
            ]
        );
    }

    #[test]
    fn written_turns_read_back() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = folder(dir.path()).join("h1.jsonl");
        let turns = [
            Turn::User("remember juniper".to_string()),
            Turn::Agent("ok".to_string()),
        ];
        let text = lines(&turns, "h1", Path::new("/work/atlas"), true).join("\n") + "\n";
        fs::create_dir_all(folder(dir.path())).unwrap_or_else(|error| panic!("{error}"));
        fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            read(&path).map(|session| session.title),
            Some("remember juniper".to_string())
        );
        assert_eq!(timeline(&path).len(), 2);
    }

    #[test]
    fn returns_nothing_before_the_first_chat() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        assert!(files(dir.path()).is_empty());
    }
}

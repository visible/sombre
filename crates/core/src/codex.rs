use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::clock;
use crate::convert::{self, Item, Turn};
use crate::event::{self, Event};
use crate::session::{Agent, Model, Session, seconds, title};

pub fn files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(&root.join("sessions"), &mut files);
    files
}

pub fn prompts(root: &Path) -> HashMap<String, String> {
    history(&root.join("history.jsonl"))
}

pub fn read(path: &Path, prompts: &HashMap<String, String>) -> Option<Session> {
    let updated = seconds(fs::metadata(path).ok()?.modified().ok()?);
    let mut lines = BufReader::new(File::open(path).ok()?)
        .lines()
        .map_while(Result::ok);
    let meta: Value = serde_json::from_str(&lines.next()?).ok()?;
    if meta["type"] != "session_meta" {
        return None;
    }
    let payload = &meta["payload"];
    if payload["source"].get("subagent").is_some() {
        return None;
    }
    let id = payload["id"].as_str()?.to_string();
    let cwd = PathBuf::from(payload["cwd"].as_str()?);
    let heading = prompts
        .get(&id)
        .and_then(|text| title(text))
        .or_else(|| lines.find_map(|line| first(&line)))?;
    Some(Session {
        agent: Agent::Codex,
        cwd,
        id,
        path: path.to_path_buf(),
        title: heading,
        updated,
    })
}

pub fn transcript(path: &Path) -> Vec<Event> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    events(path, BufReader::new(file).lines().map_while(Result::ok))
}

pub fn events(path: &Path, lines: impl Iterator<Item = String>) -> Vec<Event> {
    let mut events = Vec::new();
    let mut last: Option<String> = None;
    for entry in lines
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter(|entry| entry["type"] == "event_msg")
    {
        let payload = &entry["payload"];
        match payload["type"].as_str() {
            Some("item_completed") => events.extend(item(&payload["item"])),
            Some("user_message") => {
                if let Some(text) = said(payload) {
                    events.push(Event::Prompt(text));
                    events.extend(
                        attached(path, &entry, payload)
                            .into_iter()
                            .map(Event::Attached),
                    );
                }
            }
            Some("agent_message") => events.extend(said(payload).map(Event::Text)),
            Some("thread_settings_applied") => {
                if let Some(model) = payload["thread_settings"]["model"].as_str() {
                    last = Some(model.to_string());
                }
            }
            _ => {}
        }
    }
    events.extend(last.map(Event::Model));
    events
}

pub fn models(root: &Path) -> Vec<Model> {
    let Some(cache) = fs::read_to_string(root.join("models_cache.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    let mut listed: Vec<(i64, Model)> = cache["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["visibility"] == "list")
        .filter_map(|entry| {
            Some((
                entry["priority"].as_i64().unwrap_or(i64::MAX),
                Model {
                    about: entry["description"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    id: entry["slug"].as_str()?.to_string(),
                },
            ))
        })
        .collect();
    listed.sort_by_key(|(priority, _)| *priority);
    listed.into_iter().map(|(_, model)| model).collect()
}

pub fn model(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("config.toml")).ok()?;
    text.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "model").then(|| value.trim().trim_matches('"').to_string())
        })
}

fn item(item: &Value) -> Option<Event> {
    match item["type"].as_str()? {
        "UserMessage" => text(&item["content"]).map(Event::Prompt),
        "AgentMessage" => text(&item["content"]).map(Event::Text),
        "CommandExecution" => Some(Event::Step {
            detail: command(&item["command"]),
            id: item["id"].as_str().unwrap_or_default().to_string(),
            label: "Run".to_string(),
        }),
        "FileChange" => item["changes"].as_object().map(|changes| Event::Step {
            id: item["id"].as_str().unwrap_or_default().to_string(),
            detail: changes
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            label: "Edit".to_string(),
        }),
        _ => None,
    }
}

fn attached(path: &Path, entry: &Value, payload: &Value) -> Vec<String> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let stamp: String = entry["timestamp"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let listed = |key: &str| -> Vec<String> {
        payload[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let inline = listed("images")
        .into_iter()
        .enumerate()
        .filter_map(|(index, data)| crate::pasted::url(&format!("{stem}-{stamp}-{index}"), &data));
    let local = listed("local_images")
        .into_iter()
        .enumerate()
        .filter_map(|(index, file)| {
            crate::pasted::copy(&format!("{stem}-{stamp}-local{index}"), Path::new(&file))
        });
    inline.chain(local).collect()
}

fn said(payload: &Value) -> Option<String> {
    payload["message"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn text(content: &Value) -> Option<String> {
    let joined = content
        .as_array()?
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn command(value: &Value) -> String {
    let parts: Vec<&str> = value
        .as_array()
        .map(|parts| parts.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    match parts.as_slice() {
        [program, "-lc", script] if program.ends_with("sh") => (*script).to_string(),
        [] => value.as_str().map(event::shell).unwrap_or_default(),
        _ => parts.join(" "),
    }
}

pub fn asked(line: &str) -> Option<String> {
    first(line)
}

fn first(line: &str) -> Option<String> {
    let entry: Value = serde_json::from_str(line).ok()?;
    let payload = &entry["payload"];
    if entry["type"] != "response_item" || payload["type"] != "message" || payload["role"] != "user"
    {
        return None;
    }
    payload["content"].as_array()?.iter().find_map(|block| {
        let text = block["text"].as_str()?.trim_start();
        if block["type"] != "input_text" || text.starts_with('<') || text.starts_with('#') {
            return None;
        }
        title(text)
    })
}

pub fn names(root: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    let Ok(file) = File::open(root.join("session_index.jsonl")) else {
        return names;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let (Some(id), Some(name)) = (entry["id"].as_str(), entry["thread_name"].as_str())
            && let Some(name) = title(name)
        {
            names.insert(id.to_string(), name);
        }
    }
    names
}

fn history(path: &Path) -> HashMap<String, String> {
    let mut prompts = HashMap::new();
    let Ok(file) = File::open(path) else {
        return prompts;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let (Some(id), Some(text)) = (entry["session_id"].as_str(), entry["text"].as_str()) {
            prompts
                .entry(id.to_string())
                .or_insert_with(|| text.to_string());
        }
    }
    prompts
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        {
            files.push(path);
        }
    }
}

pub fn timeline(path: &Path) -> Vec<Item> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter(|entry| entry["type"] == "event_msg")
        .filter_map(|entry| {
            let payload = &entry["payload"];
            match payload["type"].as_str()? {
                "user_message" => return said(payload).map(|text| Item::Prompt { text }),
                "agent_message" => return said(payload).map(|text| Item::Reply { text }),
                "item_completed" => {}
                _ => return None,
            }
            let item = &payload["item"];
            match item["type"].as_str()? {
                "UserMessage" => text(&item["content"]).map(|text| Item::Prompt { text }),
                "AgentMessage" => text(&item["content"]).map(|text| Item::Reply { text }),
                "CommandExecution" => Some(Item::Tool {
                    detail: command(&item["command"]),
                    label: "Run".to_string(),
                    output: convert::cap(item["aggregated_output"].as_str().unwrap_or_default()),
                }),
                "FileChange" => item["changes"].as_object().map(|changes| Item::Tool {
                    detail: changes
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                    label: "Edit".to_string(),
                    output: String::new(),
                }),
                _ => None,
            }
        })
        .collect()
}

pub fn find(root: &Path, id: &str) -> Option<PathBuf> {
    let ending = format!("{id}.jsonl");
    let mut found = Vec::new();
    walk(&root.join("sessions"), &mut found);
    walk(&root.join("archived_sessions"), &mut found);
    found.into_iter().find(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(&ending))
    })
}

pub fn place(root: &Path, id: &str, millis: u128) -> PathBuf {
    let stamp = clock::iso(millis);
    root.join("sessions")
        .join(&stamp[..4])
        .join(&stamp[5..7])
        .join(&stamp[8..10])
        .join(format!(
            "rollout-{}-{id}.jsonl",
            stamp[..19].replace(':', "-")
        ))
}

pub fn lines(
    turns: &[Turn],
    id: &str,
    cwd: &Path,
    version: &str,
    header: bool,
    listed: bool,
) -> Vec<String> {
    let start = clock::now();
    let mut written = Vec::new();
    if header {
        let stamp = clock::iso(start);
        written.push(
            json!({
                "timestamp": stamp,
                "type": "session_meta",
                "payload": {
                    "id": id,
                    "session_id": id,
                    "timestamp": stamp,
                    "cwd": cwd.display().to_string(),
                    "originator": if listed { "codex_cli_rs" } else { "codex_exec" },
                    "cli_version": version,
                    "source": if listed { "cli" } else { "exec" },
                    "model_provider": "openai"
                }
            })
            .to_string(),
        );
    }
    for (index, turn) in turns.iter().enumerate() {
        let stamp = clock::iso(start + u128::try_from(index).unwrap_or_default() + 1);
        let (role, kind, item) = match turn {
            Turn::User(text) => (
                "user",
                "input_text",
                json!({"type": "UserMessage", "id": clock::uuid(), "content": [{"type": "text", "text": text, "text_elements": []}]}),
            ),
            Turn::Agent(text) => (
                "assistant",
                "output_text",
                json!({"type": "AgentMessage", "id": clock::uuid(), "content": [{"type": "Text", "text": text}], "phase": "final_answer"}),
            ),
        };
        let body = match turn {
            Turn::User(text) | Turn::Agent(text) => text,
        };
        written.push(
            json!({
                "timestamp": stamp,
                "type": "response_item",
                "payload": {"type": "message", "role": role, "content": [{"type": kind, "text": body}]}
            })
            .to_string(),
        );
        written.push(
            json!({
                "timestamp": stamp,
                "type": "event_msg",
                "payload": {"type": "item_completed", "thread_id": id, "turn_id": clock::uuid(), "item": item}
            })
            .to_string(),
        );
    }
    written
}

pub fn publish(path: &Path) -> std::io::Result<bool> {
    let text = fs::read_to_string(path)?;
    let (first, rest) = text.split_once('\n').unwrap_or((&text, ""));
    let Ok(mut meta) = serde_json::from_str::<Value>(first) else {
        return Ok(false);
    };
    if meta["type"] != "session_meta" || meta["payload"]["source"] != "exec" {
        return Ok(false);
    }
    meta["payload"]["source"] = json!("cli");
    if meta["payload"]["originator"] == "codex_exec" {
        meta["payload"]["originator"] = json!("codex_cli_rs");
    }
    let temporary = path.with_extension("jsonl.sombre");
    fs::write(&temporary, format!("{meta}\n{rest}"))?;
    fs::rename(&temporary, path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn codex_images_come_back_with_their_prompt() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        crate::pasted::FOLDER.with(|folder| *folder.borrow_mut() = Some(dir.path().join("pasted")));
        let local = dir.path().join("shot.png");
        std::fs::write(&local, b"png bytes").unwrap_or_else(|error| panic!("{error}"));
        let pixel = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
        let line = format!(
            r#"{{"timestamp":"2026-11-18T01:02:03.004Z","type":"event_msg","payload":{{"type":"user_message","message":"what is in these","images":["data:image/png;base64,{pixel}"],"local_images":["{}"],"text_elements":[]}}}}"#,
            local.display()
        );
        let path = dir.path().join("rollout-a1.jsonl");
        std::fs::write(&path, format!("{line}\n")).unwrap_or_else(|error| panic!("{error}"));
        let events = transcript(&path);
        assert_eq!(events[0], Event::Prompt("what is in these".to_string()));
        let stored: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                Event::Attached(path) => Some(path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(stored.len(), 2);
        assert!(stored[0].ends_with(".png") && stored[1].ends_with("local0.png"));
        assert_eq!(std::fs::read(&stored[1]).ok(), Some(b"png bytes".to_vec()));
    }

    const META: &str = r#"{"timestamp":"2026-11-10T18:33:57Z","type":"session_meta","payload":{"id":"019a-abc","cwd":"/work/atlas","originator":"codex_cli_rs"}}"#;

    fn write(path: &Path, lines: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("{error}"));
        }
        let mut file = File::create(path).unwrap_or_else(|error| panic!("{error}"));
        for line in lines {
            writeln!(file, "{line}").unwrap_or_else(|error| panic!("{error}"));
        }
    }

    #[test]
    fn reads_names_codex_saved_with_the_newest_winning() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        fs::write(
            dir.path().join("session_index.jsonl"),
            concat!(
                r#"{"id":"t1","thread_name":"Fix the login redirect","updated_at":"2026-11-11T03:54:03.154107Z"}"#,
                "\n",
                r#"{"id":"t2","thread_name":"Add a dark mode","updated_at":"2026-11-11T04:04:27.636728Z"}"#,
                "\n",
                r#"{"id":"t1","thread_name":"login redirect, renamed","updated_at":"2026-11-11T05:00:00Z"}"#,
                "\n",
            ),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let found = names(dir.path());
        assert_eq!(
            found.get("t1").map(String::as_str),
            Some("login redirect, renamed")
        );
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn skips_injected_context_and_reads_the_prompt() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-1.jsonl");
        write(
            &path,
            &[
                META,
                r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /work/atlas"},{"type":"input_text","text":"<environment_context>cwd</environment_context>"}]}}"##,
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"add retries to the payment client"}]}}"#,
            ],
        );
        let session = read(&path, &HashMap::new()).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.id, "019a-abc");
        assert_eq!(session.agent, Agent::Codex);
        assert_eq!(session.title, "add retries to the payment client");
        assert_eq!(session.cwd, PathBuf::from("/work/atlas"));
    }

    #[test]
    fn prefers_the_typed_prompt_from_history() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-2.jsonl");
        write(&path, &[META]);
        let prompts = HashMap::from([("019a-abc".to_string(), "what changed today".to_string())]);
        assert_eq!(
            read(&path, &prompts).map(|session| session.title),
            Some("what changed today".to_string())
        );
    }

    #[test]
    fn skips_subagent_threads() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-sub.jsonl");
        write(
            &path,
            &[
                r#"{"type":"session_meta","payload":{"id":"sub-1","cwd":"/work/atlas","source":{"subagent":{"thread_spawn":{"parent_thread_id":"019a-abc"}}}}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"review the diff"}]}}"#,
            ],
        );
        assert_eq!(read(&path, &HashMap::new()), None);
    }

    #[test]
    fn ignores_files_that_are_not_sessions() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-3.jsonl");
        write(&path, &[r#"{"type":"event_msg","payload":{}}"#]);
        assert_eq!(read(&path, &HashMap::new()), None);
    }

    #[test]
    fn keeps_the_first_history_entry_per_session() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("history.jsonl");
        write(
            &path,
            &[
                r#"{"session_id":"s1","ts":1,"text":"first"}"#,
                r#"{"session_id":"s1","ts":2,"text":"second"}"#,
                "broken",
            ],
        );
        assert_eq!(history(&path).get("s1").map(String::as_str), Some("first"));
    }

    #[test]
    fn scans_dated_folders_for_rollouts() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let prompt = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#;
        write(
            &dir.path().join("sessions/2026/11/10/rollout-a.jsonl"),
            &[META, prompt],
        );
        write(
            &dir.path().join("sessions/2026/11/09/rollout-b.jsonl"),
            &[META, prompt],
        );
        write(
            &dir.path().join("sessions/2026/11/09/other.jsonl"),
            &[META, prompt],
        );
        assert_eq!(files(dir.path()).len(), 2);
    }

    #[test]
    fn replays_completed_items_in_order() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-4.jsonl");
        write(
            &path,
            &[
                META,
                r#"{"type":"event_msg","payload":{"type":"thread_settings_applied","thread_id":"t","thread_settings":{"model":"gpt-6.1-sol","model_provider_id":"openai"}}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"use the shell to run: ls"}]}}"#,
                r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"UserMessage","id":"1","content":[{"type":"text","text":"use the shell to run: ls","text_elements":[]}]}}}"#,
                r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"Reasoning","id":"2","summary_text":[],"raw_content":[]}}}"#,
                r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"AgentMessage","id":"3","content":[{"type":"Text","text":"I’ll list the files."}],"phase":"commentary"}}}"#,
                r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"CommandExecution","id":"4","command":["/bin/zsh","-lc","ls -1"],"status":"completed","exit_code":0}}}"#,
                r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"FileChange","id":"5","changes":{"/w/a.ts":{"type":"add","content":"x"}}}}}"#,
            ],
        );
        assert_eq!(
            transcript(&path),
            vec![
                Event::Prompt("use the shell to run: ls".to_string()),
                Event::Text("I’ll list the files.".to_string()),
                Event::Step {
                    id: "4".to_string(),
                    detail: "ls -1".to_string(),
                    label: "Run".to_string()
                },
                Event::Step {
                    id: "5".to_string(),
                    detail: "/w/a.ts".to_string(),
                    label: "Edit".to_string()
                },
                Event::Model("gpt-6.1-sol".to_string()),
            ]
        );
    }

    #[test]
    fn lists_visible_models_in_priority_order() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        write(
            &dir.path().join("models_cache.json"),
            &[
                r#"{"fetched_at":"x","models":[{"slug":"gpt-6-astra","description":"Frontier intelligence.","visibility":"list","priority":3},{"slug":"gpt-reserve","description":"Hidden.","visibility":"hide","priority":5},{"slug":"gpt-6.1-sol","description":"Latest workhorse.","visibility":"list","priority":1}]}"#,
            ],
        );
        let ids: Vec<String> = models(dir.path())
            .into_iter()
            .map(|model| model.id)
            .collect();
        assert_eq!(ids, ["gpt-6.1-sol", "gpt-6-astra"]);
        assert!(models(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn reads_the_default_model_from_config() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        write(
            &dir.path().join("config.toml"),
            &[
                r#"model = "gpt-6.1-sol""#,
                r#"model_reasoning_effort = "high""#,
                "[profiles.fast]",
                r#"model = "gpt-6-luna""#,
            ],
        );
        assert_eq!(model(dir.path()), Some("gpt-6.1-sol".to_string()));
    }

    #[test]
    fn written_turns_read_back_as_a_listed_session() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let cwd = Path::new("/work/atlas");
        let turns = [
            Turn::User("remember saffron".to_string()),
            Turn::Agent("ran `ls`\n```\na.txt\n```".to_string()),
        ];
        let path = place(dir.path(), "c1", 1_794_346_265_123);
        assert!(path.ends_with("sessions/2026/11/10/rollout-2026-11-10T21-31-05-c1.jsonl"));
        fs::create_dir_all(path.parent().unwrap_or(dir.path()))
            .unwrap_or_else(|error| panic!("{error}"));
        fs::write(
            &path,
            lines(&turns, "c1", cwd, "0.167.0", true, true).join("\n") + "\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let session = read(&path, &HashMap::new()).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.id, "c1");
        assert_eq!(session.title, "remember saffron");
        assert_eq!(timeline(&path).len(), 2);
        assert_eq!(transcript(&path).len(), 2);
        assert_eq!(find(dir.path(), "c1"), Some(path));
    }

    #[test]
    fn reads_turns_codex_writes_in_the_older_event_shape() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-old.jsonl");
        write(
            &path,
            &[
                META,
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"also remember ember","images":[],"text_elements":[]}}"#,
                r#"{"type":"event_msg","payload":{"type":"agent_message","message":"cobalt","phase":"final_answer"}}"#,
            ],
        );
        assert_eq!(
            timeline(&path),
            vec![
                Item::Prompt {
                    text: "also remember ember".to_string()
                },
                Item::Reply {
                    text: "cobalt".to_string()
                },
            ]
        );
        assert_eq!(
            transcript(&path),
            vec![
                Event::Prompt("also remember ember".to_string()),
                Event::Text("cobalt".to_string()),
            ]
        );
    }

    #[test]
    fn reads_command_output_into_the_timeline() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-t.jsonl");
        write(
            &path,
            &[
                META,
                r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"4","command":["/bin/zsh","-lc","ls"],"aggregated_output":"a.txt\n","exit_code":0}}}"#,
            ],
        );
        assert_eq!(
            timeline(&path),
            vec![Item::Tool {
                detail: "ls".to_string(),
                label: "Run".to_string(),
                output: "a.txt".to_string()
            }]
        );
    }

    #[test]
    fn writes_an_exec_header_unless_listing_is_on() {
        let turns = [Turn::User("hi".to_string())];
        let hidden = lines(&turns, "c", Path::new("/w"), "0.167.0", true, false);
        assert!(hidden[0].contains("\"source\":\"exec\""));
        let shown = lines(&turns, "c", Path::new("/w"), "0.167.0", true, true);
        assert!(shown[0].contains("\"source\":\"cli\""));
    }

    #[test]
    fn publishing_lists_exec_sessions_in_the_picker() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("rollout-p.jsonl");
        write(
            &path,
            &[
                r#"{"type":"session_meta","payload":{"id":"p1","cwd":"/w","source":"exec","originator":"codex_exec"}}"#,
                r#"{"type":"event_msg","payload":{"type":"task_started"}}"#,
            ],
        );
        assert!(publish(&path).unwrap_or(false));
        let text = fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains("\"source\":\"cli\""));
        assert!(text.contains("task_started"));
        assert!(!publish(&path).unwrap_or(true));
    }

    #[test]
    fn returns_nothing_when_codex_is_not_installed() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        assert!(files(&dir.path().join("missing")).is_empty());
    }
}

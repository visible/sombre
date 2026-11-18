use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::clock;
use crate::convert::{self, Item, Turn};
use crate::event::{self, Event};
use crate::session::{Agent, Model, Session, seconds, title};

const MODELS: [(&str, &str); 4] = [
    ("fable", "latest fable"),
    ("opus", "latest opus"),
    ("sonnet", "latest sonnet"),
    ("haiku", "latest haiku"),
];

pub fn files(root: &Path) -> Vec<PathBuf> {
    let Ok(projects) = fs::read_dir(root.join("projects")) else {
        return Vec::new();
    };
    projects
        .flatten()
        .filter_map(|project| fs::read_dir(project.path()).ok())
        .flat_map(|files| files.flatten())
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect()
}

const OVERLAP: u64 = 64 * 1024;

pub fn grown(path: &Path, before: &Session, from: u64) -> Option<Session> {
    let updated = seconds(fs::metadata(path).ok()?.modified().ok()?);
    let mut file = File::open(path).ok()?;
    let start = from.saturating_sub(OVERLAP);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut reader = BufReader::new(file);
    if start > 0 {
        reader.read_until(b'\n', &mut Vec::new()).ok()?;
    }
    let mut named = None;
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("\"custom-title\"") {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<Value>(&line)
            && entry["type"] == "custom-title"
        {
            named = entry["customTitle"].as_str().and_then(title).or(named);
        }
    }
    Some(Session {
        path: path.to_path_buf(),
        title: named.unwrap_or_else(|| before.title.clone()),
        updated,
        ..before.clone()
    })
}

pub fn read(path: &Path) -> Option<Session> {
    let id = path.file_stem()?.to_str()?.to_string();
    let updated = seconds(fs::metadata(path).ok()?.modified().ok()?);
    let reader = BufReader::new(File::open(path).ok()?);
    let mut cwd: Option<PathBuf> = None;
    let mut heading: Option<String> = None;
    let mut named: Option<String> = None;
    for line in reader.lines().map_while(Result::ok) {
        let custom = line.contains("\"custom-title\"");
        if !custom && cwd.is_some() && heading.is_some() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if custom && entry["type"] == "custom-title" {
            named = entry["customTitle"].as_str().and_then(title).or(named);
            continue;
        }
        if cwd.is_none() {
            cwd = entry["cwd"].as_str().map(PathBuf::from);
        }
        if heading.is_none() && typed(&entry) {
            heading = prompt(&entry["message"]["content"]).and_then(title);
        }
    }
    heading = named.or(heading);
    Some(Session {
        agent: Agent::Claude,
        cwd: cwd?,
        id,
        path: path.to_path_buf(),
        title: heading?,
        updated,
    })
}

pub fn transcript(path: &Path) -> Vec<Event> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    let mut last: Option<String> = None;
    for entry in BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter(|entry| !entry["isSidechain"].as_bool().unwrap_or(false))
    {
        if typed(&entry) {
            if let Some(text) = prompt(&entry["message"]["content"]) {
                events.push(Event::Prompt(text.trim_end().to_string()));
                events.extend(pictures(&entry).into_iter().map(Event::Attached));
            }
        } else if entry["subtype"] == "compact_boundary" {
            events.push(Event::Note(COMPACTED.to_string()));
        } else if entry["isApiErrorMessage"] == true {
            let text = entry["message"]["content"][0]["text"]
                .as_str()
                .unwrap_or("the turn failed");
            events.push(Event::Failed(event::explain(
                text,
                entry["apiErrorCode"].as_str(),
            )));
        } else if entry["type"] == "assistant" {
            events.extend(event::blocks(&entry["message"]["content"]));
            if let Some(model) = entry["message"]["model"]
                .as_str()
                .filter(|model| !model.starts_with('<'))
            {
                last = Some(model.to_string());
            }
        }
    }
    events.extend(last.map(Event::Model));
    events
}

pub fn models() -> Vec<Model> {
    MODELS
        .iter()
        .map(|(id, about)| Model {
            about: (*about).to_string(),
            id: (*id).to_string(),
        })
        .collect()
}

pub fn model(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("settings.json")).ok()?;
    let settings: Value = serde_json::from_str(&text).ok()?;
    settings["model"].as_str().map(str::to_string)
}

const COMPACTED: &str = "compacted here. earlier messages were summarized to make room";

pub fn asked(line: &str) -> Option<String> {
    if !line.contains("\"user\"") {
        return None;
    }
    let entry: Value = serde_json::from_str(line).ok()?;
    typed(&entry)
        .then(|| prompt(&entry["message"]["content"]).and_then(title))
        .flatten()
}

fn typed(entry: &Value) -> bool {
    entry["type"] == "user"
        && !entry["isMeta"].as_bool().unwrap_or(false)
        && !entry["isSidechain"].as_bool().unwrap_or(false)
        && !entry["isCompactSummary"].as_bool().unwrap_or(false)
}

fn pictures(entry: &Value) -> Vec<String> {
    let session = entry["sessionId"].as_str().unwrap_or_default();
    let uuid = entry["uuid"].as_str().unwrap_or_default();
    entry["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "image" && block["source"]["type"] == "base64")
        .enumerate()
        .filter_map(|(index, block)| {
            crate::pasted::keep(
                &format!("{session}-{uuid}-{index}"),
                block["source"]["media_type"].as_str()?,
                block["source"]["data"].as_str()?,
            )
        })
        .collect()
}

fn prompt(content: &Value) -> Option<&str> {
    let text = match content {
        Value::String(text) => text.as_str(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .find(|text| !text.trim_start().starts_with("[Image: source:"))?,
        _ => return None,
    };
    let trimmed = text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('<') || trimmed.starts_with("[Request interrupted")
    {
        return None;
    }
    Some(trimmed)
}

pub fn timeline(path: &Path) -> Vec<Item> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut items: Vec<Item> = Vec::new();
    let mut calls: Vec<(String, usize)> = Vec::new();
    for entry in BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter(|entry| {
            !entry["isSidechain"].as_bool().unwrap_or(false) && entry["isApiErrorMessage"] != true
        })
    {
        let content = &entry["message"]["content"];
        if typed(&entry)
            && let Some(text) = prompt(content)
        {
            items.push(Item::Prompt {
                text: text.trim_end().to_string(),
            });
            continue;
        }
        for block in content.as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("text") if entry["type"] == "assistant" => {
                    if let Some(text) = block["text"]
                        .as_str()
                        .filter(|text| !text.trim().is_empty())
                    {
                        items.push(Item::Reply {
                            text: text.to_string(),
                        });
                    }
                }
                Some("tool_use") => {
                    let name = block["name"].as_str().unwrap_or("tool");
                    calls.push((
                        block["id"].as_str().unwrap_or_default().to_string(),
                        items.len(),
                    ));
                    items.push(Item::Tool {
                        detail: event::summary(name, &block["input"]),
                        label: event::label(name).to_string(),
                        output: String::new(),
                    });
                }
                Some("tool_result") => {
                    let id = block["tool_use_id"].as_str().unwrap_or_default();
                    let text = match &block["content"] {
                        Value::String(text) => text.clone(),
                        Value::Array(parts) => parts
                            .iter()
                            .filter_map(|part| part["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n"),
                        _ => String::new(),
                    };
                    if let Some((_, index)) = calls.iter().find(|(call, _)| call == id)
                        && let Some(Item::Tool { output, .. }) = items.get_mut(*index)
                    {
                        *output = convert::cap(&text);
                    }
                }
                _ => {}
            }
        }
    }
    items
}

pub fn spent(path: &Path) -> Option<f64> {
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(4 * 1024 * 1024)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    String::from_utf8_lossy(&tail)
        .lines()
        .rev()
        .filter(|line| line.contains("\"cost-state\""))
        .find_map(|line| serde_json::from_str::<Value>(line).ok()?["totalCostUSD"].as_f64())
}

pub fn find(root: &Path, id: &str) -> Option<PathBuf> {
    let name = format!("{id}.jsonl");
    fs::read_dir(root.join("projects"))
        .ok()?
        .flatten()
        .map(|project| project.path().join(&name))
        .find(|path| path.is_file())
}

pub fn trusted(config: &str, cwd: &Path) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(config) else {
        return false;
    };
    let real = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    [cwd, real.as_path()].iter().any(|start| {
        start.ancestors().any(|place| {
            value["projects"][place.to_string_lossy().as_ref()]["hasTrustDialogAccepted"] == true
        })
    })
}

pub fn configured(cwd: &Path) -> bool {
    [".claude/settings.json", ".claude/settings.local.json"]
        .iter()
        .any(|file| cwd.join(file).is_file())
}

pub fn trust() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(|folder| PathBuf::from(folder).join(".claude.json"))
        .or_else(|| std::env::home_dir().map(|home| home.join(".claude.json")))
}

pub fn untrusted(cwd: &Path) -> bool {
    configured(cwd)
        && !trust()
            .and_then(|file| fs::read_to_string(file).ok())
            .is_some_and(|config| trusted(&config, cwd))
}

pub fn folder(root: &Path, cwd: &Path) -> PathBuf {
    let encoded: String = cwd
        .display()
        .to_string()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    root.join("projects").join(encoded)
}

pub fn last(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut leaf = None;
    let mut newest = None;
    for entry in BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
    {
        if entry["type"] == "last-prompt" {
            leaf = entry["leafUuid"].as_str().map(str::to_string).or(leaf);
        } else if !entry["isSidechain"].as_bool().unwrap_or(false)
            && let Some(uuid) = entry["uuid"].as_str()
        {
            newest = Some(uuid.to_string());
        }
    }
    leaf.or(newest)
}

pub fn lines(
    turns: &[Turn],
    session: &str,
    cwd: &Path,
    version: &str,
    parent: Option<String>,
    listed: bool,
) -> Vec<String> {
    let mut parent = parent;
    let start = clock::now();
    let mut asked = String::new();
    let mut written: Vec<String> = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            let uuid = clock::uuid();
            let stamp = clock::iso(start + u128::try_from(index).unwrap_or_default());
            let message = match turn {
                Turn::User(text) => {
                    asked.clone_from(text);
                    json!({"role": "user", "content": text})
                }
                Turn::Agent(text) => json!({
                    "id": format!("msg_sombre_{}", uuid.replace('-', "")),
                    "type": "message",
                    "role": "assistant",
                    "model": "<synthetic>",
                    "content": [{"type": "text", "text": text}],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }),
            };
            let line = json!({
                "parentUuid": parent,
                "isSidechain": false,
                "userType": "external",
                "entrypoint": if listed { "cli" } else { "sdk-cli" },
                "cwd": cwd.display().to_string(),
                "sessionId": session,
                "version": version,
                "type": if matches!(turn, Turn::User(_)) { "user" } else { "assistant" },
                "message": message,
                "uuid": uuid,
                "timestamp": stamp,
            })
            .to_string();
            parent = Some(uuid);
            line
        })
        .collect();
    if let Some(leaf) = parent {
        written.push(
            json!({
                "type": "last-prompt",
                "lastPrompt": asked,
                "leafUuid": leaf,
                "sessionId": session,
            })
            .to_string(),
        );
    }
    written
}

pub fn publish(path: &Path) -> std::io::Result<bool> {
    let text = fs::read_to_string(path)?;
    let hidden = "\"entrypoint\":\"sdk-cli\"";
    if !text.contains(hidden) {
        return Ok(false);
    }
    let temporary = path.with_extension("jsonl.sombre");
    fs::write(&temporary, text.replace(hidden, "\"entrypoint\":\"cli\""))?;
    fs::rename(&temporary, path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_trusted_when_it_or_a_parent_was_trusted_in_claude() {
        let config = r#"{"projects":{"/work":{"hasTrustDialogAccepted":true},"/other":{"hasTrustDialogAccepted":false}}}"#;
        assert!(trusted(config, Path::new("/work")));
        assert!(trusted(config, Path::new("/work/app/src")));
        assert!(!trusted(config, Path::new("/other/app")));
        assert!(!trusted(config, Path::new("/elsewhere")));
        assert!(!trusted("not json", Path::new("/work")));
    }

    #[test]
    fn only_folders_with_claude_settings_need_trust() {
        let project = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        assert!(!configured(project.path()));
        fs::create_dir_all(project.path().join(".claude"))
            .unwrap_or_else(|error| panic!("{error}"));
        fs::write(project.path().join(".claude/settings.json"), "{}")
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(configured(project.path()));
    }

    #[test]
    fn reads_the_cost_claude_carries_over_on_resume() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("s.jsonl");
        std::fs::write(
            &path,
            [
                r#"{"type":"cost-state","sessionId":"s","totalCostUSD":12.5,"totalAPIDuration":100}"#,
                r#"{"type":"user","message":{"role":"user","content":"hi"},"sessionId":"s"}"#,
                r#"{"type":"cost-state","sessionId":"s","totalCostUSD":390.36245828000017,"totalAPIDuration":29574200,"totalLinesAdded":17223}"#,
                r#"{"type":"last-prompt","lastPrompt":"hi","sessionId":"s"}"#,
            ]
            .join("\n"),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(spent(&path), Some(390.362_458_280_000_17));
        let empty = dir.path().join("e.jsonl");
        std::fs::write(&empty, "{\"type\":\"user\"}\n").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(spent(&empty), None);
    }
    use std::io::Write;

    fn write(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).unwrap_or_else(|error| panic!("{error}"));
        for line in lines {
            writeln!(file, "{line}").unwrap_or_else(|error| panic!("{error}"));
        }
        path
    }

    #[test]
    fn a_grown_chat_keeps_its_folder_and_prompt_and_picks_up_a_new_name() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "ccdbf04b.jsonl",
            &[
                r#"{"type":"user","cwd":"/work/atlas","message":{"role":"user","content":"reply with just ok"}}"#,
                r#"{"type":"assistant","cwd":"/work/atlas","message":{"role":"assistant","content":"ok"}}"#,
            ],
        );
        let before = read(&path).unwrap_or_else(|| panic!("no session"));
        let size = fs::metadata(&path).map_or(0, |meta| meta.len());
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap_or_else(|error| panic!("{error}"));
        writeln!(
            file,
            r#"{{"type":"assistant","cwd":"/work/atlas","message":{{"role":"assistant","content":"more"}}}}"#
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let kept = grown(&path, &before, size).unwrap_or_else(|| panic!("no session"));
        assert_eq!(kept.title, "reply with just ok");
        assert_eq!(kept.cwd, PathBuf::from("/work/atlas"));
        writeln!(
            file,
            r#"{{"type":"custom-title","customTitle":"ember test","sessionId":"ccdbf04b"}}"#
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let renamed = grown(&path, &before, size).unwrap_or_else(|| panic!("no session"));
        assert_eq!(renamed.title, "ember test");
        assert_eq!(
            read(&path).map(|session| session.title),
            Some(renamed.title)
        );
    }

    #[test]
    fn a_pasted_image_comes_back_with_its_prompt_and_the_source_note_is_skipped() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        crate::pasted::FOLDER.with(|folder| *folder.borrow_mut() = Some(dir.path().join("pasted")));
        let pixel = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
        let line = format!(
            r#"{{"type":"user","uuid":"u1","sessionId":"s1","cwd":"/work","message":{{"role":"user","content":[{{"type":"text","text":"[Image: source: /tmp/claude-501/images/74.png]"}},{{"type":"text","text":"[Image #1] why is the title wrong"}},{{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"{pixel}"}}}}]}}}}"#
        );
        let path = write(dir.path(), "s1.jsonl", &[&line]);
        let events = transcript(&path);
        assert_eq!(
            events[0],
            Event::Prompt("[Image #1] why is the title wrong".to_string())
        );
        let Some(Event::Attached(stored)) = events.get(1) else {
            panic!("no image: {events:?}");
        };
        assert!(stored.ends_with("s1-u1-0.png"));
        assert_eq!(
            fs::read(stored).map(|bytes| bytes[1..4].to_vec()).ok(),
            Some(b"PNG".to_vec())
        );
        fs::write(stored, b"kept").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            transcript(&path).get(1),
            Some(&Event::Attached(stored.clone()))
        );
        assert_eq!(fs::read(stored).ok(), Some(b"kept".to_vec()));
        assert_eq!(
            read(&path).map(|session| session.title),
            Some("[Image #1] why is the title wrong".to_string())
        );
    }

    #[test]
    fn uses_the_latest_name_claude_saved() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "ccdbf04b.jsonl",
            &[
                r#"{"type":"user","cwd":"/work/atlas","message":{"role":"user","content":"reply with just ok"}}"#,
                r#"{"type":"custom-title","customTitle":"first name","sessionId":"ccdbf04b"}"#,
                r#"{"type":"assistant","cwd":"/work/atlas","message":{"role":"assistant","content":"ok"}}"#,
                r#"{"type":"custom-title","customTitle":"ember test","sessionId":"ccdbf04b"}"#,
            ],
        );
        let session = read(&path).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.title, "ember test");
    }

    #[test]
    fn reads_the_first_real_prompt() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "abc-123.jsonl",
            &[
                r#"{"type":"queue-operation","operation":"enqueue"}"#,
                r#"{"type":"user","isMeta":true,"cwd":"/work/atlas","message":{"role":"user","content":"Caveat: local commands"}}"#,
                r#"{"type":"user","cwd":"/work/atlas","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
                r#"{"type":"user","isSidechain":true,"cwd":"/work/atlas","message":{"role":"user","content":"subagent task"}}"#,
                r#"{"type":"user","cwd":"/work/atlas","message":{"role":"user","content":"fix the   flaky checkout test"}}"#,
            ],
        );
        let session = read(&path).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.id, "abc-123");
        assert_eq!(session.agent, Agent::Claude);
        assert_eq!(session.title, "fix the flaky checkout test");
        assert_eq!(session.cwd, PathBuf::from("/work/atlas"));
    }

    #[test]
    fn reads_prompts_stored_as_blocks() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "blocks.jsonl",
            &[
                r#"{"type":"user","cwd":"/work/atlas","message":{"role":"user","content":[{"type":"image"},{"type":"text","text":"why does this fail"}]}}"#,
            ],
        );
        assert_eq!(
            read(&path).map(|session| session.title),
            Some("why does this fail".to_string())
        );
    }

    #[test]
    fn skips_sessions_without_a_prompt() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "empty.jsonl",
            &[r#"{"type":"system","cwd":"/work/atlas"}"#, "not json"],
        );
        assert_eq!(read(&path), None);
    }

    #[test]
    fn scans_every_project_folder() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let line =
            r#"{"type":"user","cwd":"/work/atlas","message":{"role":"user","content":"hello"}}"#;
        for project in ["-work-atlas", "-work-orbit"] {
            let folder = dir.path().join("projects").join(project);
            fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
            write(&folder, "one.jsonl", &[line]);
            write(&folder, "notes.txt", &[line]);
        }
        assert_eq!(files(dir.path()).len(), 2);
    }

    #[test]
    fn replays_prompts_replies_and_steps_in_order() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "chat.jsonl",
            &[
                r#"{"type":"user","cwd":"/w","message":{"role":"user","content":"remember the word harbor"}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"ok"}]}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Got it."}]}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"sleep 51"}}]}}"#,
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":""}]}}"#,
                r#"{"type":"user","message":{"role":"user","content":"[Request interrupted by user for tool use]"}}"#,
                r#"{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"subagent"}]}}"#,
                r#"{"type":"user","message":{"role":"user","content":"what word?"}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","model":"claude-sonnet-5-5","content":[]}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","model":"<synthetic>","content":[]}}"#,
            ],
        );
        assert_eq!(
            transcript(&path),
            vec![
                Event::Prompt("remember the word harbor".to_string()),
                Event::Text("Got it.".to_string()),
                Event::Step {
                    id: String::new(),
                    detail: "sleep 51".to_string(),
                    label: "Run".to_string()
                },
                Event::Prompt("what word?".to_string()),
                Event::Model("claude-sonnet-5-5".to_string()),
            ]
        );
    }

    #[test]
    fn shows_a_compaction_as_a_note_not_as_a_prompt() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "chat.jsonl",
            &[
                r#"{"type":"user","cwd":"/w","message":{"role":"user","content":"fix the build"}}"#,
                r#"{"parentUuid":null,"logicalParentUuid":"b1","isSidechain":false,"type":"system","subtype":"compact_boundary","content":"Conversation compacted","level":"info","compactMetadata":{"trigger":"auto","preTokens":973472},"cwd":"/w"}"#,
                r#"{"parentUuid":"u1","isSidechain":false,"type":"user","message":{"role":"user","content":"This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation."},"isVisibleInTranscriptOnly":true,"isCompactSummary":true,"cwd":"/w"}"#,
                r#"{"type":"user","message":{"role":"user","content":"keep going"}}"#,
            ],
        );
        assert_eq!(
            transcript(&path),
            vec![
                Event::Prompt("fix the build".to_string()),
                Event::Note(COMPACTED.to_string()),
                Event::Prompt("keep going".to_string()),
            ]
        );
        assert!(timeline(&path).iter().all(
            |item| !matches!(item, Item::Prompt { text } if text.starts_with("This session"))
        ));
    }

    #[test]
    fn reads_the_default_model_from_settings() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        write(
            dir.path(),
            "settings.json",
            &[r#"{"model":"opus","hooks":{}}"#],
        );
        assert_eq!(model(dir.path()), Some("opus".to_string()));
        assert_eq!(model(&dir.path().join("missing")), None);
    }

    #[test]
    fn reopened_chats_show_api_errors_as_failures() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "e.jsonl",
            &[
                r#"{"type":"user","cwd":"/w","message":{"role":"user","content":"use the new model"}}"#,
                r#"{"type":"assistant","isApiErrorMessage":true,"error":"unknown","apiErrorStatus":400,"apiErrorType":"invalid_request_error","apiErrorCode":"model_not_available","message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"API Error: 400 That model isn't available."}]}}"#,
            ],
        );
        assert_eq!(
            transcript(&path),
            vec![
                Event::Prompt("use the new model".to_string()),
                Event::Failed(
                    "That model isn't available. Pick another model to keep going.".to_string()
                ),
            ]
        );
    }

    #[test]
    fn reads_the_timeline_with_tool_output() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "t.jsonl",
            &[
                r#"{"type":"user","cwd":"/w","message":{"role":"user","content":"run the tests"}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"x"}]}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"bun test"}}]}}"#,
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"3 pass"}]}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"all green"}]}}"#,
                r#"{"type":"assistant","isApiErrorMessage":true,"error":"unknown","apiErrorStatus":400,"message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"API Error: 400 nope"}]}}"#,
            ],
        );
        assert_eq!(
            timeline(&path),
            vec![
                Item::Prompt {
                    text: "run the tests".to_string()
                },
                Item::Tool {
                    detail: "bun test".to_string(),
                    label: "Run".to_string(),
                    output: "3 pass".to_string()
                },
                Item::Reply {
                    text: "all green".to_string()
                },
            ]
        );
    }

    #[test]
    fn written_turns_read_back_as_a_listed_session() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let cwd = Path::new("/work/atlas");
        let turns = [
            Turn::User("remember cobalt".to_string()),
            Turn::Agent("got it".to_string()),
        ];
        let folder = folder(dir.path(), cwd);
        fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
        let path = folder.join("s1.jsonl");
        let written = lines(&turns, "s1", cwd, "2.1.322", None, true);
        fs::write(&path, written.join("\n") + "\n").unwrap_or_else(|error| panic!("{error}"));
        let session = read(&path).unwrap_or_else(|| panic!("no session"));
        assert_eq!(session.title, "remember cobalt");
        assert_eq!(session.cwd, cwd);
        assert_eq!(timeline(&path).len(), 2);
        assert_eq!(find(dir.path(), "s1"), Some(path.clone()));
        let first: Value = serde_json::from_str(&written[0]).unwrap_or_default();
        let second: Value = serde_json::from_str(&written[1]).unwrap_or_default();
        assert_eq!(second["parentUuid"], first["uuid"]);
        assert_eq!(last(&path), second["uuid"].as_str().map(str::to_string));
        let pointer: Value = serde_json::from_str(&written[2]).unwrap_or_default();
        assert_eq!(pointer["type"], "last-prompt");
        assert_eq!(pointer["leafUuid"], second["uuid"]);
        assert_eq!(
            folder.file_name().and_then(|name| name.to_str()),
            Some("-work-atlas")
        );
    }

    #[test]
    fn writes_headless_markers_unless_listing_is_on() {
        let turns = [Turn::User("hi".to_string())];
        let hidden = lines(&turns, "s", Path::new("/w"), "2.1.322", None, false);
        assert!(hidden[0].contains("\"entrypoint\":\"sdk-cli\""));
        let shown = lines(&turns, "s", Path::new("/w"), "2.1.322", None, true);
        assert!(shown[0].contains("\"entrypoint\":\"cli\""));
    }

    #[test]
    fn continues_from_the_leaf_claude_recorded() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "leaf.jsonl",
            &[
                r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"role":"user","content":"hi"}}"#,
                r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","message":{"role":"assistant","content":[{"type":"text","text":"hello"}]}}"#,
                r#"{"type":"attachment","uuid":"x1","parentUuid":"a1"}"#,
                r#"{"type":"last-prompt","lastPrompt":"hi","leafUuid":"x1","sessionId":"s"}"#,
                r#"{"type":"assistant","isSidechain":true,"uuid":"side","parentUuid":"x1"}"#,
            ],
        );
        assert_eq!(last(&path), Some("x1".to_string()));
    }

    #[test]
    fn publishing_makes_headless_chats_visible() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = write(
            dir.path(),
            "p.jsonl",
            &[r#"{"type":"user","entrypoint":"sdk-cli","message":{"role":"user","content":"hi"}}"#],
        );
        assert!(publish(&path).unwrap_or(false));
        assert!(
            fs::read_to_string(&path)
                .unwrap_or_default()
                .contains("\"entrypoint\":\"cli\"")
        );
        assert!(!publish(&path).unwrap_or(true));
    }

    #[test]
    fn returns_nothing_when_claude_is_not_installed() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        assert!(files(&dir.path().join("missing")).is_empty());
    }
}

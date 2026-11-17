use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sombre_core::Event;

use crate::{fetch, patch, process, sandbox, search};

const LIMIT: usize = 30_000;
const LINES: usize = 2_000;
const WINDOW: usize = 400;
const SLIM: [&str; 6] = ["read", "write", "edit", "grep", "bash", "shell"];
const SAFE: [&str; 17] = [
    "ls", "cat", "pwd", "head", "tail", "wc", "grep", "rg", "echo", "seq", "which", "file", "stat",
    "tree", "du", "date", "whoami",
];
const GIT: [&str; 6] = ["status", "diff", "log", "show", "branch", "blame"];

pub const SHELL: &str = if cfg!(windows) { "shell" } else { "bash" };

pub struct Done {
    pub text: String,
    pub event: Option<Event>,
}

pub fn patches(model: &str) -> bool {
    let model = model.rsplit('/').next().unwrap_or(model).to_lowercase();
    ["gpt", "codex", "o3", "o4"]
        .iter()
        .any(|family| model.starts_with(family))
}

pub fn schema(model: &str, goal: bool, slim: bool) -> Value {
    let skip = if patches(model) {
        ["write", "edit"].as_slice()
    } else {
        ["patch"].as_slice()
    };
    let mut tools: Vec<Value> = all()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            tool["function"]["name"].as_str().is_none_or(|name| {
                let kept = if slim {
                    SLIM.contains(&name)
                } else {
                    !skip.contains(&name)
                };
                kept && !(slim && name == "patch")
            })
        })
        .collect();
    if goal {
        tools.push(objective());
    }
    Value::Array(tools)
}

fn all() -> Value {
    json!([
        {"type": "function", "function": {
            "name": "read",
            "description": "Read a text file. Returns numbered lines. Use offset and limit for long files.",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string", "description": "file path, relative to the project folder or absolute"},
                "offset": {"type": "integer", "description": "first line to read, starting at 1"},
                "limit": {"type": "integer", "description": "how many lines to read"}
            }, "required": ["path"]}
        }},
        {"type": "function", "function": {
            "name": "write",
            "description": "Create a file or replace all of its contents.",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            }, "required": ["path", "content"]}
        }},
        {"type": "function", "function": {
            "name": "edit",
            "description": "Replace exact text in a file. old must match the file exactly and only once, unless all is true.",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string"},
                "old": {"type": "string"},
                "new": {"type": "string"},
                "all": {"type": "boolean"}
            }, "required": ["path", "old", "new"]}
        }},
        {"type": "function", "function": {
            "name": "patch",
            "description": "Edit files with a patch. Best for several changes, several files, new files, deletes, and renames in one call. The input is the whole patch text: *** Begin Patch, then file sections (*** Add File: path, *** Update File: path with optional *** Move to: path, *** Delete File: path), then *** End Patch. In updates, @@ lines name nearby code to anchor a change, lines starting with space are kept, - removes, + adds. Prefix every new line with +.",
            "parameters": {"type": "object", "properties": {
                "input": {"type": "string", "description": "the full patch, from *** Begin Patch to *** End Patch"}
            }, "required": ["input"]}
        }},
        {"type": "function", "function": {
            "name": "plan",
            "description": "Keep a short step-by-step plan for multi-step work. Send the whole plan each time. Exactly one step is in_progress until everything is completed.",
            "parameters": {"type": "object", "properties": {
                "explanation": {"type": "string"},
                "plan": {"type": "array", "items": {"type": "object", "properties": {
                    "step": {"type": "string"},
                    "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}
                }, "required": ["step", "status"]}}
            }, "required": ["plan"]}
        }},
        {"type": "function", "function": {
            "name": "grep",
            "description": "Search file contents with a regular expression. Returns file:line:text matches.",
            "parameters": {"type": "object", "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string", "description": "folder or file to search, default the project"},
                "glob": {"type": "string", "description": "only search files matching this glob, like *.ts"}
            }, "required": ["pattern"]}
        }},
        {"type": "function", "function": {
            "name": "glob",
            "description": "Find files by name pattern, like **/*.rs or *.md.",
            "parameters": {"type": "object", "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"}
            }, "required": ["pattern"]}
        }},
        {"type": "function", "function": {
            "name": "fetch",
            "description": "Fetch a web page or file over http and return it as text.",
            "parameters": {"type": "object", "properties": {
                "url": {"type": "string"}
            }, "required": ["url"]}
        }},
        {"type": "function", "function": {
            "name": SHELL,
            "description": if cfg!(windows) {
                "Run a PowerShell command in the project folder and return its output."
            } else {
                "Run a shell command in the project folder and return its output."
            },
            "parameters": {"type": "object", "properties": {
                "command": {"type": "string"},
                "timeout": {"type": "integer", "description": "seconds before the command is stopped, default 120"}
            }, "required": ["command"]}
        }}
    ])
}

pub fn safe(command: &str) -> bool {
    if command
        .chars()
        .any(|character| matches!(character, ';' | '&' | '|' | '>' | '<' | '`' | '$' | '\n'))
    {
        return false;
    }
    let mut words = command.split_whitespace();
    match words.next() {
        Some("git") => words.next().is_some_and(|word| GIT.contains(&word)),
        Some(word) => SAFE.contains(&word),
        None => false,
    }
}

pub fn inside(cwd: &Path, path: &str) -> bool {
    let root = sandbox::real(cwd);
    let mut whole = PathBuf::new();
    for part in resolve(cwd, path).components() {
        match part {
            Component::ParentDir => {
                whole.pop();
            }
            Component::CurDir => {}
            other => whole.push(other),
        }
    }
    let mut existing = whole.clone();
    let mut tail = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                existing.pop();
            }
            None => break,
        }
    }
    let mut real = sandbox::real(&existing);
    for name in tail.iter().rev() {
        real.push(name);
    }
    real.starts_with(&root)
}

pub fn rule(name: &str, input: &Value, cwd: &Path) -> Option<String> {
    if name.starts_with("mcp__") {
        return Some(format!("mcp:{name}"));
    }
    match name {
        "patch" => {
            let hunks =
                patch::parse(input["input"].as_str().unwrap_or_default()).unwrap_or_default();
            let outside = hunks
                .iter()
                .flat_map(patch::Hunk::paths)
                .find(|path| !inside(cwd, path))
                .map(|path| format!("outside:{path}"));
            Some(outside.unwrap_or_else(|| "edit".to_string()))
        }
        "write" | "edit" => {
            let path = input["path"].as_str().unwrap_or_default();
            Some(if inside(cwd, path) {
                "edit".to_string()
            } else {
                format!("outside:{path}")
            })
        }
        "fetch" => input["url"]
            .as_str()
            .and_then(fetch::host)
            .map(|host| format!("fetch:{host}")),
        "bash" | "shell" => input["command"]
            .as_str()
            .filter(|command| !safe(command))
            .map(|command| format!("bash:{command}")),
        _ => None,
    }
}

pub fn always(name: &str, input: &Value, cwd: &Path) -> String {
    if name.starts_with("mcp__") {
        return format!(
            "don't ask again for {} in this chat",
            name.trim_start_matches("mcp__").replacen("__", " ", 1)
        );
    }
    match name {
        "bash" | "shell" => format!(
            "don't ask again for `{}` in this chat",
            input["command"].as_str().unwrap_or_default()
        ),
        "fetch" => format!(
            "don't ask again for {} in this chat",
            input["url"]
                .as_str()
                .and_then(fetch::host)
                .unwrap_or("this site")
        ),
        "patch" => match rule(name, input, cwd)
            .and_then(|rule| rule.strip_prefix("outside:").map(str::to_string))
        {
            Some(path) => format!("allow edits to `{path}` for the rest of this chat"),
            None => "allow edits for the rest of this chat".to_string(),
        },
        _ if !inside(cwd, input["path"].as_str().unwrap_or_default()) => format!(
            "allow edits to `{}` for the rest of this chat",
            input["path"].as_str().unwrap_or_default()
        ),
        _ => "allow edits for the rest of this chat".to_string(),
    }
}

pub fn change(name: &str, input: &Value, cwd: &Path) -> Vec<String> {
    let mark = |prefix: char, text: &str| -> Vec<String> {
        text.lines().map(|line| format!("{prefix}{line}")).collect()
    };
    match name {
        "patch" => patch::parse(input["input"].as_str().unwrap_or_default())
            .unwrap_or_default()
            .iter()
            .flat_map(|hunk| match hunk {
                patch::Hunk::Add { lines, .. } => {
                    lines.iter().map(|line| format!("+{line}")).collect()
                }
                patch::Hunk::Delete { path } => vec![format!("-{path}")],
                patch::Hunk::Update { chunks, .. } => chunks
                    .iter()
                    .flat_map(|chunk| {
                        let mut lines: Vec<String> = chunk
                            .old
                            .iter()
                            .filter(|line| !chunk.new.contains(line))
                            .map(|line| format!("-{line}"))
                            .collect();
                        lines.extend(
                            chunk
                                .new
                                .iter()
                                .filter(|line| !chunk.old.contains(line))
                                .map(|line| format!("+{line}")),
                        );
                        lines
                    })
                    .collect(),
            })
            .collect(),
        "edit" => {
            let mut lines = mark('-', input["old"].as_str().unwrap_or_default());
            lines.extend(mark('+', input["new"].as_str().unwrap_or_default()));
            lines
        }
        "write" => {
            let before = input["path"]
                .as_str()
                .and_then(|path| fs::read_to_string(resolve(cwd, path)).ok())
                .unwrap_or_default();
            let mut lines = mark('-', &before);
            lines.extend(mark('+', input["content"].as_str().unwrap_or_default()));
            lines
        }
        _ => Vec::new(),
    }
}

pub fn resolve(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

pub fn run(
    name: &str,
    id: &str,
    input: &Value,
    cwd: &Path,
    cancel: &AtomicBool,
    running: &Arc<Mutex<Option<u32>>>,
    sandboxed: bool,
) -> Done {
    let quiet = |text: String| Done { event: None, text };
    let result = match name {
        "read" => read(input, cwd),
        "write" => write(id, input, cwd),
        "edit" => edit(id, input, cwd),
        "grep" => search::grep(input, cwd).map(quiet),
        "glob" => search::glob(input, cwd).map(quiet),
        "fetch" => fetch::get(input).map(quiet),
        "patch" => {
            patch::apply(input["input"].as_str().unwrap_or_default(), cwd).map(|applied| Done {
                event: Some(Event::Patch {
                    id: id.to_string(),
                    lines: applied.lines,
                }),
                text: applied.summary,
            })
        }
        "plan" => Ok(plan(id, input)),
        "goal" => Ok(goal(input)),
        "bash" | "shell" => bash(id, input, cwd, cancel, running, sandboxed),
        other => Err(format!("there is no tool called {other}")),
    };
    result.unwrap_or_else(|text| Done {
        event: Some(Event::Output {
            failed: true,
            id: id.to_string(),
            text: text.clone(),
        }),
        text: format!("error: {text}"),
    })
}

fn plan(id: &str, input: &Value) -> Done {
    let steps: Vec<(String, String)> = input["plan"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|step| {
            Some((
                step["step"].as_str()?.to_string(),
                step["status"].as_str().unwrap_or("pending").to_string(),
            ))
        })
        .collect();
    Done {
        event: Some(Event::Plan {
            id: id.to_string(),
            steps,
        }),
        text: "plan updated".to_string(),
    }
}

fn goal(input: &Value) -> Done {
    let status = input["status"].as_str().unwrap_or("complete").to_string();
    let reason = input["evidence"]
        .as_str()
        .or_else(|| input["reason"].as_str())
        .unwrap_or_default()
        .to_string();
    Done {
        event: Some(Event::Goal {
            reason,
            status: status.clone(),
        }),
        text: format!("goal marked {status}"),
    }
}

pub fn objective() -> Value {
    json!({"type": "function", "function": {
        "name": "goal",
        "description": "Report on the active goal. Call it with complete only when current evidence proves every requirement is met and nothing required is left. Call it with blocked only after you tried more than once and can't continue without the user. Say why in reason.",
        "parameters": {"type": "object", "properties": {
            "status": {"type": "string", "enum": ["complete", "blocked"]},
            "evidence": {"type": "string", "description": "for complete: the commands you ran and what they showed"},
            "reason": {"type": "string", "description": "for blocked: what stops you and what you need from the user"}
        }, "required": ["status"]}
    }})
}

fn path(input: &Value, cwd: &Path) -> Result<PathBuf, String> {
    input["path"]
        .as_str()
        .filter(|path| !path.trim().is_empty())
        .map(|path| resolve(cwd, path))
        .ok_or_else(|| "path is missing".to_string())
}

fn read(input: &Value, cwd: &Path) -> Result<Done, String> {
    let file = path(input, cwd)?;
    if file.is_dir() {
        return search::list(&file).map(|text| Done { event: None, text });
    }
    let text = fs::read_to_string(&file)
        .map_err(|error| format!("couldn't read {}: {error}", file.display()))?;
    let offset = input["offset"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
        .max(1);
    let limit = input["limit"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(WINDOW)
        .min(LINES);
    let total = text.lines().count();
    let numbered: Vec<String> = text
        .lines()
        .enumerate()
        .skip(offset - 1)
        .take(limit)
        .map(|(index, line)| {
            format!(
                "{:>6}\t{}",
                index + 1,
                line.chars().take(2_000).collect::<String>()
            )
        })
        .collect();
    let shown = numbered.len();
    let mut text = if numbered.is_empty() {
        format!(
            "{} has {total} lines, so there's nothing at line {offset}",
            file.display()
        )
    } else {
        numbered.join("\n")
    };
    if shown > 0 && offset - 1 + shown < total {
        let last = offset - 1 + shown;
        text.push_str(&format!(
            "\n(lines {offset}-{last} of {total}. read with offset={} for more)",
            last + 1
        ));
    }
    Ok(Done { event: None, text })
}

fn write(id: &str, input: &Value, cwd: &Path) -> Result<Done, String> {
    let file = path(input, cwd)?;
    let content = input["content"]
        .as_str()
        .ok_or_else(|| "content is missing".to_string())?;
    let lines = change("write", input, cwd);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("couldn't create {}: {error}", parent.display()))?;
    }
    fs::write(&file, content)
        .map_err(|error| format!("couldn't write {}: {error}", file.display()))?;
    Ok(Done {
        event: Some(Event::Patch {
            id: id.to_string(),
            lines,
        }),
        text: format!("wrote {}", file.display()),
    })
}

fn missing(text: &str, old: &str, file: &Path) -> String {
    let first = old
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let near = text
        .lines()
        .enumerate()
        .find(|(_, line)| !first.is_empty() && line.contains(first))
        .map(|(index, line)| {
            format!(
                " the closest match is line {}: `{}`. copy the text exactly, including spaces",
                index + 1,
                line.trim()
            )
        });
    format!(
        "old text wasn't found in {}.{}",
        file.display(),
        near.unwrap_or_else(|| " read the file again, it may have changed".to_string())
    )
}

fn edit(id: &str, input: &Value, cwd: &Path) -> Result<Done, String> {
    let file = path(input, cwd)?;
    let old = input["old"]
        .as_str()
        .filter(|old| !old.is_empty())
        .ok_or_else(|| "old is missing".to_string())?;
    let new = input["new"].as_str().unwrap_or_default();
    let text = fs::read_to_string(&file)
        .map_err(|error| format!("couldn't read {}: {error}", file.display()))?;
    let count = text.matches(old).count();
    let all = input["all"].as_bool().unwrap_or(false);
    let changed = match count {
        0 => {
            return Err(missing(&text, old, &file));
        }
        1 => text.replacen(old, new, 1),
        _ if all => text.replace(old, new),
        _ => {
            let lines: Vec<String> = text
                .match_indices(old)
                .take(5)
                .map(|(at, _)| (text[..at].matches('\n').count() + 1).to_string())
                .collect();
            return Err(format!(
                "old text appears {count} times in {} (lines {}). include more surrounding lines so it matches once, or set all to change every one",
                file.display(),
                lines.join(", ")
            ));
        }
    };
    fs::write(&file, changed)
        .map_err(|error| format!("couldn't write {}: {error}", file.display()))?;
    Ok(Done {
        event: Some(Event::Patch {
            id: id.to_string(),
            lines: change("edit", input, cwd),
        }),
        text: format!("edited {}", file.display()),
    })
}

fn bash(
    id: &str,
    input: &Value,
    cwd: &Path,
    cancel: &AtomicBool,
    running: &Arc<Mutex<Option<u32>>>,
    sandboxed: bool,
) -> Result<Done, String> {
    let command = input["command"]
        .as_str()
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| "command is missing".to_string())?;
    let seconds = input["timeout"].as_u64().unwrap_or(120).clamp(1, 600);
    let inner = process::shell(command);
    let wrapped = sandboxed.then(|| sandbox::wrap(cwd, &inner)).flatten();
    let sandboxed = wrapped.is_some();
    let mut launch = wrapped.unwrap_or(inner);
    let mut child = process::group(&mut launch)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("couldn't start the shell: {error}"))?;
    let _tracked = process::Tracked::new(child.id());
    if let Ok(mut slot) = running.lock() {
        *slot = Some(child.id());
    }
    let reader = child.stdout.take().map(|mut stdout| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            bytes
        })
    });
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut late = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if cancel.load(Ordering::SeqCst) => break None,
            Ok(None) if Instant::now() >= deadline => {
                late = true;
                process::kill(child.id());
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(30)),
            Err(_) => break None,
        }
    };
    if let Ok(mut slot) = running.lock() {
        *slot = None;
    }
    let bytes = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let output = String::from_utf8_lossy(&bytes).trim_end().to_string();
    let code = status.and_then(|status| status.code());
    let failed = late || code.is_none_or(|code| code != 0);
    let length = output.chars().count();
    let mut text: String = if length > LIMIT {
        let head: String = output.chars().take(LIMIT / 2).collect();
        let tail: String = output.chars().skip(length - LIMIT / 2).collect();
        format!("{head}\n… {} characters cut …\n{tail}", length - LIMIT)
    } else {
        output.clone()
    };
    if late {
        text.push_str(&format!("\nstopped after {seconds}s"));
    } else if let Some(code) = code.filter(|code| *code != 0) {
        text.push_str(&format!("\nexit code {code}"));
    }
    if sandboxed && failed && sandbox::blocked(&output) {
        text.push_str("\nsombre's sandbox only allows writes inside the project folder, temp folders, and tool caches. if this needs more, ask the user to run /permissions yolo");
    }
    let shown = text.trim().to_string();
    Ok(Done {
        event: (!shown.is_empty() || failed).then(|| Event::Output {
            failed,
            id: id.to_string(),
            text: shown,
        }),
        text: if text.trim().is_empty() {
            "the command ran and printed nothing".to_string()
        } else {
            text
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> (AtomicBool, Arc<Mutex<Option<u32>>>) {
        (AtomicBool::new(false), Arc::new(Mutex::new(None)))
    }

    #[test]
    fn gives_each_model_family_one_way_to_edit() {
        let names = |model: &str, goal: bool| -> Vec<String> {
            schema(model, goal, false)
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
                .collect()
        };
        let gpt = names("openai/gpt-6.1-sol", false);
        assert!(gpt.contains(&"patch".to_string()));
        assert!(!gpt.contains(&"edit".to_string()));
        let claude = names("anthropic/claude-sonnet-5.5", false);
        assert!(claude.contains(&"edit".to_string()) && claude.contains(&"write".to_string()));
        assert!(!claude.contains(&"patch".to_string()));
        assert_eq!(names("qwen3-coder:30b", false).len(), 8);
        let local: Vec<String> = schema("gpt-oss:20b", false, true)
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
            .collect();
        assert_eq!(local, ["read", "write", "edit", "grep", SHELL]);
        assert!(names("llama3", true).contains(&"goal".to_string()));
        assert!(patches("gpt-oss:20b"));
        assert!(!patches("mistral"));
    }

    #[test]
    fn lets_read_only_commands_through() {
        assert!(safe("ls -la"));
        assert!(safe("git status"));
        assert!(safe("rg todo src"));
        assert!(!safe("git push"));
        assert!(!safe("rm -rf build"));
        assert!(!safe("cat a.txt | sh"));
        assert!(!safe("echo hi > a.txt"));
        assert!(!safe("ls $(rm x)"));
        assert!(!safe(""));
    }

    #[test]
    fn asks_once_per_kind_of_change() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let cwd = dir.path();
        assert_eq!(
            rule("write", &json!({"path": "a.txt"}), cwd),
            Some("edit".to_string())
        );
        assert_eq!(
            rule("edit", &json!({"path": "src/b.rs"}), cwd),
            Some("edit".to_string())
        );
        assert_eq!(
            rule("edit", &json!({"path": "/etc/hosts"}), cwd),
            Some("outside:/etc/hosts".to_string())
        );
        assert_eq!(
            rule("bash", &json!({"command": "touch b.txt"}), cwd),
            Some("bash:touch b.txt".to_string())
        );
        assert_eq!(rule("bash", &json!({"command": "ls"}), cwd), None);
        assert_eq!(rule("read", &json!({"path": "a"}), cwd), None);
        assert_eq!(rule("grep", &json!({"pattern": "x"}), cwd), None);
        assert_eq!(
            rule("fetch", &json!({"url": "https://sombre.ai/docs"}), cwd),
            Some("fetch:sombre.ai".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn knows_what_is_inside_the_project() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let cwd = dir.path().join("project");
        fs::create_dir_all(cwd.join("src")).unwrap_or_else(|error| panic!("{error}"));
        assert!(inside(&cwd, "src/new.rs"));
        assert!(inside(&cwd, "deep/new/folder/file.txt"));
        assert!(inside(&cwd, &cwd.join("a.txt").display().to_string()));
        assert!(!inside(&cwd, "../escape.txt"));
        assert!(!inside(&cwd, "src/../../escape.txt"));
        assert!(!inside(&cwd, "/etc/hosts"));
        std::os::unix::fs::symlink(dir.path(), cwd.join("link"))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!inside(&cwd, "link/escape.txt"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sandboxed_commands_can_only_write_inside() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let probe = process::home()
            .map(|home| home.join(".sombre-sandbox-probe").display().to_string())
            .unwrap_or_default();
        let ok = run(
            "bash",
            "c1",
            &json!({"command": "echo hi > inside.txt && cat inside.txt"}),
            dir.path(),
            &cancel,
            &running,
            true,
        );
        assert_eq!(ok.text, "hi");
        let blocked = run(
            "bash",
            "c2",
            &json!({"command": format!("echo no > {probe}")}),
            dir.path(),
            &cancel,
            &running,
            true,
        );
        let leaked = Path::new(&probe).exists();
        let _ = fs::remove_file(&probe);
        assert!(!leaked, "the sandbox let a write out");
        assert!(
            blocked.text.contains("sombre's sandbox only allows writes"),
            "{}",
            blocked.text
        );
    }

    #[test]
    fn reads_numbered_lines() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n")
            .unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let done = run(
            "read",
            "c1",
            &json!({"path": "a.txt", "offset": 2, "limit": 1}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert_eq!(
            done.text,
            "     2\ttwo\n(lines 2-2 of 3. read with offset=3 for more)"
        );
        assert_eq!(done.event, None);
    }

    #[test]
    fn edits_exact_text_once() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let file = dir.path().join("a.txt");
        fs::write(&file, "one\ntwo\ntwo\n").unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let twice = run(
            "edit",
            "c1",
            &json!({"path": "a.txt", "old": "two", "new": "2"}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert!(twice.text.starts_with("error: old text appears 2 times"));
        let once = run(
            "edit",
            "c2",
            &json!({"path": "a.txt", "old": "one", "new": "1"}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert_eq!(
            once.event,
            Some(Event::Patch {
                id: "c2".to_string(),
                lines: vec!["-one".to_string(), "+1".to_string()]
            })
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap_or_default(),
            "1\ntwo\ntwo\n"
        );
    }

    #[test]
    fn writes_new_files_and_folders() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let done = run(
            "write",
            "c1",
            &json!({"path": "notes/b.txt", "content": "hi"}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert_eq!(
            done.event,
            Some(Event::Patch {
                id: "c1".to_string(),
                lines: vec!["+hi".to_string()]
            })
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("notes/b.txt")).unwrap_or_default(),
            "hi"
        );
    }

    #[cfg(unix)]
    #[test]
    fn runs_commands_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let ok = run(
            "bash",
            "c1",
            &json!({"command": "echo hi"}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert_eq!(ok.text, "hi");
        let bad = run(
            "bash",
            "c2",
            &json!({"command": "echo nope; exit 3"}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert_eq!(bad.text, "nope\nexit code 3");
        assert!(matches!(
            bad.event,
            Some(Event::Output { failed: true, .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn stops_commands_that_run_too_long() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let done = run(
            "bash",
            "c1",
            &json!({"command": "sleep 5", "timeout": 1}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert!(done.text.ends_with("stopped after 1s"));
    }

    #[test]
    fn explains_unknown_tools() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let (cancel, running) = quiet();
        let done = run(
            "fly",
            "c1",
            &json!({}),
            dir.path(),
            &cancel,
            &running,
            false,
        );
        assert_eq!(done.text, "error: there is no tool called fly");
    }
}

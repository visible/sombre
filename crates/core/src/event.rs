use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Session(String),
    Model(String),
    Echo,
    Note(String),
    Prompt(String),
    Delta(String),
    Text(String),
    Image(String),
    Step {
        id: String,
        label: String,
        detail: String,
    },
    Output {
        id: String,
        text: String,
        failed: bool,
    },
    Patch {
        id: String,
        lines: Vec<String>,
    },
    Plan {
        id: String,
        steps: Vec<(String, String)>,
    },
    Goal {
        status: String,
        reason: String,
    },
    Approval {
        id: String,
        tool: String,
        detail: String,
        reason: String,
        change: Vec<String>,
        input: Value,
        always: Option<String>,
        rules: Value,
    },
    Questions {
        id: String,
        input: Value,
        questions: Vec<Question>,
    },
    Done {
        millis: Option<u64>,
        cost: Option<f64>,
        left: Option<u8>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Question {
    pub header: String,
    pub text: String,
    pub choices: Vec<Choice>,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type), serde(rename = "Option"))]
pub struct Choice {
    pub label: String,
    pub about: String,
}

impl Event {
    pub fn scrubbed(self) -> Self {
        let clean = |text: String| crate::scrub::scrub(&text);
        let all = |lines: Vec<String>| lines.into_iter().map(clean).collect();
        match self {
            Self::Model(name) => Self::Model(clean(name)),
            Self::Note(text) => Self::Note(clean(text)),
            Self::Prompt(text) => Self::Prompt(clean(text)),
            Self::Delta(text) => Self::Delta(clean(text)),
            Self::Text(text) => Self::Text(clean(text)),
            Self::Failed(text) => Self::Failed(clean(text)),
            Self::Step { id, label, detail } => Self::Step {
                detail: clean(detail),
                id,
                label: clean(label),
            },
            Self::Output { id, text, failed } => Self::Output {
                failed,
                id,
                text: clean(text),
            },
            Self::Patch { id, lines } => Self::Patch {
                id,
                lines: all(lines),
            },
            Self::Plan { id, steps } => Self::Plan {
                id,
                steps: steps
                    .into_iter()
                    .map(|(step, status)| (clean(step), status))
                    .collect(),
            },
            Self::Goal { status, reason } => Self::Goal {
                reason: clean(reason),
                status,
            },
            Self::Approval {
                id,
                tool,
                detail,
                reason,
                change,
                input,
                always,
                rules,
            } => Self::Approval {
                always: always.map(clean),
                change: all(change),
                detail: clean(detail),
                id,
                input,
                reason: clean(reason),
                rules,
                tool: clean(tool),
            },
            Self::Questions {
                id,
                input,
                questions,
            } => Self::Questions {
                id,
                input,
                questions: questions
                    .into_iter()
                    .map(|question| Question {
                        choices: question
                            .choices
                            .into_iter()
                            .map(|choice| Choice {
                                about: clean(choice.about),
                                label: clean(choice.label),
                            })
                            .collect(),
                        header: clean(question.header),
                        multiple: question.multiple,
                        text: clean(question.text),
                    })
                    .collect(),
            },
            other => other,
        }
    }
}

pub fn begins(line: &str) -> bool {
    line.contains(r#""subtype":"init""#)
        && serde_json::from_str::<Value>(line)
            .is_ok_and(|entry| entry["type"] == "system" && entry["subtype"] == "init")
}

pub fn claude(line: &str) -> Vec<Event> {
    let Ok(entry) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    match entry["type"].as_str() {
        Some("system") if entry["subtype"] == "task_notification" => entry["summary"]
            .as_str()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .map(|summary| vec![Event::Note(summary.to_string())])
            .unwrap_or_default(),
        Some("system") if entry["subtype"] == "init" => [
            entry["session_id"]
                .as_str()
                .map(|id| Event::Session(id.to_string())),
            entry["model"]
                .as_str()
                .map(|model| Event::Model(model.to_string())),
        ]
        .into_iter()
        .flatten()
        .collect(),
        Some("system") if entry["subtype"] == "compact_boundary" => {
            let meta = &entry["compact_metadata"];
            vec![Event::Note(
                match (meta["pre_tokens"].as_u64(), meta["post_tokens"].as_u64()) {
                    (Some(before), Some(after)) => format!(
                        "compacted the chat from {}k to {}k tokens",
                        before / 1000,
                        after / 1000
                    ),
                    _ => "compacted the chat".to_string(),
                },
            )]
        }
        Some("user") if entry["isReplay"] == true => vec![Event::Echo],
        Some("stream_event")
            if entry["event"]["type"] == "content_block_delta"
                && entry["event"]["delta"]["type"] == "text_delta" =>
        {
            entry["event"]["delta"]["text"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| vec![Event::Delta(text.to_string())])
                .unwrap_or_default()
        }
        Some("user") => result(&entry),
        Some("assistant") if entry["is_api_error_message"] == true => Vec::new(),
        Some("assistant") => blocks(&entry["message"]["content"]),
        Some("control_request")
            if entry["request"]["subtype"] == "can_use_tool"
                && entry["request"]["tool_name"] == "AskUserQuestion"
                && !questions(&entry["request"]["input"]).is_empty() =>
        {
            vec![Event::Questions {
                id: entry["request_id"].as_str().unwrap_or_default().to_string(),
                input: entry["request"]["input"].clone(),
                questions: questions(&entry["request"]["input"]),
            }]
        }
        Some("control_request") if entry["request"]["subtype"] == "can_use_tool" => {
            let request = &entry["request"];
            let tool = request["tool_name"].as_str().unwrap_or("tool").to_string();
            let input = &request["input"];
            let description = request["description"].as_str().unwrap_or_default().trim();
            let found = summary(&tool, input);
            let detail = if found.is_empty() {
                description.to_string()
            } else {
                found
            };
            let reason = if description == detail || detail.ends_with(description) {
                String::new()
            } else {
                description.to_string()
            };
            let rules = scoped(&request["permission_suggestions"]);
            vec![Event::Approval {
                always: always(&rules),
                change: change(&tool, input),
                detail,
                id: entry["request_id"].as_str().unwrap_or_default().to_string(),
                input: input.clone(),
                reason,
                rules,
                tool,
            }]
        }
        Some("result") if entry["is_error"].as_bool().unwrap_or(false) => {
            vec![Event::Failed(explain(
                entry["result"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .unwrap_or("the turn failed"),
                entry["api_error_code"].as_str(),
            ))]
        }
        Some("result") if entry["num_turns"] == 0 => vec![Event::Done {
            cost: None,
            left: None,
            millis: Some(0),
        }],
        Some("result") => vec![Event::Done {
            cost: entry["total_cost_usd"].as_f64(),
            left: left(&entry),
            millis: entry["duration_ms"].as_u64(),
        }],
        _ => Vec::new(),
    }
}

pub(crate) fn explain(text: &str, code: Option<&str>) -> String {
    let plain = text.strip_prefix("API Error: ").map_or(text, |rest| {
        rest.trim_start_matches(|character: char| character.is_ascii_digit())
            .trim_start()
    });
    let plain = serde_json::from_str::<Value>(plain)
        .ok()
        .and_then(|body| body["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| plain.to_string());
    match code {
        Some("model_not_available") => format!("{plain} Pick another model to keep going."),
        _ if plain.starts_with("Not logged in") => {
            "Claude Code isn't signed in. Run claude in a terminal and sign in, then send this again."
                .to_string()
        }
        _ => plain,
    }
}

pub fn prompt(text: &str) -> String {
    json!({"type": "user", "message": {"role": "user", "content": text}}).to_string()
}

pub fn allow(id: &str, input: &Value) -> String {
    reply(id, &json!({"behavior": "allow", "updatedInput": input}))
}

pub fn remember(id: &str, input: &Value, rules: &Value) -> String {
    reply(
        id,
        &json!({"behavior": "allow", "updatedInput": input, "updatedPermissions": rules}),
    )
}

pub fn answered(id: &str, input: &Value, answers: &[String]) -> String {
    let mut updated = input.clone();
    updated["answers"] = input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(answers)
        .filter_map(|(question, answer)| {
            Some((
                question["question"].as_str()?.to_string(),
                Value::String(answer.clone()),
            ))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();
    reply(id, &json!({"behavior": "allow", "updatedInput": updated}))
}

pub(crate) fn questions(input: &Value) -> Vec<Question> {
    let field =
        |value: &Value, key: &str| value[key].as_str().unwrap_or_default().trim().to_string();
    input["questions"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|question| Question {
                    choices: question["options"]
                        .as_array()
                        .map(|options| {
                            options
                                .iter()
                                .map(|option| Choice {
                                    about: field(option, "description"),
                                    label: field(option, "label"),
                                })
                                .filter(|choice| !choice.label.is_empty())
                                .collect()
                        })
                        .unwrap_or_default(),
                    header: field(question, "header"),
                    multiple: question["multiSelect"].as_bool().unwrap_or(false),
                    text: field(question, "question"),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn permission(mode: &str) -> String {
    json!({
        "type": "control_request",
        "request_id": crate::clock::uuid(),
        "request": {"subtype": "set_permission_mode", "mode": mode}
    })
    .to_string()
}

pub fn halt(id: &str) -> String {
    reply(
        id,
        &json!({
            "behavior": "deny",
            "message": "the user said no and will say what to do instead",
            "interrupt": true
        }),
    )
}

fn result(entry: &Value) -> Vec<Event> {
    let outcome = &entry["tool_use_result"];
    let blocks = entry["message"]["content"].as_array();
    let Some(first) = blocks.and_then(|blocks| blocks.first()) else {
        return Vec::new();
    };
    if first["type"] != "tool_result" {
        return Vec::new();
    }
    let failed = first["is_error"] == true;
    let id = text(&first["tool_use_id"]);
    if let Some(hunks) = outcome["structuredPatch"]
        .as_array()
        .filter(|hunks| !hunks.is_empty())
    {
        let lines: Vec<String> = hunks
            .iter()
            .filter_map(|hunk| hunk["lines"].as_array())
            .flatten()
            .filter_map(Value::as_str)
            .filter(|line| line.starts_with('+') || line.starts_with('-'))
            .map(str::to_string)
            .collect();
        return vec![Event::Patch { id, lines }];
    }
    if outcome["type"] == "create" {
        let lines = outcome["content"]
            .as_str()
            .unwrap_or_default()
            .lines()
            .map(|line| format!("+{line}"))
            .collect();
        return vec![Event::Patch { id, lines }];
    }
    let text = match outcome["stdout"].as_str() {
        Some(stdout) => {
            let stderr = outcome["stderr"].as_str().unwrap_or_default();
            [stdout.trim_end(), stderr.trim_end()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
        None if failed => content(&first["content"]),
        None => return Vec::new(),
    };
    if text.trim().is_empty() {
        return Vec::new();
    }
    vec![Event::Output { failed, id, text }]
}

fn content(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim_end().to_string(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn left(entry: &Value) -> Option<u8> {
    let window = entry["modelUsage"]
        .as_object()?
        .values()
        .filter_map(|usage| usage["contextWindow"].as_u64())
        .max()
        .filter(|window| *window > 0)?;
    let last = entry["usage"]["iterations"].as_array()?.last()?;
    let used: u64 = [
        "input_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .filter_map(|key| last[*key].as_u64())
    .sum();
    u8::try_from(100_u64.saturating_sub(used.saturating_mul(100) / window)).ok()
}

fn scoped(suggestions: &Value) -> Value {
    Value::Array(
        suggestions
            .as_array()
            .into_iter()
            .flatten()
            .filter(|suggestion| {
                matches!(suggestion["type"].as_str(), Some("setMode" | "addRules"))
            })
            .map(|suggestion| {
                let mut copy = suggestion.clone();
                copy["destination"] = json!("session");
                copy
            })
            .collect(),
    )
}

fn always(rules: &Value) -> Option<String> {
    let first = rules.as_array()?.first()?;
    match first["type"].as_str()? {
        "setMode" if first["mode"] == "acceptEdits" => {
            Some("allow edits for the rest of this chat".to_string())
        }
        "addRules" => {
            let rule = &first["rules"][0];
            let tool = rule["toolName"].as_str().unwrap_or("this tool");
            Some(match rule["ruleContent"].as_str() {
                Some(content) => format!("don't ask again for `{content}` in this chat"),
                None => format!("don't ask again for {tool} in this chat"),
            })
        }
        _ => None,
    }
}

fn change(tool: &str, input: &Value) -> Vec<String> {
    let mark = |prefix: char, text: &str| -> Vec<String> {
        text.lines().map(|line| format!("{prefix}{line}")).collect()
    };
    match tool {
        "Edit" => {
            let mut lines = mark('-', input["old_string"].as_str().unwrap_or_default());
            lines.extend(mark('+', input["new_string"].as_str().unwrap_or_default()));
            lines
        }
        "Write" => mark('+', input["content"].as_str().unwrap_or_default()),
        _ => Vec::new(),
    }
}

fn reply(id: &str, response: &Value) -> String {
    json!({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": id, "response": response}
    })
    .to_string()
}

pub(crate) fn blocks(content: &Value) -> Vec<Event> {
    content
        .as_array()
        .map(|blocks| blocks.iter().filter_map(block).collect())
        .unwrap_or_default()
}

pub(crate) fn shell(command: &str) -> String {
    let inner = command
        .split_once(" -lc ")
        .filter(|(program, _)| program.ends_with("sh"))
        .map_or(command, |(_, script)| script);
    ['\'', '"']
        .iter()
        .find_map(|quote| {
            inner
                .strip_prefix(*quote)
                .and_then(|script| script.strip_suffix(*quote))
        })
        .unwrap_or(inner)
        .to_string()
}

fn block(block: &Value) -> Option<Event> {
    match block["type"].as_str()? {
        "text" => block["text"]
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .map(|text| Event::Text(text.to_string())),
        "tool_use" => {
            let name = block["name"].as_str()?;
            Some(Event::Step {
                detail: summary(name, &block["input"]),
                id: text(&block["id"]),
                label: label(name).to_string(),
            })
        }
        _ => None,
    }
}

pub(crate) fn label(tool: &str) -> &str {
    match tool {
        "Bash" => "Run",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "Edit",
        "Grep" | "Glob" => "Find",
        "WebFetch" | "WebSearch" => "Web",
        "Task" | "Agent" => "Task",
        "AskUserQuestion" => "Ask",
        other => other,
    }
}

pub(crate) fn summary(tool: &str, input: &Value) -> String {
    let key = match tool {
        "Bash" => "command",
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "file_path",
        "Grep" | "Glob" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        "Task" | "Agent" => "description",
        "AskUserQuestion" => {
            return input["questions"][0]["question"]
                .as_str()
                .unwrap_or_default()
                .to_string();
        }
        _ => return String::new(),
    };
    input[key].as_str().unwrap_or_default().to_string()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_every_text_an_agent_sends() {
        let step = Event::Step {
            detail: "cat \u{1b}[2Jx".to_string(),
            id: "1".to_string(),
            label: "Run".to_string(),
        }
        .scrubbed();
        assert_eq!(
            step,
            Event::Step {
                detail: "cat x".to_string(),
                id: "1".to_string(),
                label: "Run".to_string()
            }
        );
        assert_eq!(
            Event::Delta("\u{1b}]52;c;eA==\u{7}hi".to_string()).scrubbed(),
            Event::Delta("hi".to_string())
        );
    }

    #[test]
    fn claude_reads_the_session_and_model_from_init() {
        assert_eq!(
            claude(
                r#"{"type":"system","subtype":"init","session_id":"s1","cwd":"/w","model":"claude-sonnet-5-5"}"#
            ),
            vec![
                Event::Session("s1".to_string()),
                Event::Model("claude-sonnet-5-5".to_string())
            ]
        );
    }

    #[test]
    fn claude_streams_text_deltas() {
        assert_eq!(
            claude(
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"\n2\n3"}}}"#
            ),
            vec![Event::Delta("\n2\n3".to_string())]
        );
        assert!(claude(r#"{"type":"stream_event","event":{"type":"message_start"}}"#).is_empty());
    }

    #[test]
    fn claude_reports_compaction() {
        assert_eq!(
            claude(
                r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual","pre_tokens":36076,"post_tokens":12631}}"#
            ),
            vec![Event::Note(
                "compacted the chat from 36k to 12k tokens".to_string()
            )]
        );
    }

    #[test]
    fn claude_ignores_hooks_and_other_system_events() {
        assert!(
            claude(r#"{"type":"system","subtype":"hook_started","session_id":"s1"}"#).is_empty()
        );
        assert!(claude("not json").is_empty());
    }

    #[test]
    fn claude_marks_the_echo_of_a_sent_message() {
        assert_eq!(
            claude(
                r#"{"type":"user","session_id":"s1","parent_tool_use_id":null,"isReplay":true,"message":{"role":"user","content":"run ls"}}"#
            ),
            vec![Event::Echo]
        );
        assert!(
            claude(
                r#"{"type":"user","session_id":"s1","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":""}]}}"#
            )
            .is_empty()
        );
    }

    #[test]
    fn claude_turns_text_and_tools_into_events() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"thinking","thinking":"hmm"},
            {"type":"text","text":"Looking at the test."},
            {"type":"tool_use","name":"Bash","input":{"command":"bun test checkout"}},
            {"type":"tool_use","name":"Edit","input":{"file_path":"/w/tests/checkout.spec.ts"}},
            {"type":"tool_use","name":"mcp__docs__search","input":{"q":"x"}}
        ]}}"#;
        assert_eq!(
            claude(line),
            vec![
                Event::Text("Looking at the test.".to_string()),
                Event::Step {
                    id: String::new(),
                    detail: "bun test checkout".to_string(),
                    label: "Run".to_string()
                },
                Event::Step {
                    id: String::new(),
                    detail: "/w/tests/checkout.spec.ts".to_string(),
                    label: "Edit".to_string()
                },
                Event::Step {
                    id: String::new(),
                    detail: String::new(),
                    label: "mcp__docs__search".to_string()
                },
            ]
        );
    }

    const ASK: &str = r#"{"type":"control_request","request_id":"q1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","display_name":"AskUserQuestion","input":{"questions":[{"question":"Which color should the button be?","header":"Color","options":[{"label":"Ember","description":"The brand orange"},{"label":"Ink","description":"Plain and dark"}],"multiSelect":false},{"question":"Which pages need it?","header":"Pages","options":[{"label":"Home","description":""},{"label":"Docs","description":""}],"multiSelect":true}]},"requires_user_interaction":true,"tool_use_id":"t9"}}"#;

    #[test]
    fn claude_turns_ask_user_question_into_questions() {
        let events = claude(ASK);
        let [
            Event::Questions {
                id,
                questions,
                input,
            },
        ] = events.as_slice()
        else {
            panic!("expected questions");
        };
        assert_eq!(id, "q1");
        assert_eq!(input["questions"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            questions[0],
            Question {
                choices: vec![
                    Choice {
                        about: "The brand orange".to_string(),
                        label: "Ember".to_string(),
                    },
                    Choice {
                        about: "Plain and dark".to_string(),
                        label: "Ink".to_string(),
                    },
                ],
                header: "Color".to_string(),
                multiple: false,
                text: "Which color should the button be?".to_string(),
            }
        );
        assert!(questions[1].multiple);
    }

    #[test]
    fn answers_go_back_inside_the_original_questions() {
        let events = claude(ASK);
        let [Event::Questions { input, .. }] = events.as_slice() else {
            panic!("expected questions");
        };
        let line = answered(
            "q1",
            input,
            &["Ember".to_string(), "Home, Docs".to_string()],
        );
        let sent: Value = serde_json::from_str(&line).unwrap_or_default();
        let response = &sent["response"]["response"];
        assert_eq!(sent["response"]["request_id"], "q1");
        assert_eq!(response["behavior"], "allow");
        assert_eq!(response["updatedInput"]["questions"], input["questions"]);
        assert_eq!(
            response["updatedInput"]["answers"],
            json!({"Which color should the button be?": "Ember", "Which pages need it?": "Home, Docs"})
        );
    }

    #[test]
    fn a_question_without_any_text_stays_an_approval() {
        let line = r#"{"type":"control_request","request_id":"q2","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[]}}}"#;
        assert!(matches!(claude(line).as_slice(), [Event::Approval { .. }]));
    }

    #[test]
    fn claude_turns_permission_requests_into_approvals() {
        let line = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Edit","display_name":"Edit","input":{"file_path":"/w/a.txt","old_string":"two","new_string":"TWO","replace_all":false},"description":"a.txt","permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],"tool_use_id":"t1"}}"#;
        assert_eq!(
            claude(line),
            vec![Event::Approval {
                always: Some("allow edits for the rest of this chat".to_string()),
                change: vec!["-two".to_string(), "+TWO".to_string()],
                detail: "/w/a.txt".to_string(),
                id: "r1".to_string(),
                input: json!({"file_path": "/w/a.txt", "old_string": "two", "new_string": "TWO", "replace_all": false}),
                reason: String::new(),
                rules: json!([{"type": "setMode", "mode": "acceptEdits", "destination": "session"}]),
                tool: "Edit".to_string(),
            }]
        );
    }

    #[test]
    fn claude_keeps_command_rules_to_this_chat() {
        let line = r#"{"type":"control_request","request_id":"r2","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"touch b.txt"},"description":"Create b.txt","permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"touch b.txt"}],"behavior":"allow","destination":"localSettings"},{"type":"addDirectories","directories":["/w"],"destination":"session"}]}}"#;
        let events = claude(line);
        let [
            Event::Approval {
                always,
                detail,
                reason,
                rules,
                ..
            },
        ] = events.as_slice()
        else {
            panic!("expected one approval");
        };
        assert_eq!(detail, "touch b.txt");
        assert_eq!(reason, "Create b.txt");
        assert_eq!(
            always.as_deref(),
            Some("don't ask again for `touch b.txt` in this chat")
        );
        assert_eq!(
            rules,
            &json!([{"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "touch b.txt"}], "behavior": "allow", "destination": "session"}])
        );
    }

    #[test]
    fn claude_shows_command_output_and_errors() {
        assert_eq!(
            claude(
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"1\n2","is_error":false}]},"tool_use_result":{"stdout":"1\n2","stderr":"","interrupted":false}}"#
            ),
            vec![Event::Output {
                id: "t".to_string(),
                failed: false,
                text: "1\n2".to_string()
            }]
        );
        assert_eq!(
            claude(
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"Exit code 1\nnope","is_error":true}]},"tool_use_result":"Error: Exit code 1"}"#
            ),
            vec![Event::Output {
                id: "t".to_string(),
                failed: true,
                text: "Exit code 1\nnope".to_string()
            }]
        );
        assert!(
            claude(
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"1\tone"}]},"tool_use_result":{"type":"text","file":{"filePath":"/w/a.txt"}}}"#
            )
            .is_empty()
        );
    }

    #[test]
    fn claude_shows_edits_as_patches() {
        assert_eq!(
            claude(
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"updated"}]},"tool_use_result":{"filePath":"/w/a.txt","structuredPatch":[{"oldStart":1,"oldLines":3,"newStart":1,"newLines":3,"lines":[" one","-two","+TWO"," three"]}]}}"#
            ),
            vec![Event::Patch {
                id: "t".to_string(),
                lines: vec!["-two".to_string(), "+TWO".to_string()]
            }]
        );
        assert_eq!(
            claude(
                r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t","type":"tool_result","content":"created"}]},"tool_use_result":{"type":"create","filePath":"/w/b.txt","content":"hi\nthere","structuredPatch":[]}}"#
            ),
            vec![Event::Patch {
                id: "t".to_string(),
                lines: vec!["+hi".to_string(), "+there".to_string()]
            }]
        );
    }

    #[test]
    fn claude_reports_results() {
        assert_eq!(
            claude(
                r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":3200,"total_cost_usd":0.012}"#
            ),
            vec![Event::Done {
                cost: Some(0.012),
                left: None,
                millis: Some(3200)
            }]
        );
        assert_eq!(
            claude(
                r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":""}"#
            ),
            vec![Event::Failed("the turn failed".to_string())]
        );
    }

    #[test]
    fn claude_local_commands_finish_without_a_summary() {
        assert_eq!(
            claude(
                r#"{"type":"result","subtype":"success","is_error":false,"result":"No goal set","duration_ms":16,"duration_api_ms":0,"num_turns":0,"total_cost_usd":0.0018}"#
            ),
            vec![Event::Done {
                cost: None,
                left: None,
                millis: Some(0)
            }]
        );
    }

    #[test]
    fn claude_says_when_a_background_task_wakes_it() {
        let finished = r#"{"type":"system","subtype":"task_notification","task_id":"bg1","run_id":"r1","tool_use_id":"toolu_1","status":"completed","output_file":"/tmp/claude/tasks/bg1.output","summary":"Background command \"Sleep 12 seconds then echo finished\" completed (exit code 0)"}"#;
        assert_eq!(
            claude(finished),
            vec![Event::Note(
                "Background command \"Sleep 12 seconds then echo finished\" completed (exit code 0)"
                    .to_string()
            )]
        );
        assert_eq!(
            claude(
                r#"{"type":"system","subtype":"task_updated","task_id":"bg1","patch":{"status":"completed"}}"#
            ),
            Vec::new()
        );
        let init = r#"{"type":"system","subtype":"init","cwd":"/work","session_id":"s1","model":"claude-haiku-5-5"}"#;
        assert!(begins(init));
        assert!(!begins(finished));
        assert!(!begins(
            r#"{"type":"user","message":{"content":"{\"subtype\":\"init\"}"}}"#
        ));
        assert!(!begins(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"\"subtype\":\"init\""}]}}"#
        ));
        assert!(!begins("not json"));
    }

    #[test]
    fn claude_api_errors_show_once_without_the_status_code() {
        assert_eq!(
            claude(
                r#"{"type":"assistant","message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"API Error: 400 In order to access this model, your organization or workspace must have data retention enabled."}]},"error":"unknown","is_api_error_message":true,"api_error_status":400,"api_error_type":"invalid_request_error","api_error_code":"model_not_available"}"#
            ),
            Vec::new()
        );
        assert_eq!(
            claude(
                r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: 400 In order to access this model, your organization or workspace must have data retention enabled.","api_error_status":400,"api_error_type":"invalid_request_error","api_error_code":"model_not_available","terminal_reason":"api_error"}"#
            ),
            vec![Event::Failed(
                "In order to access this model, your organization or workspace must have data retention enabled. Pick another model to keep going."
                    .to_string()
            )]
        );
        assert_eq!(
            claude(
                r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}"}"#
            ),
            vec![Event::Failed("Overloaded".to_string())]
        );
        assert_eq!(
            claude(
                r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#
            ),
            vec![Event::Failed(
                "Claude Code isn't signed in. Run claude in a terminal and sign in, then send this again."
                    .to_string()
            )]
        );
    }

    #[test]
    fn claude_works_out_how_much_context_is_left() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":10,"usage":{"input_tokens":6,"cache_read_input_tokens":63040,"iterations":[{"input_tokens":2,"output_tokens":5,"cache_read_input_tokens":249000,"cache_creation_input_tokens":998}]},"modelUsage":{"claude-opus-5-5":{"contextWindow":1000000}}}"#;
        assert_eq!(
            claude(line),
            vec![Event::Done {
                cost: None,
                left: Some(75),
                millis: Some(10)
            }]
        );
    }

    #[test]
    fn codex_unwraps_double_quoted_shell_commands() {
        assert_eq!(shell("/bin/zsh -lc \"pwd; ls\""), "pwd; ls");
        assert_eq!(shell("ls -la"), "ls -la");
    }

    #[test]
    fn builds_the_messages_sombre_sends_back() {
        let sent: Value = serde_json::from_str(&prompt("fix it")).unwrap_or_default();
        assert_eq!(sent["message"]["content"], "fix it");
        let yes: Value = serde_json::from_str(&allow("r1", &json!({"a": 1}))).unwrap_or_default();
        assert_eq!(yes["response"]["request_id"], "r1");
        assert_eq!(yes["response"]["response"]["behavior"], "allow");
        assert_eq!(yes["response"]["response"]["updatedInput"]["a"], 1);
        let always: Value =
            serde_json::from_str(&remember("r2", &json!({}), &json!([{"type": "setMode"}])))
                .unwrap_or_default();
        assert_eq!(always["response"]["response"]["behavior"], "allow");
        assert_eq!(
            always["response"]["response"]["updatedPermissions"][0]["type"],
            "setMode"
        );
        let no: Value = serde_json::from_str(&halt("r3")).unwrap_or_default();
        assert_eq!(no["response"]["response"]["behavior"], "deny");
        assert_eq!(no["response"]["response"]["interrupt"], true);
    }

    #[test]
    fn switches_claude_modes_live_and_ignores_the_reply() {
        let sent: Value = serde_json::from_str(&permission("acceptEdits")).unwrap_or_default();
        assert_eq!(sent["type"], "control_request");
        assert_eq!(sent["request"]["subtype"], "set_permission_mode");
        assert_eq!(sent["request"]["mode"], "acceptEdits");
        assert!(sent["request_id"].as_str().is_some_and(|id| !id.is_empty()));
        let reply = r#"{"type":"control_response","response":{"subtype":"success","request_id":"m1","response":{"mode":"bypassPermissions"}}}"#;
        assert!(claude(reply).is_empty());
    }
}

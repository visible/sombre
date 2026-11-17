use std::collections::HashMap;

use serde_json::{Value, json};

use crate::event::{Event, shell};

#[derive(Default)]
pub struct Server {
    next: u64,
    thread: Option<String>,
    queued: Vec<(String, Value, bool)>,
    quiet: Vec<u64>,
    changes: HashMap<String, (String, Vec<String>)>,
    left: Option<u8>,
    busy: bool,
    turn: Option<String>,
    steering: Vec<u64>,
    aiming: bool,
    status: String,
    pub turns: usize,
    pub outbox: Vec<String>,
}

pub fn answer(id: &Value, decision: &Value) -> String {
    json!({"id": id, "result": {"decision": decision}}).to_string()
}

pub fn replied(input: &Value, answers: &[String]) -> String {
    let answers: serde_json::Map<String, Value> = input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .zip(answers)
        .filter_map(|(question, answer)| {
            Some((
                question["id"].as_str()?.to_string(),
                json!({"answers": [answer]}),
            ))
        })
        .collect();
    json!({"id": input["id"], "result": {"answers": answers}}).to_string()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn ending(status: &str) -> Option<(String, String)> {
    let (word, reason) = match status {
        "active" => return None,
        "complete" | "paused" | "blocked" => (status, ""),
        "usageLimited" => ("blocked", "codex hit its usage limit"),
        "budgetLimited" => ("blocked", "the goal used up its token budget"),
        _ => ("blocked", status),
    };
    Some((word.to_string(), reason.to_string()))
}

fn status(value: &Value) -> String {
    match value.as_str() {
        Some("inProgress") => "in_progress".to_string(),
        other => other.unwrap_or_default().to_string(),
    }
}

fn change(item: &Value) -> (String, Vec<String>) {
    let changes = item["changes"].as_array().map_or(&[][..], Vec::as_slice);
    let paths = changes
        .iter()
        .filter_map(|change| change["path"].as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let lines = changes
        .iter()
        .flat_map(|change| {
            let diff = change["diff"].as_str().unwrap_or_default();
            let mark = match change["kind"]["type"].as_str() {
                Some("add") => "+",
                Some("delete") => "-",
                _ => "",
            };
            diff.lines()
                .filter(|line| {
                    !line.starts_with("@@") && !line.starts_with("---") && !line.starts_with("+++")
                })
                .map(move |line| format!("{mark}{line}"))
                .collect::<Vec<_>>()
        })
        .collect();
    (paths, lines)
}

fn always(decisions: &Value) -> (Option<String>, Value) {
    let Some(decisions) = decisions.as_array() else {
        return (
            Some("don't ask again this session".to_string()),
            json!("acceptForSession"),
        );
    };
    if let Some(rule) = decisions
        .iter()
        .find(|decision| decision["acceptWithExecpolicyAmendment"].is_object())
    {
        let words: Vec<&str> = rule["acceptWithExecpolicyAmendment"]["execpolicy_amendment"]
            .as_array()
            .map(|words| words.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        return (
            Some(format!("always allow `{}`", shell(&words.join(" ")))),
            rule.clone(),
        );
    }
    if decisions
        .iter()
        .any(|decision| decision == "acceptForSession")
    {
        return (
            Some("don't ask again this session".to_string()),
            json!("acceptForSession"),
        );
    }
    (None, Value::Null)
}

impl Server {
    pub fn open(session: Option<&str>, settings: &Value) -> Self {
        let mut server = Self::default();
        server.call(
            "initialize",
            json!({"clientInfo": {"name": "sombre", "version": env!("CARGO_PKG_VERSION")}}),
        );
        server
            .outbox
            .push(json!({"method": "initialized"}).to_string());
        let mut params = settings.clone();
        match session {
            Some(id) => {
                params["threadId"] = json!(id);
                server.call("thread/resume", params);
            }
            None => server.call("thread/start", params),
        }
        server
    }

    pub fn prompt(&mut self, text: &str, overrides: &Value, images: &[String]) {
        let mut input = vec![json!({"type": "text", "text": text})];
        input.extend(
            images
                .iter()
                .map(|path| json!({"type": "localImage", "path": path})),
        );
        let mut params = overrides.clone();
        params["input"] = json!(input);
        self.ask("turn/start", params, false);
    }

    pub fn aim(&mut self, objective: &str) {
        self.aiming = true;
        self.status = "active".to_string();
        self.turns = 0;
        self.ask(
            "thread/goal/set",
            json!({"objective": objective, "origin": "user", "status": "active"}),
            false,
        );
    }

    pub fn clear(&mut self) {
        self.aiming = false;
        self.ask("thread/goal/clear", json!({"origin": "user"}), true);
    }

    pub fn waiting(&self) -> bool {
        !self.queued.is_empty() || !self.quiet.is_empty()
    }

    pub fn pause(&mut self) -> bool {
        if !self.aiming || self.thread.is_none() {
            return false;
        }
        self.aiming = false;
        self.ask(
            "thread/goal/set",
            json!({"origin": "user", "status": "paused"}),
            true,
        );
        true
    }

    pub fn steer(&mut self, text: &str, images: &[String]) -> bool {
        let (Some(thread), Some(turn)) = (self.thread.clone(), self.turn.clone()) else {
            return false;
        };
        let mut input = vec![json!({"type": "text", "text": text})];
        input.extend(
            images
                .iter()
                .map(|path| json!({"type": "localImage", "path": path})),
        );
        self.call(
            "turn/steer",
            json!({"expectedTurnId": turn, "input": input, "threadId": thread}),
        );
        self.steering.push(self.next);
        true
    }

    pub fn compact(&mut self) {
        self.ask("thread/compact/start", json!({}), false);
    }

    fn ask(&mut self, method: &str, mut params: Value, quiet: bool) {
        match self.thread.clone() {
            Some(thread) => {
                params["threadId"] = json!(thread);
                self.call(method, params);
                if quiet {
                    self.quiet.push(self.next);
                }
            }
            None => self.queued.push((method.to_string(), params, quiet)),
        }
    }

    pub fn read(&mut self, line: &str) -> Vec<Event> {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match message["method"].as_str() {
            None => self.reply(&message),
            Some(method) if message.get("id").is_some() => {
                self.request(method, &message["id"], &message["params"])
            }
            Some(method) => self.notice(method, &message["params"]),
        }
    }

    fn call(&mut self, method: &str, params: Value) {
        self.next += 1;
        self.outbox
            .push(json!({"id": self.next, "method": method, "params": params}).to_string());
    }

    fn reply(&mut self, message: &Value) -> Vec<Event> {
        if let Some(spot) = self
            .quiet
            .iter()
            .position(|id| message["id"].as_u64() == Some(*id))
        {
            self.quiet.remove(spot);
            return Vec::new();
        }
        if let Some(spot) = self
            .steering
            .iter()
            .position(|id| message["id"].as_u64() == Some(*id))
        {
            self.steering.remove(spot);
            return if message["error"].is_object() {
                vec![Event::Note(format!(
                    "codex couldn't take that message mid-turn ({}). send it again",
                    text(&message["error"]["message"])
                ))]
            } else {
                Vec::new()
            };
        }
        if message["error"].is_object() {
            return vec![Event::Failed(text(&message["error"]["message"]))];
        }
        let result = &message["result"];
        let Some(id) = result["thread"]["id"].as_str() else {
            return Vec::new();
        };
        self.thread = Some(id.to_string());
        for (method, params, quiet) in std::mem::take(&mut self.queued) {
            self.ask(&method, params, quiet);
        }
        [
            Some(Event::Session(id.to_string())),
            result["model"]
                .as_str()
                .map(|model| Event::Model(model.to_string())),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn request(&mut self, method: &str, id: &Value, params: &Value) -> Vec<Event> {
        match method {
            "item/commandExecution/requestApproval" => {
                let (always, rules) = always(&params["availableDecisions"]);
                vec![Event::Approval {
                    always,
                    change: Vec::new(),
                    detail: shell(params["command"].as_str().unwrap_or_default()),
                    id: id.to_string(),
                    input: id.clone(),
                    reason: text(&params["reason"]),
                    rules,
                    tool: "Bash".to_string(),
                }]
            }
            "item/fileChange/requestApproval" => {
                let (detail, change) = self
                    .changes
                    .get(params["itemId"].as_str().unwrap_or_default())
                    .cloned()
                    .unwrap_or_default();
                vec![Event::Approval {
                    always: Some("allow these files for the rest of the session".to_string()),
                    change,
                    detail,
                    id: id.to_string(),
                    input: id.clone(),
                    reason: text(&params["reason"]),
                    rules: json!("acceptForSession"),
                    tool: "Edit".to_string(),
                }]
            }
            "item/tool/requestUserInput" if !crate::event::questions(params).is_empty() => {
                vec![Event::Questions {
                    id: id.to_string(),
                    input: json!({"id": id, "questions": params["questions"]}),
                    questions: crate::event::questions(params),
                }]
            }
            _ => {
                self.outbox.push(
                    json!({"id": id, "error": {"code": -32601, "message": format!("sombre can't answer {method} yet")}})
                        .to_string(),
                );
                Vec::new()
            }
        }
    }

    fn notice(&mut self, method: &str, params: &Value) -> Vec<Event> {
        let item = &params["item"];
        match (method, item["type"].as_str()) {
            ("item/agentMessage/delta", _) => params["delta"]
                .as_str()
                .filter(|delta| !delta.is_empty())
                .map(|delta| vec![Event::Delta(delta.to_string())])
                .unwrap_or_default(),
            ("item/started", Some("commandExecution")) => vec![Event::Step {
                detail: shell(item["command"].as_str().unwrap_or_default()),
                id: text(&item["id"]),
                label: "Run".to_string(),
            }],
            ("item/started", Some("fileChange")) => {
                let (detail, lines) = change(item);
                self.changes
                    .insert(text(&item["id"]), (detail.clone(), lines));
                vec![Event::Step {
                    detail,
                    id: text(&item["id"]),
                    label: "Edit".to_string(),
                }]
            }
            ("item/completed", Some("commandExecution")) => {
                let output = item["aggregatedOutput"].as_str().unwrap_or_default();
                let failed = item["exitCode"].as_i64().is_some_and(|code| code != 0);
                if output.trim().is_empty() && !failed {
                    Vec::new()
                } else {
                    vec![Event::Output {
                        failed,
                        id: text(&item["id"]),
                        text: output.trim_end().to_string(),
                    }]
                }
            }
            ("item/completed", Some("imageView")) => item["path"]
                .as_str()
                .map(|path| vec![Event::Image(path.to_string())])
                .unwrap_or_default(),
            ("item/completed", Some("imageGeneration")) => item["savedPath"]
                .as_str()
                .map(|path| vec![Event::Image(path.to_string())])
                .unwrap_or_default(),
            ("item/completed", Some("contextCompaction")) => {
                self.left = None;
                vec![Event::Note("compacted the chat".to_string())]
            }
            ("turn/started", _) => {
                self.busy = true;
                self.turn = params["turn"]["id"].as_str().map(str::to_string);
                Vec::new()
            }
            ("thread/goal/updated", _) => {
                let status = text(&params["goal"]["status"]);
                if !self.aiming || status == self.status {
                    return Vec::new();
                }
                self.status.clone_from(&status);
                let Some((word, reason)) = ending(&status) else {
                    return Vec::new();
                };
                let mut events = vec![Event::Goal {
                    reason,
                    status: word,
                }];
                if !self.busy {
                    self.aiming = false;
                    events.push(Event::Done {
                        cost: None,
                        left: self.left,
                        millis: None,
                    });
                }
                events
            }
            ("item/completed", Some("agentMessage")) => item["text"]
                .as_str()
                .filter(|said| !said.trim().is_empty())
                .map(|said| vec![Event::Text(said.to_string())])
                .unwrap_or_default(),
            ("turn/plan/updated", _) => vec![Event::Plan {
                id: text(&params["turnId"]),
                steps: params["plan"]
                    .as_array()
                    .map(|plan| {
                        plan.iter()
                            .map(|step| (text(&step["step"]), status(&step["status"])))
                            .collect()
                    })
                    .unwrap_or_default(),
            }],
            ("thread/tokenUsage/updated", _) => {
                let usage = &params["tokenUsage"];
                self.left = usage["modelContextWindow"]
                    .as_u64()
                    .filter(|window| *window > 0)
                    .zip(usage["last"]["totalTokens"].as_u64())
                    .and_then(|(window, used)| {
                        u8::try_from(100_u64.saturating_sub(used.saturating_mul(100) / window)).ok()
                    });
                Vec::new()
            }
            ("turn/completed", _) => {
                let turn = &params["turn"];
                self.busy = false;
                self.turn = None;
                if self.aiming {
                    self.turns += 1;
                }
                if self.aiming && self.status == "active" && turn["status"] == "completed" {
                    return vec![Event::Note(format!(
                        "turn {} done, codex keeps going on the goal",
                        self.turns
                    ))];
                }
                self.aiming = false;
                if turn["status"] == "failed" {
                    vec![Event::Failed(text(&turn["error"]["message"]))]
                } else {
                    vec![Event::Done {
                        cost: None,
                        left: self.left,
                        millis: turn["durationMs"].as_u64(),
                    }]
                }
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREAD: &str = r#"{"id":2,"result":{"thread":{"id":"01a1c860-cd8c-7af0-b2a1-51fe7fa3a3d6","ephemeral":true,"path":null,"cwd":"/tmp/run","cliVersion":"0.167.0","originator":"sombre","source":"vscode","turns":[]},"model":"gpt-6.1-sol","modelProvider":"openai","approvalPolicy":"untrusted"}}"#;
    const FILE: &str = r#"{"method":"item/started","params":{"item":{"type":"fileChange","id":"exec-f04542c2","changes":[{"path":"/tmp/run/hi.txt","kind":{"type":"add"},"diff":"ember\n"}],"status":"inProgress","startedAtMs":1794363156220,"completedAtMs":null},"threadId":"01a1c860","turnId":"01a1c860-d0ae","startedAtMs":1794363156220},"emittedAtMs":1794363156220}"#;
    const EDIT: &str = r#"{"method":"item/fileChange/requestApproval","id":0,"params":{"threadId":"01a1c860","turnId":"01a1c860-d0ae","itemId":"exec-f04542c2","startedAtMs":1794363156221,"reason":null,"grantRoot":null}}"#;
    const COMMAND: &str = r#"{"method":"item/commandExecution/requestApproval","id":1,"params":{"kind":"command","threadId":"01a1c860","turnId":"01a1c860-d0ae","itemId":"exec-47eb82c7","startedAtMs":1794363156276,"environmentId":"local","command":"/bin/zsh -lc 'cat hi.txt'","cwd":"/tmp/run","commandActions":[{"type":"read","command":"cat hi.txt","name":"hi.txt","path":"/tmp/run/hi.txt"}],"proposedExecpolicyAmendment":["cat","hi.txt"],"availableDecisions":["accept",{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["cat","hi.txt"]}},"cancel"]}}"#;
    const RAN: &str = r#"{"method":"item/completed","params":{"item":{"type":"commandExecution","id":"exec-47eb82c7","command":"/bin/zsh -lc 'cat hi.txt'","cwd":"/tmp/run","processId":"91655","source":"unifiedExecStartup","status":"completed","commandActions":[],"aggregatedOutput":"ember\n","exitCode":0,"durationMs":0},"threadId":"01a1c860","turnId":"01a1c860-d0ae","completedAtMs":1794363156384},"emittedAtMs":1794363156384}"#;
    const USAGE: &str = r#"{"method":"thread/tokenUsage/updated","params":{"threadId":"01a1c860","turnId":"01a1c860-d0ae","tokenUsage":{"total":{"totalTokens":52788,"inputTokens":52677,"cachedInputTokens":26112,"cacheWriteInputTokens":0,"outputTokens":111,"reasoningOutputTokens":0},"last":{"totalTokens":47500,"inputTokens":47480,"cachedInputTokens":26112,"cacheWriteInputTokens":0,"outputTokens":20,"reasoningOutputTokens":0},"modelContextWindow":475000}},"emittedAtMs":1794363158882}"#;
    const DONE: &str = r#"{"method":"turn/completed","params":{"threadId":"01a1c860","turn":{"id":"01a1c860-d0ae","items":[],"itemsView":"summary","status":"completed","error":null,"startedAt":1794363150,"completedAt":1794363158,"durationMs":8386}},"emittedAtMs":1794363158902}"#;

    fn sent(server: &mut Server) -> Vec<Value> {
        server
            .outbox
            .drain(..)
            .map(|line| serde_json::from_str(&line).unwrap_or_else(|error| panic!("{error}")))
            .collect()
    }

    #[test]
    fn starts_a_thread_and_sends_the_prompt_once_it_exists() {
        let mut server = Server::open(
            None,
            &json!({"cwd": "/tmp/run", "approvalPolicy": "untrusted"}),
        );
        server.prompt(
            "make hi.txt",
            &json!({"effort": "high"}),
            &["/tmp/shot.png".to_string()],
        );
        let opening = sent(&mut server);
        let methods: Vec<&str> = opening
            .iter()
            .filter_map(|line| line["method"].as_str())
            .collect();
        assert_eq!(methods, ["initialize", "initialized", "thread/start"]);
        assert_eq!(opening[2]["params"]["approvalPolicy"], "untrusted");
        assert_eq!(
            server.read(THREAD),
            [
                Event::Session("01a1c860-cd8c-7af0-b2a1-51fe7fa3a3d6".to_string()),
                Event::Model("gpt-6.1-sol".to_string())
            ]
        );
        let turn = sent(&mut server);
        assert_eq!(turn[0]["method"], "turn/start");
        assert_eq!(
            turn[0]["params"]["threadId"],
            "01a1c860-cd8c-7af0-b2a1-51fe7fa3a3d6"
        );
        assert_eq!(turn[0]["params"]["input"][0]["text"], "make hi.txt");
        assert_eq!(turn[0]["params"]["effort"], "high");
        assert_eq!(
            turn[0]["params"]["input"][1],
            json!({"type": "localImage", "path": "/tmp/shot.png"})
        );
        server.prompt("again", &json!({"approvalPolicy": "never"}), &[]);
        let again = sent(&mut server);
        assert_eq!(again[0]["params"]["input"][0]["text"], "again");
        assert!(again[0]["params"].get("effort").is_none());
        assert_eq!(again[0]["params"]["approvalPolicy"], "never");
    }

    const GOAL: &str = r#"{"method":"thread/goal/updated","params":{"threadId":"01a1c87a","turnId":"01a1c87a-4133","goal":{"threadId":"01a1c87a","objective":"count.txt exists and contains the numbers 1 to 3, one per line","status":"STATUS","tokenBudget":null,"tokensUsed":27865,"timeUsedSeconds":9,"createdAt":1794364817,"updatedAt":1794364827}},"emittedAtMs":1794364827000}"#;
    const STARTED: &str = r#"{"method":"turn/started","params":{"threadId":"01a1c87a","turn":{"id":"01a1c87a-4133","items":[],"status":"inProgress","error":null}}}"#;

    #[test]
    fn hands_the_goal_to_codex_and_ends_when_it_reports() {
        let mut server = Server::default();
        server.aim("count to 3");
        assert!(server.outbox.is_empty());
        server.read(THREAD);
        let goal = sent(&mut server);
        assert_eq!(goal[0]["method"], "thread/goal/set");
        assert_eq!(goal[0]["params"]["status"], "active");
        assert_eq!(goal[0]["params"]["objective"], "count to 3");
        assert_eq!(
            goal[0]["params"]["threadId"],
            "01a1c860-cd8c-7af0-b2a1-51fe7fa3a3d6"
        );
        assert!(server.read(&GOAL.replace("STATUS", "active")).is_empty());
        server.read(STARTED);
        assert!(matches!(&server.read(DONE)[..], [Event::Note(_)]));
        server.read(STARTED);
        assert_eq!(
            server.read(&GOAL.replace("STATUS", "complete")),
            [Event::Goal {
                reason: String::new(),
                status: "complete".to_string()
            }]
        );
        assert!(server.read(&GOAL.replace("STATUS", "complete")).is_empty());
        assert!(matches!(&server.read(DONE)[..], [Event::Done { .. }]));
        assert_eq!(server.turns, 2);
        assert!(!server.pause());
    }

    #[test]
    fn ends_at_once_when_the_goal_stops_between_turns() {
        let mut server = Server::default();
        server.read(THREAD);
        server.aim("x");
        assert_eq!(
            server.read(&GOAL.replace("STATUS", "usageLimited")),
            [
                Event::Goal {
                    reason: "codex hit its usage limit".to_string(),
                    status: "blocked".to_string()
                },
                Event::Done {
                    cost: None,
                    left: None,
                    millis: None
                }
            ]
        );
    }

    #[test]
    fn pauses_clears_and_compacts_on_the_thread() {
        let mut server = Server::default();
        server.read(THREAD);
        server.aim("x");
        sent(&mut server);
        assert!(server.pause());
        server.clear();
        server.compact();
        assert!(server.waiting());
        let calls = sent(&mut server);
        assert_eq!(calls[0]["params"]["status"], "paused");
        assert_eq!(calls[1]["method"], "thread/goal/clear");
        assert_eq!(calls[2]["method"], "thread/compact/start");
        let pause = format!(r#"{{"id":{},"result":{{}}}}"#, calls[0]["id"]);
        let clear = format!(
            r#"{{"id":{},"error":{{"message":"no goal"}}}}"#,
            calls[1]["id"]
        );
        assert!(server.read(&pause).is_empty());
        assert!(server.read(&clear).is_empty());
        assert!(!server.waiting());
        assert_eq!(
            server.read(r#"{"method":"item/completed","params":{"item":{"type":"contextCompaction","id":"c"},"threadId":"t","turnId":"u"}}"#),
            [Event::Note("compacted the chat".to_string())]
        );
    }

    #[test]
    fn shows_images_codex_looks_at_or_makes() {
        let mut server = Server::default();
        assert_eq!(
            server.read(r#"{"method":"item/completed","params":{"item":{"type":"imageView","id":"v1","path":"/tmp/run/shot.png"},"threadId":"t","turnId":"u"}}"#),
            vec![Event::Image("/tmp/run/shot.png".to_string())]
        );
        assert_eq!(
            server.read(r#"{"method":"item/completed","params":{"item":{"type":"imageGeneration","id":"g1","status":"completed","revisedPrompt":"a sunset","image":{"type":"saved"},"savedPath":"/Users/me/.codex/generated_images/g1.png","inputImages":[]},"threadId":"t","turnId":"u"}}"#),
            vec![Event::Image("/Users/me/.codex/generated_images/g1.png".to_string())]
        );
        assert!(server.read(r#"{"method":"item/completed","params":{"item":{"type":"imageGeneration","id":"g2","status":"failed","savedPath":null},"threadId":"t","turnId":"u"}}"#).is_empty());
    }

    #[test]
    fn steers_the_running_turn_and_says_when_it_could_not() {
        let mut server = Server::default();
        server.read(THREAD);
        assert!(!server.steer("too early", &[]));
        server.read(STARTED);
        assert!(server.steer("also add tests", &["/tmp/shot.png".to_string()]));
        let call = sent(&mut server);
        assert_eq!(call[0]["method"], "turn/steer");
        assert_eq!(call[0]["params"]["expectedTurnId"], "01a1c87a-4133");
        assert_eq!(call[0]["params"]["input"][0]["text"], "also add tests");
        assert_eq!(call[0]["params"]["input"][1]["type"], "localImage");
        let failed = format!(
            r#"{{"id":{},"error":{{"message":"turn already finished"}}}}"#,
            call[0]["id"]
        );
        assert!(matches!(&server.read(&failed)[..], [Event::Note(_)]));
        server.read(DONE);
        assert!(!server.steer("after", &[]));
    }

    #[test]
    fn resumes_an_existing_thread() {
        let mut server = Server::open(Some("abc"), &json!({}));
        let opening = sent(&mut server);
        assert_eq!(opening[2]["method"], "thread/resume");
        assert_eq!(opening[2]["params"]["threadId"], "abc");
    }

    #[test]
    fn shows_the_edit_being_approved() {
        let mut server = Server::default();
        assert_eq!(
            server.read(FILE),
            [Event::Step {
                detail: "/tmp/run/hi.txt".to_string(),
                id: "exec-f04542c2".to_string(),
                label: "Edit".to_string()
            }]
        );
        let [
            Event::Approval {
                tool,
                change,
                input,
                rules,
                ..
            },
        ] = &server.read(EDIT)[..]
        else {
            panic!("expected one approval");
        };
        assert_eq!(tool, "Edit");
        assert_eq!(change, &["+ember"]);
        assert_eq!(
            answer(input, rules),
            r#"{"id":0,"result":{"decision":"acceptForSession"}}"#
        );
    }

    #[test]
    fn offers_the_rule_codex_proposed_for_commands() {
        let mut server = Server::default();
        let [
            Event::Approval {
                tool,
                detail,
                always,
                input,
                rules,
                ..
            },
        ] = &server.read(COMMAND)[..]
        else {
            panic!("expected one approval");
        };
        assert_eq!((tool.as_str(), detail.as_str()), ("Bash", "cat hi.txt"));
        assert_eq!(always.as_deref(), Some("always allow `cat hi.txt`"));
        let wrapped = json!([{"acceptWithExecpolicyAmendment": {"execpolicy_amendment": ["/bin/zsh", "-lc", "printf 'hi' > hello.txt"]}}]);
        assert_eq!(
            super::always(&wrapped).0.as_deref(),
            Some("always allow `printf 'hi' > hello.txt`")
        );
        assert_eq!(
            answer(input, rules),
            r#"{"id":1,"result":{"decision":{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["cat","hi.txt"]}}}}"#
        );
        assert_eq!(
            answer(input, &json!("cancel")),
            r#"{"id":1,"result":{"decision":"cancel"}}"#
        );
    }

    #[test]
    fn turns_a_turn_into_output_and_context_left() {
        let mut server = Server::default();
        assert_eq!(
            server.read(RAN),
            [Event::Output {
                failed: false,
                id: "exec-47eb82c7".to_string(),
                text: "ember".to_string()
            }]
        );
        assert!(server.read(USAGE).is_empty());
        assert_eq!(
            server.read(DONE),
            [Event::Done {
                cost: None,
                left: Some(90),
                millis: Some(8386)
            }]
        );
    }

    #[test]
    fn codex_questions_come_through_and_answers_go_back_by_id() {
        let mut server = Server::default();
        let events = server.read(r#"{"method":"item/tool/requestUserInput","id":9,"params":{"threadId":"t","turnId":"u","itemId":"i","isBlocking":true,"questions":[{"id":"scope","header":"Scope","question":"Which part should change?","isOther":true,"isSecret":false,"options":[{"label":"Just the api","description":"Leave the ui alone"},{"label":"Everything","description":""}]}]}}"#);
        let [
            Event::Questions {
                input, questions, ..
            },
        ] = events.as_slice()
        else {
            panic!("expected questions");
        };
        assert_eq!(questions[0].text, "Which part should change?");
        assert_eq!(questions[0].choices[0].label, "Just the api");
        assert!(!questions[0].multiple);
        let reply: Value =
            serde_json::from_str(&replied(input, &["Everything".to_string()])).unwrap_or_default();
        assert_eq!(reply["id"], 9);
        assert_eq!(
            reply["result"]["answers"],
            json!({"scope": {"answers": ["Everything"]}})
        );
        let skipped: Value = serde_json::from_str(&replied(input, &[])).unwrap_or_default();
        assert_eq!(skipped["result"]["answers"], json!({}));
    }

    #[test]
    fn streams_text_and_declines_requests_it_cannot_answer() {
        let mut server = Server::default();
        assert_eq!(
            server.read(r#"{"method":"item/agentMessage/delta","params":{"threadId":"t","turnId":"u","itemId":"m","delta":" check"}}"#),
            [Event::Delta(" check".to_string())]
        );
        assert!(
            server
                .read(r#"{"method":"item/tool/requestUserInput","id":7,"params":{}}"#)
                .is_empty()
        );
        let reply = sent(&mut server);
        assert_eq!(reply[0]["id"], 7);
        assert_eq!(reply[0]["error"]["code"], -32601);
        assert_eq!(
            server.read(r#"{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","status":"failed","error":{"message":"usage limit reached"}}}}"#),
            [Event::Failed("usage limit reached".to_string())]
        );
    }
}

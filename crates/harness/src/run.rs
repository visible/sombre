use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sombre_core::Event;
use sombre_core::sombre::{self as store, Record};

use crate::config::{Config, Mode, split};
use crate::{process, prompt, provider, tools};

const STEPS: usize = 200;
const WAITS: [u64; 3] = [5, 15, 30];
const STOPPED: &str = "stopped by the user before this ran";
const SUMMARY: &str = "This chat was compacted to save context. Here is a summary of everything before this point:\n\n";
const ASK: &str = "Summarize this conversation so you can keep working without the earlier messages. Keep the user's goals and preferences, decisions made, files changed and why, commands that matter, what is done, and what is left. Be specific and brief. Reply with the summary only.";
const REFUSED: &str = "the user said no to this. stop and wait for them to say what to do instead";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    Always,
    No,
}

#[derive(Clone, Default)]
pub struct Shared {
    pub allowed: Arc<Mutex<HashSet<String>>>,
    pub windows: Arc<Mutex<HashMap<String, u64>>>,
    pub plain: Arc<Mutex<HashSet<String>>>,
    pub flat: Arc<Mutex<HashSet<String>>>,
    pub classic: Arc<Mutex<HashSet<String>>>,
    pub blind: Arc<Mutex<HashSet<String>>>,
    pub seen: Arc<Mutex<HashSet<PathBuf>>>,
    pub pool: Arc<Mutex<Option<crate::mcp::Pool>>>,
}

pub struct Setup {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub session: Option<String>,
    pub model: String,
    pub mode: Mode,
    pub effort: Option<String>,
    pub images: Vec<PathBuf>,
    pub compact: bool,
    pub goal: Option<String>,
    pub shared: Shared,
}

pub struct Turn {
    pub events: Receiver<Event>,
    answers: Sender<Answer>,
    steers: Sender<(String, Vec<PathBuf>)>,
    cancel: Arc<AtomicBool>,
    running: Arc<Mutex<Option<u32>>>,
    mode: Arc<Mutex<Mode>>,
}

impl Turn {
    pub fn shift(&self, mode: Mode) {
        if let Ok(mut current) = self.mode.lock() {
            *current = mode;
        }
    }

    pub fn answer(&self, answer: Answer) {
        let _ = self.answers.send(answer);
    }

    pub fn steer(&self, text: String, images: Vec<PathBuf>) {
        let _ = self.steers.send((text, images));
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let pid = self.running.lock().ok().and_then(|slot| *slot);
        if let Some(pid) = pid {
            process::kill(pid);
        }
    }
}

struct Worker {
    setup: Setup,
    events: Sender<Event>,
    answers: Receiver<Answer>,
    steers: Receiver<(String, Vec<PathBuf>)>,
    cancel: Arc<AtomicBool>,
    running: Arc<Mutex<Option<u32>>>,
    mode: Arc<Mutex<Mode>>,
}

pub fn start(setup: Setup, text: String) -> Turn {
    launch(setup, move |worker| worker.go(&text))
}

pub fn squash(setup: Setup) -> Turn {
    launch(setup, Worker::squash)
}

fn launch<F>(setup: Setup, job: F) -> Turn
where
    F: FnOnce(&Worker) -> Result<(), String> + Send + 'static,
{
    let (events, receiver) = mpsc::channel();
    let (answers, inbox) = mpsc::channel();
    let (steers, steered) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let running = Arc::new(Mutex::new(None));
    let mode = Arc::new(Mutex::new(setup.mode));
    let worker = Worker {
        answers: inbox,
        cancel: Arc::clone(&cancel),
        events: events.clone(),
        mode: Arc::clone(&mode),
        running: Arc::clone(&running),
        setup,
        steers: steered,
    };
    thread::spawn(move || {
        if let Err(message) = job(&worker) {
            let _ = events.send(Event::Failed(message));
        }
    });
    Turn {
        answers,
        cancel,
        events: receiver,
        mode,
        running,
        steers,
    }
}

impl Worker {
    fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn mode(&self) -> Mode {
        self.mode.lock().map_or(self.setup.mode, |mode| *mode)
    }

    fn stopped(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn steered(&self, file: &Path, messages: &mut Vec<Value>) -> Result<bool, String> {
        let mut any = false;
        while let Ok((text, images)) = self.steers.try_recv() {
            let mut user = json!({"role": "user", "content": text});
            if !images.is_empty() {
                user["images"] = json!(
                    images
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                );
            }
            append(file, None, &user)?;
            messages.push(user);
            any = true;
        }
        Ok(any)
    }

    fn go(&self, text: &str) -> Result<(), String> {
        let config = Config::load(&self.setup.root)?;
        let (name, model) = split(&self.setup.model);
        let endpoint = config
            .endpoint(name)
            .ok_or_else(|| format!("there's no provider called {name}. run /provider to add it"))?;
        let (id, file, mut messages) = open(&self.setup)?;
        if let Some(first) = messages
            .first_mut()
            .filter(|first| first["role"] == "system")
        {
            first["content"] = json!(prompt::system(&self.setup.cwd));
        }
        self.send(Event::Session(id));
        self.send(Event::Model(self.setup.model.clone()));
        if self.setup.compact && messages.len() > 2 {
            self.summarize(&endpoint, model, &file, &mut messages)?;
        }
        let mut user = json!({"role": "user", "content": text});
        if !self.setup.images.is_empty() {
            user["images"] = json!(
                self.setup
                    .images
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
            );
        }
        append(&file, None, &user)?;
        messages.push(user);
        let started = Instant::now();
        let mut cost: Option<f64> = None;
        let mut used: Option<u64> = None;
        let local = ["://localhost", "://127.0.0.1", "://0.0.0.0", "://[::1]"]
            .iter()
            .any(|host| endpoint.url.contains(host));
        let mut schema = tools::schema(model, self.setup.goal.is_some(), local);
        if !local && !config.mcp.is_empty() {
            self.connect(&config);
            if let (Ok(pool), Some(list)) = (self.setup.shared.pool.lock(), schema.as_array_mut()) {
                list.extend(
                    pool.as_ref()
                        .map(crate::mcp::Pool::schema)
                        .unwrap_or_default(),
                );
            }
        }
        for _ in 0..STEPS {
            if self.stopped() {
                return Ok(());
            }
            let mut reply = self.ask(&endpoint, model, &trim(&messages), &schema)?;
            let known: Vec<String> = schema
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
                .collect();
            crate::salvage::repair(&mut reply.message, &known);
            if self.stopped() {
                return Ok(());
            }
            cost = match (cost, reply.cost) {
                (Some(total), Some(more)) => Some(total + more),
                (total, more) => total.or(more),
            };
            used = reply.prompt.or(used);
            append(&file, Some(&self.setup.model), &reply.message)?;
            messages.push(reply.message.clone());
            let calls = reply.message["tool_calls"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if calls.is_empty() {
                if self.steered(&file, &mut messages)? {
                    continue;
                }
                if reply.message["content"]
                    .as_str()
                    .is_none_or(|body| body.trim().is_empty())
                {
                    self.send(Event::Note(
                        "the model stopped without a reply. say continue to keep it going"
                            .to_string(),
                    ));
                }
                self.finish(started, cost, used, &endpoint, model);
                return Ok(());
            }
            let mut refused = false;
            for call in &calls {
                let call = call.clone();
                let result = if refused {
                    REFUSED.to_string()
                } else {
                    match self.call(&call) {
                        Some(result) => result,
                        None => {
                            refused = true;
                            REFUSED.to_string()
                        }
                    }
                };
                if self.stopped() {
                    return Ok(());
                }
                let answer = json!({
                    "role": "tool",
                    "tool_call_id": call["id"],
                    "content": result,
                });
                append(&file, None, &answer)?;
                messages.push(answer);
            }
            if refused {
                self.finish(started, cost, used, &endpoint, model);
                return Ok(());
            }
            self.steered(&file, &mut messages)?;
        }
        self.send(Event::Note(format!(
            "sombre paused after {STEPS} steps in one turn so it can't loop forever. say continue to keep going"
        )));
        self.finish(started, cost, used, &endpoint, model);
        Ok(())
    }

    fn ask(
        &self,
        endpoint: &crate::config::Endpoint,
        model: &str,
        messages: &[Value],
        schema: &Value,
    ) -> Result<provider::Reply, String> {
        let bare = json!([]);
        let known = self
            .setup
            .shared
            .plain
            .lock()
            .is_ok_and(|plain| plain.contains(model));
        let tools = if known { &bare } else { schema };
        let delta = |text: &str| self.send(Event::Delta(text.to_string()));
        let mut tools = tools;
        let flat = self
            .setup
            .shared
            .flat
            .lock()
            .is_ok_and(|flat| flat.contains(model));
        let mut effort = self.setup.effort.as_deref().filter(|_| !flat);
        let mut modern = endpoint.responses
            && !self
                .setup
                .shared
                .classic
                .lock()
                .is_ok_and(|classic| classic.contains(model));
        let pictured = messages.iter().any(|message| message["images"].is_array());
        let mut sighted = !self
            .setup
            .shared
            .blind
            .lock()
            .is_ok_and(|blind| blind.contains(model));
        let mut attempt = 0;
        loop {
            let ask = provider::Ask {
                effort,
                images: sighted,
                messages,
                model,
                tools,
            };
            let reply = if modern {
                provider::stream(endpoint, &ask, &self.cancel, delta)
            } else {
                provider::complete(endpoint, &ask, &self.cancel, delta)
            };
            match reply {
                Ok(reply) => return Ok(reply),
                Err(failure) if modern && failure.status.is_some_and(unrouted) => {
                    if let Ok(mut classic) = self.setup.shared.classic.lock() {
                        classic.insert(model.to_string());
                    }
                    modern = false;
                }
                Err(failure) if sighted && pictured && unseen(&failure.message) => {
                    if let Ok(mut blind) = self.setup.shared.blind.lock() {
                        blind.insert(model.to_string());
                    }
                    self.send(Event::Note(format!(
                        "{model} can't see images, so sombre sent the text only"
                    )));
                    sighted = false;
                }
                Err(failure) if effort.is_some() && unreasoned(&failure.message) => {
                    if let Ok(mut flat) = self.setup.shared.flat.lock() {
                        flat.insert(model.to_string());
                    }
                    self.send(Event::Note(format!(
                        "{model} doesn't take an effort level, so sombre is sending without one"
                    )));
                    effort = None;
                }
                Err(failure) if !known && tools != &bare && toolless(&failure.message) => {
                    if let Ok(mut plain) = self.setup.shared.plain.lock() {
                        plain.insert(model.to_string());
                    }
                    self.send(Event::Note(format!(
                        "{model} can't use tools, so sombre is chatting with it without them"
                    )));
                    tools = &bare;
                }
                Err(failure) if failure.retry() && attempt < WAITS.len() && !self.stopped() => {
                    let seconds = failure.wait.unwrap_or(WAITS[attempt]).min(60);
                    attempt += 1;
                    let why = match failure.status {
                        None => failure.message.clone(),
                        Some(429) => format!("{} is busy", endpoint.name),
                        Some(_) => format!("{} had a problem on its side", endpoint.name),
                    };
                    self.send(Event::Note(format!(
                        "{why}. trying again in {seconds}s, esc to stop"
                    )));
                    let until = Instant::now() + std::time::Duration::from_secs(seconds);
                    while Instant::now() < until && !self.stopped() {
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(failure) => return Err(failure.message),
            }
        }
    }

    fn connect(&self, config: &Config) {
        let Ok(mut pool) = self.setup.shared.pool.lock() else {
            return;
        };
        if pool.is_some() {
            return;
        }
        let started = crate::mcp::Pool::start(&config.mcp, &self.setup.cwd);
        for problem in &started.problems {
            self.send(Event::Note(problem.clone()));
        }
        if !started.tools.is_empty() {
            let servers: HashSet<&str> = started
                .tools
                .iter()
                .map(|tool| tool.server.as_str())
                .collect();
            self.send(Event::Note(format!(
                "{} mcp tools from {}",
                started.tools.len(),
                servers.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }
        *pool = Some(started);
    }

    fn squash(&self) -> Result<(), String> {
        let config = Config::load(&self.setup.root)?;
        let (name, model) = split(&self.setup.model);
        let endpoint = config
            .endpoint(name)
            .ok_or_else(|| format!("there's no provider called {name}. run /provider to add it"))?;
        let (_, file, mut messages) = open(&self.setup)?;
        let started = Instant::now();
        if messages.len() > 2 {
            self.summarize(&endpoint, model, &file, &mut messages)?;
        } else {
            self.send(Event::Note("nothing to compact yet".to_string()));
        }
        self.send(Event::Done {
            cost: None,
            left: None,
            millis: u64::try_from(started.elapsed().as_millis()).ok(),
        });
        Ok(())
    }

    fn summarize(
        &self,
        endpoint: &crate::config::Endpoint,
        model: &str,
        file: &Path,
        messages: &mut Vec<Value>,
    ) -> Result<(), String> {
        let mut request = messages.clone();
        request.push(json!({"role": "user", "content": ASK}));
        let reply = provider::chat(endpoint, model, &request, &json!([]))?;
        let summary = reply.message["content"]
            .as_str()
            .filter(|summary| !summary.trim().is_empty())
            .ok_or_else(|| "the model sent back an empty summary".to_string())?
            .trim()
            .to_string();
        write(
            file,
            &Record::Compact {
                summary: summary.clone(),
            },
        )?;
        *messages = shrink(std::mem::take(messages), &summary);
        self.send(Event::Note(
            "compacted the chat. sombre keeps a summary of the earlier messages".to_string(),
        ));
        Ok(())
    }

    fn call(&self, call: &Value) -> Option<String> {
        let id = call["id"].as_str().unwrap_or_default().to_string();
        let name = call["function"]["name"].as_str().unwrap_or_default();
        let input: Value = match call["function"]["arguments"]
            .as_str()
            .map(serde_json::from_str::<Value>)
        {
            Some(Ok(input)) => input,
            _ => return Some("error: the tool arguments weren't valid json".to_string()),
        };
        let cwd = &self.setup.cwd;
        self.send(Event::Step {
            detail: store::describe(name, &input),
            id: id.clone(),
            label: store::label(name).to_string(),
        });
        if let Some(rule) = tools::rule(name, &input, cwd).filter(|rule| needed(self.mode(), rule))
        {
            let known = self
                .setup
                .shared
                .allowed
                .lock()
                .is_ok_and(|allowed| allowed.contains(&rule));
            if !known {
                self.send(Event::Approval {
                    always: Some(tools::always(name, &input, cwd)),
                    change: tools::change(name, &input, cwd),
                    detail: store::describe(name, &input),
                    id: id.clone(),
                    input: input.clone(),
                    reason: String::new(),
                    rules: json!(rule),
                    tool: tool(name).to_string(),
                });
                match self.answers.recv() {
                    Ok(Answer::Yes) => {}
                    Ok(Answer::Always) => {
                        if let Ok(mut allowed) = self.setup.shared.allowed.lock() {
                            allowed.insert(rule);
                        }
                    }
                    Ok(Answer::No) | Err(_) => return None,
                }
            }
        }
        if name.starts_with("mcp__") {
            let result = self
                .setup
                .shared
                .pool
                .lock()
                .map_err(|_| "the mcp servers are unavailable".to_string())
                .and_then(|mut pool| {
                    pool.as_mut()
                        .ok_or_else(|| "no mcp servers are running".to_string())
                        .and_then(|pool| pool.call(name, &input))
                });
            let (failed, text) = match result {
                Ok(text) => (false, text),
                Err(problem) => (true, problem),
            };
            if !text.trim().is_empty() || failed {
                self.send(Event::Output {
                    failed,
                    id: id.clone(),
                    text: text.clone(),
                });
            }
            return Some(if failed {
                format!("error: {text}")
            } else {
                text
            });
        }
        let target = tools::resolve(cwd, input["path"].as_str().unwrap_or_default());
        let known = self
            .setup
            .shared
            .seen
            .lock()
            .is_ok_and(|seen| seen.contains(&target));
        if name == "write" && target.is_file() && !known {
            return Some(format!(
                "error: {} already exists and you haven't read it in this chat. read it first, then use edit for changes or write to replace it",
                target.display()
            ));
        }
        if matches!(name, "read" | "write" | "edit")
            && let Ok(mut seen) = self.setup.shared.seen.lock()
        {
            seen.insert(target);
        }
        let done = tools::run(
            name,
            &id,
            &input,
            cwd,
            &self.cancel,
            &self.running,
            self.mode() != Mode::Yolo,
        );
        if let Some(event) = done.event {
            self.send(event);
        }
        Some(done.text)
    }

    fn finish(
        &self,
        started: Instant,
        cost: Option<f64>,
        used: Option<u64>,
        endpoint: &crate::config::Endpoint,
        model: &str,
    ) {
        let window = self
            .setup
            .shared
            .windows
            .lock()
            .ok()
            .and_then(|windows| windows.get(model).copied());
        let window = window.or_else(|| {
            let found = provider::window(endpoint, model).unwrap_or(0);
            if let Ok(mut windows) = self.setup.shared.windows.lock() {
                windows.insert(model.to_string(), found);
            }
            Some(found)
        });
        let left = match (used, window) {
            (Some(used), Some(window)) if window > 0 => {
                u8::try_from(100_u64.saturating_sub(used.saturating_mul(100) / window)).ok()
            }
            _ => None,
        };
        self.send(Event::Done {
            cost,
            left,
            millis: u64::try_from(started.elapsed().as_millis()).ok(),
        });
    }
}

pub fn toolless(problem: &str) -> bool {
    let problem = problem.to_lowercase();
    (problem.contains("tool") || problem.contains("function call"))
        && [
            "support",
            "not allowed",
            "invalid",
            "unknown",
            "unrecognized",
            "not available",
            "extra",
        ]
        .iter()
        .any(|word| problem.contains(word))
}

pub fn unrouted(status: u16) -> bool {
    matches!(status, 400 | 404 | 422)
}

pub fn unseen(problem: &str) -> bool {
    let problem = problem.to_lowercase();
    problem.contains("image") || problem.contains("vision") || problem.contains("multimodal")
}

pub fn unreasoned(problem: &str) -> bool {
    let problem = problem.to_lowercase();
    problem.contains("reasoning") || problem.contains("effort")
}

pub fn needed(mode: Mode, rule: &str) -> bool {
    match mode {
        Mode::Ask => true,
        Mode::Auto => rule != "edit",
        Mode::Yolo => false,
    }
}

fn tool(name: &str) -> &str {
    match name {
        "bash" | "shell" => "Bash",
        "write" => "Write",
        "edit" => "Edit",
        "read" => "Read",
        "fetch" => "WebFetch",
        other => other,
    }
}

pub fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_nanos())
        .unwrap_or_default()
}

fn fresh() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_nanos())
        .unwrap_or_default();
    let low = u32::try_from(nanos % (1 << 32)).unwrap_or_default();
    let whole = u64::try_from(nanos % (1 << 64)).unwrap_or_default();
    format!("{low:08x}{whole:016x}{:08x}", std::process::id())
}

fn open(setup: &Setup) -> Result<(String, PathBuf, Vec<Value>), String> {
    let folder = store::folder(&setup.root);
    fs::create_dir_all(&folder)
        .map_err(|error| format!("couldn't create {}: {error}", folder.display()))?;
    if let Some(id) = &setup.session {
        let file = folder.join(format!("{id}.jsonl"));
        let mut messages: Vec<Value> = Vec::new();
        for record in store::records(&file) {
            match record {
                Record::Message { message, .. } => messages.push(message),
                Record::Compact { summary } => messages = shrink(messages, &summary),
                Record::Meta { .. } | Record::Title { .. } => {}
            }
        }
        if messages.is_empty() {
            return Err(format!("couldn't find the sombre chat {id}"));
        }
        if messages
            .first()
            .is_none_or(|first| first["role"] != "system")
        {
            messages.insert(
                0,
                json!({"role": "system", "content": prompt::system(&setup.cwd)}),
            );
        }
        return Ok((id.clone(), file, mend(messages)));
    }
    let id = fresh();
    let file = folder.join(format!("{id}.jsonl"));
    let meta = Record::Meta {
        cwd: setup.cwd.clone(),
        id: id.clone(),
    };
    write(&file, &meta)?;
    let system = json!({"role": "system", "content": prompt::system(&setup.cwd)});
    append(&file, None, &system)?;
    Ok((id, file, vec![system]))
}

fn append(file: &Path, model: Option<&String>, message: &Value) -> Result<(), String> {
    write(
        file,
        &Record::Message {
            message: message.clone(),
            model: model.cloned(),
        },
    )
}

fn write(file: &Path, record: &Record) -> Result<(), String> {
    let line = serde_json::to_string(record).map_err(|error| error.to_string())?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut handle| writeln!(handle, "{line}"))
        .map_err(|error| format!("couldn't save the chat to {}: {error}", file.display()))
}

pub fn trim(messages: &[Value]) -> Vec<Value> {
    let tools = messages
        .iter()
        .filter(|message| message["role"] == "tool")
        .count();
    let mut seen = 0;
    messages
        .iter()
        .map(|message| {
            if message["role"] != "tool" {
                return message.clone();
            }
            seen += 1;
            let text = message["content"].as_str().unwrap_or_default();
            if tools - seen < 5 || text.chars().count() <= 300 {
                return message.clone();
            }
            let mut short = message.clone();
            let head: String = text.chars().take(200).collect();
            short["content"] = json!(format!(
                "{head}\n… older output trimmed. run it again if you need it"
            ));
            short
        })
        .collect()
}

pub fn shrink(messages: Vec<Value>, summary: &str) -> Vec<Value> {
    let mut kept: Vec<Value> = messages
        .into_iter()
        .filter(|message| message["role"] == "system")
        .take(1)
        .collect();
    kept.push(json!({"role": "user", "content": format!("{SUMMARY}{summary}")}));
    kept
}

pub fn mend(messages: Vec<Value>) -> Vec<Value> {
    let answered: HashSet<String> = messages
        .iter()
        .filter(|message| message["role"] == "tool")
        .filter_map(|message| message["tool_call_id"].as_str().map(str::to_string))
        .collect();
    let mut mended = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut asked: HashSet<String> = HashSet::new();
    let flush = |mended: &mut Vec<Value>, pending: &mut Vec<String>| {
        for id in pending.drain(..) {
            mended.push(json!({"role": "tool", "tool_call_id": id, "content": STOPPED}));
        }
    };
    for message in messages {
        if message["role"] != "tool" {
            flush(&mut mended, &mut pending);
        }
        if message["role"] == "tool"
            && message["tool_call_id"]
                .as_str()
                .is_none_or(|id| !asked.contains(id))
        {
            continue;
        }
        if message["role"] == "assistant" {
            asked.extend(
                message["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|call| call["id"].as_str().map(str::to_string)),
            );
            pending = message["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|call| call["id"].as_str())
                .filter(|id| !answered.contains(*id))
                .map(str::to_string)
                .collect();
        }
        mended.push(message);
    }
    flush(&mut mended, &mut pending);
    mended
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(ids: &[&str]) -> Value {
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": ids.iter().map(|id| json!({"id": id, "type": "function", "function": {"name": "bash", "arguments": "{}"}})).collect::<Vec<_>>()
        })
    }

    fn answer(id: &str, content: &str) -> Value {
        json!({"role": "tool", "tool_call_id": id, "content": content})
    }

    #[test]
    fn falls_back_to_chat_only_when_responses_refuses_the_request() {
        assert!(unrouted(400));
        assert!(unrouted(404));
        assert!(!unrouted(403));
        assert!(!unrouted(429));
        assert!(!unrouted(500));
    }

    #[test]
    fn trims_all_but_the_latest_tool_output() {
        let long = "x".repeat(500);
        let messages: Vec<Value> = (0..7)
            .map(|index| answer(&index.to_string(), &long))
            .collect();
        let trimmed = trim(&messages);
        let short = |message: &Value| {
            message["content"]
                .as_str()
                .unwrap_or_default()
                .contains("older output trimmed")
        };
        assert!(short(&trimmed[0]) && short(&trimmed[1]));
        assert!(trimmed[2..].iter().all(|message| !short(message)));
        assert_eq!(messages[0]["content"], long);
    }

    #[test]
    fn mend_drops_tool_outputs_whose_call_is_gone() {
        let user = json!({"role": "user", "content": "summary of earlier work"});
        let mended = mend(vec![
            user.clone(),
            answer("lost", "output from before the summary"),
            assistant(&["a"]),
            answer("a", "done"),
        ]);
        assert_eq!(mended, vec![user, assistant(&["a"]), answer("a", "done")]);
    }

    #[test]
    fn mend_answers_calls_left_hanging_by_a_stop() {
        let user = json!({"role": "user", "content": "next"});
        let mended = mend(vec![
            assistant(&["a", "b"]),
            answer("a", "done"),
            user.clone(),
        ]);
        assert_eq!(
            mended,
            vec![
                assistant(&["a", "b"]),
                answer("a", "done"),
                answer("b", STOPPED),
                user
            ]
        );
    }

    #[test]
    fn mend_handles_a_stop_at_the_very_end() {
        assert_eq!(
            mend(vec![assistant(&["a"])]),
            vec![assistant(&["a"]), answer("a", STOPPED)]
        );
    }

    #[test]
    fn mend_leaves_complete_history_alone() {
        let history = vec![assistant(&["a"]), answer("a", "ok")];
        assert_eq!(mend(history.clone()), history);
    }

    #[test]
    fn shrink_keeps_the_system_prompt_and_the_summary() {
        let system = json!({"role": "system", "content": "you are sombre"});
        let shrunk = shrink(
            vec![
                system.clone(),
                json!({"role": "user", "content": "old"}),
                assistant(&["a"]),
                answer("a", "done"),
            ],
            "the user wants tests",
        );
        assert_eq!(shrunk.len(), 2);
        assert_eq!(shrunk[0], system);
        assert!(
            shrunk[1]["content"]
                .as_str()
                .unwrap_or_default()
                .ends_with("the user wants tests")
        );
    }

    #[test]
    fn reopens_from_the_last_compaction() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let setup = Setup {
            compact: false,
            goal: None,
            cwd: dir.path().to_path_buf(),
            effort: None,
            images: Vec::new(),
            mode: Mode::Ask,
            model: "gateway:x".to_string(),
            root: dir.path().to_path_buf(),
            session: None,
            shared: Shared::default(),
        };
        let (id, file, _) = open(&setup).unwrap_or_else(|error| panic!("{error}"));
        append(&file, None, &json!({"role": "user", "content": "before"}))
            .unwrap_or_else(|error| panic!("{error}"));
        write(
            &file,
            &Record::Compact {
                summary: "short".to_string(),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        append(&file, None, &json!({"role": "user", "content": "after"}))
            .unwrap_or_else(|error| panic!("{error}"));
        let again = Setup {
            session: Some(id),
            ..setup
        };
        let (_, _, messages) = open(&again).unwrap_or_else(|error| panic!("{error}"));
        let contents: Vec<&str> = messages
            .iter()
            .filter_map(|message| message["content"].as_str())
            .collect();
        assert_eq!(contents.len(), 3);
        assert!(contents[1].ends_with("short"));
        assert_eq!(contents[2], "after");
    }

    #[test]
    fn spots_endpoints_without_tool_support() {
        assert!(toolless("local: this model does not support tools"));
        assert!(toolless(
            "abliteration: Unrecognized request argument supplied: tools"
        ));
        assert!(toolless(
            "x: function calling is not available for this model"
        ));
        assert!(!toolless(
            "gateway: Free tier users do not have access to this model."
        ));
        assert!(!toolless("couldn't reach local: connection refused"));
    }

    #[test]
    fn each_mode_asks_for_less() {
        assert!(needed(Mode::Ask, "edit"));
        assert!(needed(Mode::Ask, "bash:rm x"));
        assert!(!needed(Mode::Auto, "edit"));
        assert!(needed(Mode::Auto, "bash:rm x"));
        assert!(!needed(Mode::Yolo, "edit"));
        assert!(!needed(Mode::Yolo, "bash:rm x"));
    }

    #[test]
    fn a_running_turn_sees_the_mode_switch() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let setup = Setup {
            cwd: dir.path().to_path_buf(),
            compact: false,
            goal: None,
            effort: None,
            images: Vec::new(),
            mode: Mode::Ask,
            model: "gateway:x".to_string(),
            root: dir.path().to_path_buf(),
            session: None,
            shared: Shared::default(),
        };
        let turn = launch(setup, |worker| {
            worker.send(Event::Note(worker.mode().name().to_string()));
            let _ = worker.steers.recv();
            worker.send(Event::Note(worker.mode().name().to_string()));
            Ok(())
        });
        let first = turn.events.recv_timeout(std::time::Duration::from_secs(5));
        assert_eq!(first.ok(), Some(Event::Note("ask".to_string())));
        turn.shift(Mode::Yolo);
        turn.steer(String::new(), Vec::new());
        let second = turn.events.recv_timeout(std::time::Duration::from_secs(5));
        assert_eq!(second.ok(), Some(Event::Note("yolo".to_string())));
    }

    #[test]
    fn new_ids_are_unique_and_file_safe() {
        let first = fresh();
        let second = fresh();
        assert_ne!(first, second);
        assert_ne!(first[..8], second[..8]);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn reopens_a_saved_chat_with_its_history() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let setup = Setup {
            cwd: dir.path().to_path_buf(),
            compact: false,
            goal: None,
            effort: None,
            images: Vec::new(),
            mode: Mode::Ask,
            model: "gateway:x".to_string(),
            root: dir.path().to_path_buf(),
            session: None,
            shared: Shared::default(),
        };
        let (id, file, messages) = open(&setup).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(messages.len(), 1);
        append(&file, None, &json!({"role": "user", "content": "hi"}))
            .unwrap_or_else(|error| panic!("{error}"));
        let again = Setup {
            session: Some(id),
            ..setup
        };
        let (_, _, messages) = open(&again).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["content"], "hi");
    }
}

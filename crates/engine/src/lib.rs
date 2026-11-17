pub mod carry;
pub mod files;
pub mod git;
pub mod goal;
pub mod purge;
pub mod shell;

use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sombre_core::event::{self, Event};
use sombre_core::server::{self, Server};
use sombre_core::{Agent, Model, Roots};
use sombre_harness::config::{Config, LEVELS, Mode, level};
use sombre_harness::process;
use sombre_harness::{Answer, Setup, Shared, Turn};

pub enum Output {
    Line(String),
    Error(String),
}

pub enum Next {
    Events(Vec<Event>),
    Error(String),
    Idle,
    Closed,
}

pub struct Engine {
    pub agent: Agent,
    pub cwd: PathBuf,
    pub session: Option<String>,
    pub model: Option<String>,
    pub label: Option<String>,
    pub mode: Mode,
    pub effort: Option<String>,
    pub images: Vec<PathBuf>,
    pub left: Option<u8>,
    pub cost: Option<f64>,
    billed: f64,
    pub goal: Option<String>,
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: Option<Receiver<Output>>,
    turn: Option<Turn>,
    shared: Shared,
    published: bool,
    server: Option<Server>,
    told: Option<Mode>,
    woke: Arc<AtomicUsize>,
}

impl Engine {
    pub fn new(agent: Agent, cwd: PathBuf, session: Option<String>) -> Self {
        let config = Roots::detect().and_then(|roots| Config::load(&roots.sombre).ok());
        Self {
            agent,
            child: None,
            cwd,
            input: None,
            cost: None,
            billed: 0.0,
            goal: None,
            label: None,
            left: None,
            mode: config
                .as_ref()
                .and_then(|config| config.permissions)
                .unwrap_or_default(),
            effort: config.and_then(|config| config.effort),
            images: Vec::new(),
            model: None,
            output: None,
            session,
            published: true,
            server: None,
            shared: Shared::default(),
            told: None,
            turn: None,
            woke: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn bell(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.woke)
    }

    pub fn woke(&self) -> bool {
        self.woke.load(Ordering::SeqCst) > 0
    }

    pub fn name(&self) -> &'static str {
        match self.agent {
            Agent::Claude => "claude code",
            Agent::Codex => "codex",
            Agent::Sombre => "sombre",
        }
    }

    pub fn send(&mut self, text: &str) -> io::Result<()> {
        let images = std::mem::take(&mut self.images);
        let paths: Vec<String> = images
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        match self.agent {
            Agent::Claude => {
                if !self.alive() {
                    self.spawn(self.claude())?;
                    self.billed = self.carried().unwrap_or_default();
                }
                self.write(&said(text, &images))
            }
            Agent::Codex => {
                let mut overrides = json!({});
                if let Some(effort) = self.level() {
                    overrides["effort"] = json!(effort);
                }
                if self.server.is_some() && self.told != Some(self.mode) {
                    overrides["approvalPolicy"] = json!(policy(self.mode).0);
                    overrides["sandboxPolicy"] = walls(self.mode);
                    self.told = Some(self.mode);
                }
                self.serve(|server| server.prompt(text, &overrides, &paths))
            }
            Agent::Sombre => {
                self.stop();
                let mut setup = self.setup()?;
                setup.images = images;
                self.turn = Some(sombre_harness::start(setup, text.to_string()));
                Ok(())
            }
        }
    }

    pub fn reconnect(&mut self) {
        if let Ok(mut pool) = self.shared.pool.lock() {
            *pool = None;
        }
    }

    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        let session = self
            .session
            .clone()
            .ok_or("there's nothing to rename yet. send a message first")?;
        let roots = Roots::detect().ok_or("couldn't find your home folder")?;
        match self.agent {
            Agent::Sombre => {
                let path = sombre_core::sombre::find(&roots.sombre, &session)
                    .ok_or("couldn't find this chat's file")?;
                sombre_core::sombre::rename(&path, name)
                    .map_err(|error| format!("couldn't save the name: {error}"))
            }
            Agent::Claude => {
                self.stop();
                let output = Command::new(program("claude"))
                    .args([
                        "-p",
                        "--model",
                        "haiku",
                        "--output-format",
                        "json",
                        "--resume",
                    ])
                    .arg(&session)
                    .arg(format!("/rename {name}"))
                    .current_dir(&self.cwd)
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output()
                    .map_err(|error| format!("couldn't start claude code: {error}"))?;
                let reply: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
                let said = reply["result"].as_str().unwrap_or_default();
                if said.starts_with("Session renamed") {
                    Ok(())
                } else if said.is_empty() {
                    Err("claude code didn't answer the rename".to_string())
                } else {
                    Err(format!("claude code didn't rename it: {said}"))
                }
            }
            Agent::Codex => retitle(&session, name),
        }
    }

    pub fn steer(&mut self, text: &str, images: &[PathBuf]) -> io::Result<bool> {
        let alive = self.alive();
        let paths: Vec<String> = images
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        match self.agent {
            Agent::Claude if alive => {
                self.write(&said(text, images))?;
                Ok(true)
            }
            Agent::Codex if self.server.is_some() => {
                let sent = self
                    .server
                    .as_mut()
                    .is_some_and(|server| server.steer(text, &paths));
                self.flush()?;
                Ok(sent)
            }
            Agent::Sombre => Ok(self
                .turn
                .as_ref()
                .map(|turn| turn.steer(text.to_string(), images.to_vec()))
                .is_some()),
            _ => Ok(false),
        }
    }

    fn carried(&self) -> Option<f64> {
        let roots = Roots::detect()?;
        let path = sombre_core::claude::find(&roots.claude, self.session.as_deref()?)?;
        sombre_core::claude::spent(&path)
    }

    pub fn charge(&mut self, reported: Option<f64>) -> Option<f64> {
        let (spent, billed) = bill(self.agent, self.billed, reported);
        self.billed = billed;
        if let Some(spent) = spent {
            self.cost = Some(self.cost.unwrap_or_default() + spent);
        }
        spent
    }

    pub const fn native(&self) -> bool {
        matches!(self.agent, Agent::Claude | Agent::Codex)
    }

    pub fn aim(&mut self, objective: &str) -> io::Result<()> {
        self.serve(|server| server.aim(objective))
    }

    pub fn unaim(&mut self) -> io::Result<()> {
        self.serve(Server::clear)?;
        self.settle();
        Ok(())
    }

    pub fn levels(&self) -> &'static [&'static str] {
        match self.agent {
            Agent::Claude => &CLAUDE,
            Agent::Codex => &CODEX,
            Agent::Sombre => &LEVELS,
        }
    }

    pub fn level(&self) -> Option<&'static str> {
        self.effort
            .as_deref()
            .and_then(|effort| level(effort, self.levels()))
    }

    pub fn rounds(&self) -> usize {
        self.server.as_ref().map_or(1, |server| server.turns.max(1))
    }

    pub fn pause(&mut self) {
        if self.server.as_mut().is_some_and(Server::pause) {
            self.settle();
        }
    }

    fn settle(&mut self) {
        let until = Instant::now() + Duration::from_secs(5);
        while self.server.as_ref().is_some_and(Server::waiting) && self.flush().is_ok() {
            let (Some(output), Some(wait)) = (
                self.output.as_ref(),
                until.checked_duration_since(Instant::now()),
            ) else {
                break;
            };
            match output.recv_timeout(wait) {
                Ok(Output::Line(line)) => {
                    if let Some(server) = self.server.as_mut() {
                        server.read(&line);
                    }
                }
                Ok(Output::Error(_)) => {}
                Err(_) => break,
            }
        }
    }

    fn serve(&mut self, act: impl FnOnce(&mut Server)) -> io::Result<()> {
        if !self.alive() || self.server.is_none() {
            self.spawn(("codex", vec!["app-server".to_string()]))?;
            self.server = Some(Server::open(self.session.as_deref(), &self.settings()));
            self.told = Some(self.mode);
        }
        if let Some(server) = self.server.as_mut() {
            act(server);
        }
        self.flush()
    }

    pub fn compact(&mut self) -> io::Result<bool> {
        match self.agent {
            Agent::Claude => self.send("/compact").map(|()| true),
            Agent::Codex => self.serve(Server::compact).map(|()| true),
            Agent::Sombre => {
                self.stop();
                let mut setup = self.setup()?;
                setup.compact = true;
                self.turn = Some(sombre_harness::squash(setup));
                Ok(true)
            }
        }
    }

    fn setup(&self) -> io::Result<Setup> {
        let root = Roots::detect()
            .map(|roots| roots.sombre)
            .ok_or_else(|| io::Error::other("couldn't find your home folder"))?;
        let model = self.model.clone().unwrap_or_else(|| {
            Config::load(&root).map_or_else(
                |_| sombre_harness::config::DEFAULT.to_string(),
                |config| config.model(),
            )
        });
        Ok(Setup {
            compact: self.left.is_some_and(|left| left <= 15),
            goal: self.goal.clone(),
            cwd: self.cwd.clone(),
            effort: self.effort.clone(),
            images: Vec::new(),
            mode: self.mode,
            model,
            root,
            session: self.session.clone(),
            shared: self.shared.clone(),
        })
    }

    pub fn shift(&mut self, mode: Mode) {
        self.mode = mode;
        if let Some(turn) = &self.turn {
            turn.shift(mode);
        }
        if self.agent == Agent::Claude && self.alive() {
            let _ = self.write(&event::permission(claude(mode)));
        }
    }

    pub fn answer(&mut self, item: &Event, choice: Answer) -> io::Result<()> {
        let Event::Approval {
            id, input, rules, ..
        } = item
        else {
            return Ok(());
        };
        if self.server.is_some() {
            let decision = match choice {
                Answer::Yes => json!("accept"),
                Answer::Always => rules.clone(),
                Answer::No => json!("cancel"),
            };
            return self.write(&server::answer(input, &decision));
        }
        if let Some(turn) = &self.turn {
            turn.answer(choice);
            return Ok(());
        }
        let line = match choice {
            Answer::Yes => event::allow(id, input),
            Answer::Always => event::remember(id, input, rules),
            Answer::No => event::halt(id),
        };
        self.write(&line)
    }

    pub fn next(&mut self, wait: Duration) -> Next {
        if let Some(turn) = &self.turn {
            return match turn.events.recv_timeout(wait) {
                Ok(item) => Next::Events(vec![item]),
                Err(RecvTimeoutError::Timeout) => Next::Idle,
                Err(RecvTimeoutError::Disconnected) => Next::Closed,
            };
        }
        let Some(output) = &self.output else {
            return Next::Closed;
        };
        match output.recv_timeout(wait) {
            Ok(Output::Line(line)) => {
                if self.agent == Agent::Claude
                    && event::begins(&line)
                    && self.woke.load(Ordering::SeqCst) > 0
                {
                    self.woke.fetch_sub(1, Ordering::SeqCst);
                }
                let events = match (self.agent, self.server.as_mut()) {
                    (_, Some(server)) => server.read(&line),
                    (Agent::Claude, None) => event::claude(&line),
                    (Agent::Codex | Agent::Sombre, None) => Vec::new(),
                };
                if self.flush().is_err() {
                    return Next::Closed;
                }
                Next::Events(events)
            }
            Ok(Output::Error(line)) => Next::Error(line),
            Err(RecvTimeoutError::Timeout) => Next::Idle,
            Err(RecvTimeoutError::Disconnected) => Next::Closed,
        }
    }

    pub fn stop(&mut self) {
        if let Some(turn) = self.turn.take() {
            turn.stop();
        }
        self.pause();
        if let Some(mut child) = self.child.take() {
            self.published = false;
            process::interrupt(child.id());
            for _ in 0..40 {
                if !matches!(child.try_wait(), Ok(None)) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            let _ = child.kill();
            let _ = child.wait();
            process::untrack(child.id());
        }
        self.input = None;
        self.output = None;
        self.server = None;
        self.publish();
    }

    fn settings(&self) -> Value {
        let (approval, sandbox) = policy(self.mode);
        json!({
            "approvalPolicy": approval,
            "cwd": self.cwd,
            "model": self.model,
            "sandbox": sandbox
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        let lines = self
            .server
            .as_mut()
            .map(|server| std::mem::take(&mut server.outbox))
            .unwrap_or_default();
        for line in lines {
            self.write(&line)?;
        }
        Ok(())
    }

    fn publish(&mut self) {
        if self.published {
            return;
        }
        self.published = true;
        let (Some(session), Some(roots)) = (&self.session, Roots::detect()) else {
            return;
        };
        if !Config::load(&roots.sombre).is_ok_and(|config| config.publish) {
            return;
        }
        match self.agent {
            Agent::Claude => {
                if let Some(path) = sombre_core::claude::find(&roots.claude, session) {
                    let _ = sombre_core::claude::publish(&path);
                }
            }
            Agent::Codex => {
                if let Some(path) = sombre_core::codex::find(&roots.codex, session) {
                    let _ = sombre_core::codex::publish(&path);
                }
            }
            Agent::Sombre => {}
        }
    }

    fn alive(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    fn claude(&self) -> (&'static str, Vec<String>) {
        let mut args: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--replay-user-messages",
            "--permission-mode",
            claude(self.mode),
            "--allow-dangerously-skip-permissions",
            "--permission-prompts",
            "host",
            "--permission-prompt-tool",
            "stdio",
        ]
        .map(String::from)
        .to_vec();
        if let Some(model) = &self.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        if let Some(effort) = self.level() {
            args.push("--effort".to_string());
            args.push(effort.to_string());
        }
        if let Some(session) = &self.session {
            args.push("--resume".to_string());
            args.push(session.clone());
        }
        ("claude", args)
    }

    fn spawn(&mut self, (program, args): (&'static str, Vec<String>)) -> io::Result<()> {
        self.stop();
        self.billed = 0.0;
        let mut command = Command::new(self::program(program));
        let mut child = process::group(&mut command)
            .args(&args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        process::track(child.id());
        self.woke.store(0, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            let sender = sender.clone();
            let woke = (program == "claude").then(|| Arc::clone(&self.woke));
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(woke) = &woke
                        && event::begins(&line)
                    {
                        woke.fetch_add(1, Ordering::SeqCst);
                    }
                    if sender.send(Output::Line(line)).is_err() {
                        break;
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if sender.send(Output::Error(line)).is_err() {
                        break;
                    }
                }
            });
        }
        self.input = child.stdin.take();
        self.output = Some(receiver);
        self.child = Some(child);
        Ok(())
    }

    fn write(&mut self, line: &str) -> io::Result<()> {
        let Some(input) = self.input.as_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the agent isn't running",
            ));
        };
        writeln!(input, "{line}")?;
        input.flush()
    }
}

const CLAUDE: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const CODEX: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];

fn said(text: &str, images: &[PathBuf]) -> String {
    let mut blocks = vec![json!({"type": "text", "text": text})];
    blocks.extend(images.iter().filter_map(|path| {
        sombre_harness::image::load(path).ok().map(|(media, data)| {
            json!({"type": "image", "source": {"type": "base64", "media_type": media, "data": data}})
        })
    }));
    if blocks.len() > 1 {
        json!({"type": "user", "message": {"role": "user", "content": blocks}}).to_string()
    } else {
        event::prompt(text)
    }
}

fn bill(agent: Agent, billed: f64, reported: Option<f64>) -> (Option<f64>, f64) {
    match (agent, reported) {
        (Agent::Claude, Some(total)) => (Some((total - billed).max(0.0)), total),
        (_, spent) => (spent, billed),
    }
}

fn retitle(session: &str, name: &str) -> Result<(), String> {
    let mut child = Command::new(program("codex"))
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("couldn't start codex: {error}"))?;
    let _tracked = process::Tracked::new(child.id());
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
    }
    let requests = [
        json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "sombre", "version": env!("CARGO_PKG_VERSION")}}}),
        json!({"method": "initialized"}),
        json!({"id": 2, "method": "thread/name/set", "params": {"name": name, "threadId": session}}),
    ];
    let sent = child.stdin.as_mut().is_some_and(|input| {
        requests
            .iter()
            .all(|request| writeln!(input, "{request}").is_ok())
            && input.flush().is_ok()
    });
    let until = Instant::now() + Duration::from_secs(15);
    let mut outcome = Err("codex didn't answer the rename".to_string());
    while sent && let Some(wait) = until.checked_duration_since(Instant::now()) {
        let Ok(line) = receiver.recv_timeout(wait) else {
            break;
        };
        let Ok(reply) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if reply["id"] == 2 {
            outcome = match reply["error"]["message"].as_str() {
                Some(problem) => Err(format!("codex didn't rename it: {problem}")),
                None => Ok(()),
            };
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    outcome
}

fn program(name: &str) -> OsString {
    process::find(name).map_or_else(|| name.into(), PathBuf::into_os_string)
}

pub fn policy(mode: Mode) -> (&'static str, &'static str) {
    match mode {
        Mode::Ask => ("untrusted", "workspace-write"),
        Mode::Auto => ("on-request", "workspace-write"),
        Mode::Yolo => ("never", "danger-full-access"),
    }
}

fn walls(mode: Mode) -> Value {
    match mode {
        Mode::Yolo => json!({"type": "dangerFullAccess"}),
        Mode::Ask | Mode::Auto => json!({"type": "workspaceWrite"}),
    }
}

pub fn covers(mode: Mode, item: &Event) -> bool {
    let Event::Approval { tool, .. } = item else {
        return false;
    };
    match mode {
        Mode::Ask => false,
        Mode::Auto => matches!(
            tool.as_str(),
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit"
        ),
        Mode::Yolo => true,
    }
}

pub fn claude(mode: Mode) -> &'static str {
    match mode {
        Mode::Ask => "manual",
        Mode::Auto => "acceptEdits",
        Mode::Yolo => "bypassPermissions",
    }
}

pub fn about(agent: Agent, mode: Mode) -> &'static str {
    match (agent, mode) {
        (Agent::Codex, Mode::Auto) => {
            "work inside this folder's sandbox, ask before going outside it"
        }
        (_, mode) => mode.about(),
    }
}

pub fn recorded(history: &[Event]) -> Option<String> {
    history.iter().rev().find_map(|item| match item {
        Event::Model(name) => Some(name.clone()),
        _ => None,
    })
}

pub fn kept(agent: Agent, model: &str, roots: Option<&Roots>) -> Option<String> {
    match agent {
        Agent::Sombre => roots
            .and_then(|roots| Config::load(&roots.sombre).ok())
            .filter(|config| {
                config
                    .endpoint(sombre_harness::config::split(model).0)
                    .is_some()
            })
            .map(|_| model.to_string()),
        Agent::Claude => sombre_core::claude::models()
            .into_iter()
            .find(|family| model.contains(&family.id))
            .map(|family| family.id),
        Agent::Codex => roots
            .map(|roots| roots.models(Agent::Codex))
            .unwrap_or_default()
            .into_iter()
            .find(|listed| listed.id == model)
            .map(|listed| listed.id),
    }
}

pub fn fallback(roots: Option<&Roots>, agent: Agent) -> Option<String> {
    let roots = roots?;
    match agent {
        Agent::Sombre => Config::load(&roots.sombre)
            .ok()
            .map(|config| config.model()),
        _ => roots.model(agent),
    }
}

pub fn catalog(roots: &Roots, agent: Agent) -> Result<(Vec<Model>, Vec<String>), String> {
    if agent != Agent::Sombre {
        return Ok((roots.models(agent), Vec::new()));
    }
    let config = Config::load(&roots.sombre)?;
    let (listed, problems) = sombre_harness::catalog(&config);
    Ok((
        listed
            .into_iter()
            .map(|model| Model {
                about: model.name,
                id: model.id,
            })
            .collect(),
        problems,
    ))
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_turns_cost_the_rise_in_its_running_total() {
        let (first, billed) = bill(Agent::Claude, 0.0, Some(0.004_41));
        assert_eq!(first, Some(0.004_41));
        let (second, billed) = bill(Agent::Claude, billed, Some(0.006_93));
        assert!(second.is_some_and(|spent| (spent - 0.002_52).abs() < 1e-9));
        assert_eq!(bill(Agent::Claude, billed, None), (None, billed));
        assert_eq!(bill(Agent::Sombre, 0.5, Some(0.1)), (Some(0.1), 0.5));
        assert_eq!(bill(Agent::Codex, 0.0, None), (None, 0.0));
    }

    #[test]
    fn keeps_each_chat_on_its_own_model_family() {
        assert_eq!(
            kept(Agent::Claude, "claude-opus-5-5", None),
            Some("opus".to_string())
        );
        assert_eq!(
            kept(Agent::Claude, "claude-haiku-5-5", None),
            Some("haiku".to_string())
        );
        assert_eq!(kept(Agent::Claude, "mystery-model", None), None);
        assert_eq!(kept(Agent::Codex, "gpt-6.1-sol", None), None);
        assert_eq!(kept(Agent::Sombre, "gateway:x", None), None);
    }

    #[test]
    fn maps_each_mode_to_claude_code() {
        assert_eq!(claude(Mode::Ask), "manual");
        assert_eq!(claude(Mode::Auto), "acceptEdits");
        assert_eq!(claude(Mode::Yolo), "bypassPermissions");
    }

    fn asking(tool: &str) -> Event {
        Event::Approval {
            always: None,
            change: Vec::new(),
            detail: String::new(),
            id: "r1".to_string(),
            input: Value::Null,
            reason: String::new(),
            rules: Value::Null,
            tool: tool.to_string(),
        }
    }

    #[test]
    fn a_new_mode_answers_the_approvals_it_would_not_have_asked() {
        assert!(!covers(Mode::Ask, &asking("Edit")));
        assert!(covers(Mode::Auto, &asking("Edit")));
        assert!(covers(Mode::Auto, &asking("Write")));
        assert!(!covers(Mode::Auto, &asking("Bash")));
        assert!(covers(Mode::Yolo, &asking("Bash")));
        assert!(!covers(Mode::Yolo, &Event::Echo));
    }

    #[test]
    fn maps_each_mode_to_codex_approvals() {
        assert_eq!(walls(Mode::Ask)["type"], "workspaceWrite");
        assert_eq!(walls(Mode::Auto)["type"], "workspaceWrite");
        assert_eq!(walls(Mode::Yolo)["type"], "dangerFullAccess");
        assert_eq!(policy(Mode::Ask), ("untrusted", "workspace-write"));
        assert_eq!(policy(Mode::Auto), ("on-request", "workspace-write"));
        assert_eq!(policy(Mode::Yolo), ("never", "danger-full-access"));
    }
}

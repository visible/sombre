use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sombre_core::event::Question;
use sombre_core::{Agent, Event, Roots};
use sombre_engine::goal::{self, Goal, Outcome, Status};
use sombre_engine::{Engine, Next, covers, git, shell};
use sombre_harness::Answer;
use sombre_harness::config::{Config, Mode};
use tauri::ipc::Channel;

use crate::feed::{Task, relative};

const IDLE: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Aim {
    pub objective: String,
    pub status: String,
    pub active: bool,
    pub summary: String,
}

impl From<&Goal> for Aim {
    fn from(current: &Goal) -> Self {
        Self {
            active: current.status == Status::Active,
            objective: current.objective.clone(),
            status: current.word().to_string(),
            summary: current.summary(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Changed {
    pub path: String,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Update {
    State {
        busy: bool,
        model: Option<String>,
        mode: Mode,
        session: Option<String>,
        effort: Option<String>,
        levels: Vec<String>,
        cost: Option<f64>,
        left: Option<u8>,
        folder: String,
        goal: Option<Aim>,
    },
    Delta {
        text: String,
    },
    Text {
        text: String,
    },
    Step {
        step: String,
        label: String,
        detail: String,
    },
    Output {
        step: String,
        text: String,
        failed: bool,
    },
    Patch {
        step: String,
        lines: Vec<String>,
    },
    Plan {
        tasks: Vec<Task>,
    },
    Ask {
        id: String,
        tool: String,
        detail: String,
        reason: String,
        change: Vec<String>,
        always: Option<String>,
    },
    Questions {
        id: String,
        questions: Vec<Question>,
    },
    Granted {
        id: String,
    },
    Phone {
        text: String,
    },
    Decided {
        id: String,
        yes: bool,
    },
    Replied {
        id: String,
        answers: Option<Vec<String>>,
    },
    Note {
        text: String,
    },
    Changes {
        from: String,
        to: String,
        files: Vec<Changed>,
    },
    Failed {
        text: String,
    },
    Done {
        millis: Option<u64>,
        cost: Option<f64>,
        left: Option<u8>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Yes,
    Always,
    No,
}

impl From<Choice> for Answer {
    fn from(choice: Choice) -> Self {
        match choice {
            Choice::Yes => Self::Yes,
            Choice::Always => Self::Always,
            Choice::No => Self::No,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Pass {
    Skip,
    Show(Update),
    Session(String),
    Model(String),
    Ask(Update),
    Done {
        millis: Option<u64>,
        cost: Option<f64>,
        left: Option<u8>,
        last: bool,
    },
    Failed(String),
    Halted,
    Goal(String, String),
}

pub struct Gate {
    agent: Agent,
    cwd: PathBuf,
    ours: bool,
    halted: bool,
    sent: usize,
    echoed: usize,
    outcome: Outcome,
}

impl Gate {
    pub fn new(agent: Agent, cwd: &Path) -> Self {
        Self {
            agent,
            cwd: cwd.to_path_buf(),
            echoed: 0,
            halted: false,
            ours: agent != Agent::Claude,
            outcome: Outcome::default(),
            sent: 1,
        }
    }

    pub fn begin(&mut self) {
        self.ours = self.agent != Agent::Claude;
        self.halted = false;
        self.sent = 1;
        self.echoed = 0;
        self.outcome = Outcome::default();
    }

    pub fn own(&mut self) {
        self.begin();
        self.ours = true;
        self.sent = 0;
    }

    pub fn finish(&mut self) -> Outcome {
        std::mem::take(&mut self.outcome)
    }

    pub const fn steered(&mut self) {
        self.sent += 1;
    }

    pub const fn refuse(&mut self) {
        self.halted = true;
        self.outcome.refused = true;
    }

    pub const fn stopped(&mut self) {
        self.outcome.stopped = true;
    }

    pub fn pass(&mut self, event: Event) -> Pass {
        let pass = self.route(event);
        match &pass {
            Pass::Show(Update::Delta { text }) => self.outcome.said.push_str(text),
            Pass::Show(Update::Text { text }) if self.outcome.said.is_empty() => {
                self.outcome.said.clone_from(text);
            }
            Pass::Show(Update::Step { .. }) => {
                self.outcome.steps += 1;
                self.outcome.said.clear();
            }
            Pass::Goal(status, reason) => {
                self.outcome.goal = Some((status.clone(), reason.clone()));
            }
            Pass::Failed(_) => self.outcome.failed = true,
            _ => {}
        }
        pass
    }

    fn route(&mut self, event: Event) -> Pass {
        match event {
            Event::Session(id) => Pass::Session(id),
            Event::Model(name) => Pass::Model(name),
            Event::Echo => {
                self.ours = true;
                self.echoed += 1;
                Pass::Skip
            }
            Event::Note(text) => Pass::Show(Update::Note { text }),
            Event::Prompt(_) => Pass::Skip,
            Event::Goal { status, reason } => Pass::Goal(status, reason),
            Event::Text(_)
            | Event::Delta(_)
            | Event::Step { .. }
            | Event::Output { .. }
            | Event::Patch { .. }
            | Event::Plan { .. }
            | Event::Done { .. }
            | Event::Failed(_)
                if !self.ours =>
            {
                Pass::Skip
            }
            Event::Output { .. } | Event::Patch { .. } if self.halted => Pass::Skip,
            Event::Done { .. } | Event::Failed(_) if self.halted => Pass::Halted,
            Event::Delta(text) => Pass::Show(Update::Delta { text }),
            Event::Text(text) => Pass::Show(Update::Text { text }),
            Event::Step { label, .. } if label == "Plan" => Pass::Skip,
            Event::Step { id, label, detail } => Pass::Show(Update::Step {
                detail: relative(&detail, &self.cwd),
                label,
                step: id,
            }),
            Event::Image(path) => Pass::Show(Update::Step {
                detail: Path::new(&path)
                    .file_name()
                    .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned()),
                label: "View".to_string(),
                step: path,
            }),
            Event::Output { id, text, failed } => Pass::Show(Update::Output {
                failed,
                step: id,
                text,
            }),
            Event::Patch { id, lines } => Pass::Show(Update::Patch { lines, step: id }),
            Event::Plan { steps, .. } => Pass::Show(Update::Plan {
                tasks: steps
                    .into_iter()
                    .map(|(text, status)| Task { status, text })
                    .collect(),
            }),
            Event::Approval {
                id,
                tool,
                detail,
                reason,
                change,
                always,
                ..
            } => Pass::Ask(Update::Ask {
                always,
                change,
                detail: relative(&detail, &self.cwd),
                id,
                reason,
                tool,
            }),
            Event::Questions { id, questions, .. } => {
                Pass::Ask(Update::Questions { id, questions })
            }
            Event::Done { millis, cost, left } => Pass::Done {
                cost,
                last: self.agent != Agent::Claude || self.echoed >= self.sent,
                left,
                millis,
            },
            Event::Failed(text) => Pass::Failed(text),
        }
    }
}

pub struct Tuning {
    pub model: Option<String>,
    pub mode: Option<Mode>,
    pub effort: Option<String>,
}

pub enum Ask {
    Attach(Channel<Update>),
    Send(String, Vec<PathBuf>),
    Goal(String),
    Rename(String, Sender<Result<String, String>>),
    Compact,
    Steer(String, Sender<bool>),
    Answer(String, Answer),
    Reply(String, Option<Vec<String>>),
    Tune(Tuning, Sender<Result<String, String>>),
    Stop,
    Share(bool),
    Close,
}

struct Runner {
    key: String,
    held: VecDeque<(String, Vec<PathBuf>)>,
    engine: Engine,
    out: Channel<Update>,
    gate: Gate,
    waiting: HashMap<String, Event>,
    errors: Vec<String>,
    started: Instant,
    busy: bool,
    clearing: bool,
    goal: Option<Goal>,
    halt: Arc<AtomicBool>,
    before: Option<String>,
}

enum Check {
    Passed,
    Failed(String, String),
    Stopped,
}

impl Runner {
    fn emit(&self, update: Update) {
        let _ = self.out.send(update);
    }

    fn state(&self) {
        self.emit(Update::State {
            busy: self.busy,
            mode: self.engine.mode,
            model: self
                .engine
                .label
                .clone()
                .or_else(|| self.engine.model.clone()),
            session: self.engine.session.clone(),
            effort: self.engine.level().map(str::to_string),
            levels: self
                .engine
                .levels()
                .iter()
                .map(ToString::to_string)
                .collect(),
            cost: self.engine.cost,
            left: self.engine.left,
            folder: self.engine.cwd.display().to_string(),
            goal: self.goal.as_ref().map(Aim::from),
        });
    }

    fn note(&self, text: impl Into<String>) {
        self.emit(Update::Note { text: text.into() });
    }

    fn ended(&mut self) {
        self.compare();
        if std::mem::take(&mut self.clearing) {
            self.settle();
            self.unaim();
            return;
        }
        if !self.advance() {
            self.settle();
        }
    }

    fn compare(&mut self) {
        let Some(from) = self.before.take() else {
            return;
        };
        let Some(to) = git::snapshot(&self.engine.cwd).filter(|to| *to != from) else {
            return;
        };
        let Ok(found) = git::counts(&self.engine.cwd, &from, &to) else {
            return;
        };
        if found.is_empty() {
            return;
        }
        self.emit(Update::Changes {
            files: found
                .into_iter()
                .map(|count| Changed {
                    added: count.added,
                    path: count.path,
                    removed: count.removed,
                })
                .collect(),
            from,
            to,
        });
    }

    fn settle(&mut self) {
        self.busy = false;
        self.waiting.clear();
        self.state();
    }

    fn handle(&mut self, ask: Ask) -> bool {
        match ask {
            Ask::Attach(channel) => {
                self.out = channel;
                self.state();
            }
            Ask::Send(text, images) if !self.busy => {
                if self.followed() {
                    sombre_pair::said(&text, &images);
                }
                self.engine.images = images;
                self.send(&text);
            }
            Ask::Send(..) => {}
            Ask::Goal(text) => self.aim(&text),
            Ask::Rename(name, reply) => {
                let renamed = if self.busy {
                    Err("wait for this turn to finish first".to_string())
                } else {
                    self.engine
                        .rename(&name)
                        .map(|()| format!("renamed this chat to {name}"))
                };
                let _ = reply.send(renamed);
            }
            Ask::Compact if !self.busy => self.compact(),
            Ask::Compact => {}
            Ask::Steer(text, reply) => {
                let sent = self.busy && self.engine.steer(&text, &[]).unwrap_or(false);
                if sent {
                    self.gate.steered();
                }
                let _ = reply.send(sent);
            }
            Ask::Answer(id, choice) => self.answer(&id, choice),
            Ask::Reply(id, answers) => self.reply(&id, answers),
            Ask::Tune(tuning, reply) => {
                let _ = reply.send(self.tune(tuning));
            }
            Ask::Stop if self.busy => {
                self.engine.stop();
                if !self
                    .goal
                    .as_ref()
                    .is_some_and(|current| current.status == Status::Active)
                {
                    self.note("stopped");
                }
                self.gate.stopped();
                self.ended();
            }
            Ask::Stop => {}
            Ask::Share(fresh) => self.share(fresh),
            Ask::Close => return false,
        }
        true
    }

    fn followed(&self) -> bool {
        sombre_pair::paired() && sombre_pair::followed().as_deref() == Some(self.key.as_str())
    }

    fn share(&self, fresh: bool) {
        let mut items = vec![json!({
            "agent": self.engine.name(),
            "folder": self.engine.cwd.display().to_string(),
            "fresh": fresh,
            "kind": "hello",
            "model": self.engine.label,
            "name": self.engine.cwd.file_name().map(|name| name.to_string_lossy().into_owned()),
        })];
        let history = Roots::detect()
            .zip(self.engine.session.as_deref())
            .and_then(|(roots, session)| {
                let sessions = sombre_core::scan(&roots);
                match sombre_core::find(&sessions, session) {
                    sombre_core::Lookup::Found(found) => Some(sombre_core::transcript(found)),
                    _ => None,
                }
            })
            .unwrap_or_default();
        items.extend(replayed(history));
        for item in items {
            sombre_pair::post(&item);
        }
    }

    fn phone(&mut self) {
        while let Some(arrival) = sombre_pair::message() {
            self.emit(Update::Phone {
                text: arrival.text.clone(),
            });
            if !self.busy {
                sombre_pair::said(&arrival.text, &arrival.images);
                self.engine.images = arrival.images;
                self.send(&arrival.text);
            } else if arrival.now
                && self
                    .engine
                    .steer(&arrival.text, &arrival.images)
                    .unwrap_or(false)
            {
                sombre_pair::said(&arrival.text, &arrival.images);
                self.gate.steered();
            } else {
                self.held.push_back((arrival.text, arrival.images));
            }
        }
        for (id, yes) in sombre_pair::answers() {
            if self.waiting.contains_key(&id) {
                self.answer(&id, if yes { Answer::Yes } else { Answer::No });
                self.emit(Update::Decided { id, yes });
            }
        }
        for (id, answers) in sombre_pair::replies() {
            if self.waiting.contains_key(&id) {
                self.reply(&id, answers.clone());
                self.emit(Update::Replied { id, answers });
            }
        }
    }

    fn compact(&mut self) {
        self.gate.begin();
        self.errors.clear();
        self.waiting.clear();
        self.started = Instant::now();
        match self.engine.compact() {
            Ok(_) => {
                self.busy = true;
                self.emit(Update::Note {
                    text: "compacting this chat so it has room to keep going".to_string(),
                });
                self.state();
            }
            Err(error) => self.emit(Update::Failed {
                text: format!("couldn't compact: {error}"),
            }),
        }
    }

    fn send(&mut self, text: &str) {
        self.start(|engine| engine.send(text));
        if self.engine.agent == Agent::Claude && text.trim_start().starts_with('/') {
            self.gate.own();
        }
    }

    fn wake(&mut self) {
        self.start(|_| Ok(()));
        self.gate.own();
    }

    fn start(&mut self, begin: impl FnOnce(&mut Engine) -> io::Result<()>) {
        self.before = git::snapshot(&self.engine.cwd);
        self.gate.begin();
        self.errors.clear();
        self.waiting.clear();
        self.started = Instant::now();
        match begin(&mut self.engine) {
            Ok(()) => {
                self.busy = true;
                self.state();
            }
            Err(error) => {
                self.emit(Update::Failed {
                    text: format!(
                        "couldn't start {}: {error}. is it installed and on your PATH?",
                        self.engine.name()
                    ),
                });
                self.gate.outcome.failed = true;
                self.ended();
            }
        }
    }

    fn aim(&mut self, text: &str) {
        match goal::parse(text) {
            goal::Command::Show => self.note(self.goal.as_ref().map_or_else(
                || "no goal yet. /goal <what done looks like> starts one, like /goal all tests pass".to_string(),
                |current| format!("goal: {}\nstatus: {}", current.objective, current.summary()),
            )),
            goal::Command::Pause => match self.goal.as_mut() {
                Some(current) if current.status == Status::Active => {
                    current.status = Status::Paused;
                    let said = if self.busy {
                        "goal paused. it stops after this turn. /goal resume picks it up".to_string()
                    } else {
                        current.summary()
                    };
                    self.note(said);
                    self.state();
                }
                Some(_) => self.note("the goal isn't running. /goal resume picks it up"),
                None => self.note("there's no goal to pause"),
            },
            goal::Command::Clear => self.clear(),
            goal::Command::Resume | goal::Command::Set(_) if self.busy => {
                self.note("wait for this turn to finish, or stop it first");
            }
            goal::Command::Resume => match self.goal.as_mut() {
                Some(current) if current.status != Status::Complete => {
                    current.status = Status::Active;
                    current.idle = 0;
                    if current.turns >= goal::TURNS {
                        current.turns = 0;
                    }
                    self.pursue(false);
                }
                Some(_) => self.note(
                    "that goal is already achieved. /goal <what done looks like> sets a new one",
                ),
                None => self.note("there's no goal to resume. /goal <what done looks like> starts one"),
            },
            goal::Command::Set(objective) => {
                self.note(format!("goal: {objective}"));
                self.goal = Some(Goal::new(objective));
                self.pursue(true);
            }
        }
    }

    fn clear(&mut self) {
        if self.goal.is_none() {
            self.note("there's no goal to clear");
            return;
        }
        self.goal = None;
        self.engine.goal = None;
        if self.busy && self.engine.native() {
            self.clearing = true;
            self.note("the goal clears once this turn ends");
        } else {
            if self.engine.agent != Agent::Claude {
                self.note("goal cleared");
            }
            self.unaim();
        }
        self.state();
    }

    fn unaim(&mut self) {
        if self.engine.agent == Agent::Claude {
            self.send("/goal clear");
        } else if self.engine.agent == Agent::Codex
            && let Err(error) = self.engine.unaim()
        {
            self.emit(Update::Failed {
                text: format!("couldn't clear codex's goal: {error}"),
            });
        }
        self.state();
    }

    fn pursue(&mut self, first: bool) {
        let Some(objective) = self.goal.as_ref().map(|current| current.objective.clone()) else {
            return;
        };
        if self.engine.agent == Agent::Claude {
            self.send(&format!("/goal {objective}"));
        } else if self.engine.agent == Agent::Codex {
            self.start(|engine| engine.aim(&objective));
        } else {
            self.engine.goal = Some(objective.clone());
            self.send(&if first {
                goal::opening(&objective)
            } else {
                goal::onward(&objective)
            });
        }
    }

    fn verify(&self, objective: &str, round: usize) -> Check {
        for (index, command) in goal::checks(objective).into_iter().enumerate() {
            let step = format!("goal:{round}:{index}");
            self.emit(Update::Step {
                detail: command.clone(),
                label: "Test".to_string(),
                step: step.clone(),
            });
            match shell::run(
                &command,
                &self.engine.cwd,
                &self.halt,
                self.engine.mode != Mode::Yolo,
                |_| {},
            ) {
                Ok(ran) if ran.stopped => return Check::Stopped,
                Ok(ran) if ran.code == Some(0) => self.emit(Update::Output {
                    failed: false,
                    step,
                    text: if ran.output.trim().is_empty() {
                        "passed".to_string()
                    } else {
                        ran.output
                    },
                }),
                Ok(ran) => {
                    self.emit(Update::Output {
                        failed: true,
                        step,
                        text: ran.output.clone(),
                    });
                    return Check::Failed(command, ran.output);
                }
                Err(problem) => return Check::Failed(command, problem),
            }
        }
        Check::Passed
    }

    fn advance(&mut self) -> bool {
        let outcome = self.gate.finish();
        let Some(mut current) = self.goal.take() else {
            return false;
        };
        if current.status != Status::Active {
            self.goal = Some(current);
            return false;
        }
        let objective = current.objective.clone();
        let mut next = None;
        if self.engine.native() {
            if outcome.refused {
                self.engine.pause();
            }
            current.turns += self.engine.rounds();
            let round = current.turns;
            let mut stopped = false;
            current.native(&outcome, || match self.verify(&objective, round) {
                Check::Passed => None,
                Check::Failed(command, _) => Some(command),
                Check::Stopped => {
                    stopped = true;
                    None
                }
            });
            if stopped {
                current.status = Status::Paused;
            }
        } else {
            current.turns += 1;
            current.judge(&outcome);
            if current.status == Status::Complete {
                match self.verify(&objective, current.turns) {
                    Check::Passed => {}
                    Check::Stopped => current.status = Status::Paused,
                    Check::Failed(command, output) => {
                        self.note(format!("`{command}` failed, so the goal isn't done yet"));
                        current.status = if current.turns < goal::TURNS {
                            next = Some(goal::retry(&command, &output, &objective));
                            Status::Active
                        } else {
                            Status::Limit
                        };
                    }
                }
            }
            if current.status == Status::Active && next.is_none() {
                next = Some(goal::onward(&objective));
            }
        }
        self.note(if outcome.stopped {
            format!("stopped. {}", current.summary())
        } else {
            current.summary()
        });
        let going = current.status == Status::Active;
        self.goal = Some(current);
        match next {
            Some(prompt) if going => {
                self.send(&prompt);
                self.busy
            }
            _ => {
                self.engine.goal = None;
                false
            }
        }
    }

    fn answer(&mut self, id: &str, choice: Answer) {
        let Some(item) = self.waiting.remove(id) else {
            return;
        };
        if self.followed() {
            sombre_pair::post(&json!({"id": id, "kind": "decided", "yes": choice != Answer::No}));
        }
        if choice == Answer::No {
            self.gate.refuse();
        }
        if self.engine.answer(&item, choice).is_err() {
            self.emit(Update::Failed {
                text: "the agent stopped before it got the answer".to_string(),
            });
            self.settle();
        }
    }

    fn reply(&mut self, id: &str, answers: Option<Vec<String>>) {
        let Some(item) = self.waiting.remove(id) else {
            return;
        };
        if self.followed() {
            sombre_pair::post(&json!({"answers": answers, "id": id, "kind": "answered"}));
        }
        if answers.is_none() && self.engine.agent == Agent::Claude {
            self.gate.refuse();
        }
        if self.engine.reply(&item, answers.as_deref()).is_err() {
            self.emit(Update::Failed {
                text: "the agent stopped before it got the answer".to_string(),
            });
            self.settle();
        }
    }

    fn grant(&mut self) {
        let covered: Vec<String> = self
            .waiting
            .iter()
            .filter(|(_, item)| covers(self.engine.mode, item, &self.engine.cwd))
            .map(|(id, _)| id.clone())
            .collect();
        for id in covered {
            self.answer(&id, Answer::Yes);
            self.emit(Update::Granted { id });
        }
    }

    fn tune(&mut self, tuning: Tuning) -> Result<String, String> {
        if self.busy && (tuning.model.is_some() || tuning.effort.is_some()) {
            return Err("wait for this turn to finish first".to_string());
        }
        let roots = Roots::detect().ok_or("couldn't find your home folder")?;
        let mut said = Vec::new();
        if let Some(model) = tuning.model {
            self.engine.stop();
            if self.engine.agent == Agent::Sombre {
                Config::load(&roots.sombre).and_then(|mut config| {
                    config.model = Some(model.clone());
                    config
                        .save(&roots.sombre)
                        .map_err(|error| format!("couldn't save the model choice: {error}"))
                })?;
            }
            said.push(format!("{} now uses {model}", self.engine.name()));
            self.engine.label = Some(model.clone());
            self.engine.model = Some(model);
        }
        if let Some(mode) = tuning.mode {
            self.engine.shift(mode);
            Config::load(&roots.sombre).and_then(|mut config| {
                config.permissions = Some(mode);
                config
                    .save(&roots.sombre)
                    .map_err(|error| format!("couldn't save permissions: {error}"))
            })?;
            said.push(format!(
                "permissions: {}. {}",
                mode.name(),
                sombre_engine::about(self.engine.agent, mode)
            ));
            self.grant();
        }
        if let Some(effort) = tuning.effort {
            let chosen = (effort != "default").then_some(effort);
            if self.engine.agent == Agent::Claude {
                self.engine.stop();
            }
            self.engine.effort.clone_from(&chosen);
            Config::load(&roots.sombre).and_then(|mut config| {
                config.effort.clone_from(&chosen);
                config
                    .save(&roots.sombre)
                    .map_err(|error| format!("couldn't save the effort level: {error}"))
            })?;
            said.push(match chosen {
                Some(level) => {
                    format!("effort: {level}. sombre uses the closest level each agent takes")
                }
                None => "effort: default. each model decides how hard to think".to_string(),
            });
        }
        self.state();
        Ok(said.join("\n"))
    }

    fn pump(&mut self) {
        match self.engine.next(Duration::from_millis(40)) {
            Next::Idle => {}
            Next::Error(line) => self.errors.push(line),
            Next::Closed => {
                self.engine.stop();
                let reason = self
                    .errors
                    .iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map_or("the agent stopped without an answer", String::as_str)
                    .to_string();
                self.emit(Update::Failed { text: reason });
                self.gate.outcome.failed = true;
                self.ended();
            }
            Next::Events(events) => {
                for event in events {
                    self.apply(event.scrubbed());
                    if !self.busy {
                        break;
                    }
                }
            }
        }
    }

    fn apply(&mut self, event: Event) {
        let approval = matches!(event, Event::Approval { .. } | Event::Questions { .. })
            .then(|| event.clone());
        let raw = self.followed().then(|| event.clone());
        let pass = self.gate.pass(event);
        if let Some(raw) = raw
            && matches!(pass, Pass::Show(_) | Pass::Ask(_) | Pass::Failed(_))
        {
            sombre_pair::mirror(&raw);
        }
        match pass {
            Pass::Skip => {}
            Pass::Show(update) => self.emit(update),
            Pass::Session(id) => {
                self.engine.session = Some(id);
                self.state();
            }
            Pass::Model(name) => {
                self.engine.label = Some(name);
                self.state();
            }
            Pass::Ask(update) => {
                if let (Some(item), Update::Ask { id, .. } | Update::Questions { id, .. }) =
                    (approval, &update)
                {
                    let covered = covers(self.engine.mode, &item, &self.engine.cwd);
                    self.waiting.insert(id.clone(), item);
                    if covered {
                        self.answer(id, Answer::Yes);
                        return;
                    }
                }
                self.emit(update);
            }
            Pass::Done {
                millis,
                cost,
                left,
                last,
            } => {
                self.engine.left = left.or(self.engine.left);
                let spent = self.engine.charge(cost);
                let elapsed = u64::try_from(self.started.elapsed().as_millis()).ok();
                if self.followed() {
                    sombre_pair::post(&json!({
                        "cost": spent,
                        "kind": "done",
                        "left": left,
                        "millis": millis.or(elapsed),
                    }));
                }
                self.emit(Update::Done {
                    cost: spent,
                    left,
                    millis: millis.or(elapsed),
                });
                if last {
                    self.ended();
                }
            }
            Pass::Failed(text) => {
                self.emit(Update::Failed { text });
                self.ended();
            }
            Pass::Halted => {
                self.note(format!(
                    "stopped. tell {} what to do instead",
                    self.engine.name()
                ));
                self.ended();
            }
            Pass::Goal(..) => {}
        }
    }

    fn run(mut self, asks: &Receiver<Ask>) {
        self.state();
        loop {
            let ask = if self.busy {
                match asks.try_recv() {
                    Ok(ask) => Some(ask),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => break,
                }
            } else {
                match asks.recv_timeout(IDLE) {
                    Ok(ask) => Some(ask),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            };
            if let Some(ask) = ask
                && !self.handle(ask)
            {
                break;
            }
            if self.followed() {
                self.phone();
            }
            if !self.busy
                && let Some((text, images)) = self.held.pop_front()
            {
                if self.followed() {
                    sombre_pair::said(&text, &images);
                }
                self.engine.images = images;
                self.send(&text);
            }
            if !self.busy && self.engine.woke() {
                self.wake();
            }
            if self.busy {
                self.pump();
            }
        }
    }
}

struct Handle {
    sender: Sender<Ask>,
    thread: JoinHandle<()>,
    halt: Arc<AtomicBool>,
    cwd: PathBuf,
}

impl Handle {
    fn end(self) {
        self.halt.store(true, Ordering::SeqCst);
        let _ = self.sender.send(Ask::Close);
        let _ = self.thread.join();
    }
}

#[derive(Default)]
pub struct Live(Mutex<HashMap<String, Handle>>);

impl Live {
    pub fn attach(&self, key: &str, channel: Channel<Update>) -> bool {
        self.0.lock().is_ok_and(|held| {
            held.get(key)
                .is_some_and(|handle| handle.sender.send(Ask::Attach(channel)).is_ok())
        })
    }

    pub fn spawn(&self, key: &str, engine: Engine, channel: Channel<Update>) -> Result<(), String> {
        let mut held = self
            .0
            .lock()
            .map_err(|_| "sombre lost track of its chats")?;
        let (sender, receiver) = mpsc::channel();
        let halt = Arc::new(AtomicBool::new(false));
        let cwd = engine.cwd.clone();
        let runner = Runner {
            key: key.to_string(),
            held: VecDeque::new(),
            busy: false,
            clearing: false,
            errors: Vec::new(),
            gate: Gate::new(engine.agent, &engine.cwd),
            engine,
            before: None,
            goal: None,
            halt: Arc::clone(&halt),
            out: channel,
            started: Instant::now(),
            waiting: HashMap::new(),
        };
        let thread = thread::spawn(move || runner.run(&receiver));
        if let Some(old) = held.insert(
            key.to_string(),
            Handle {
                cwd,
                halt,
                sender,
                thread,
            },
        ) {
            let _ = old.sender.send(Ask::Close);
        }
        Ok(())
    }

    pub fn remove(&self, key: &str) {
        let removed = self.0.lock().ok().and_then(|mut held| held.remove(key));
        if let Some(runner) = removed {
            runner.end();
        }
    }

    pub fn clear(&self, folder: &Path) {
        let removed: Vec<Handle> = self.0.lock().map_or_else(
            |_| Vec::new(),
            |mut held| {
                let keys: Vec<String> = held
                    .iter()
                    .filter(|(_, handle)| handle.cwd == folder)
                    .map(|(key, _)| key.clone())
                    .collect();
                keys.iter().filter_map(|key| held.remove(key)).collect()
            },
        );
        for runner in removed {
            runner.end();
        }
    }

    pub fn tell(&self, key: &str, ask: Ask) -> Result<(), String> {
        let held = self
            .0
            .lock()
            .map_err(|_| "sombre lost track of its chats")?;
        let handle = held
            .get(key)
            .ok_or_else(|| "this chat isn't open. pick it again".to_string())?;
        if matches!(ask, Ask::Stop) {
            handle.halt.store(true, Ordering::SeqCst);
        }
        handle
            .sender
            .send(ask)
            .map_err(|_| "this chat stopped. pick it again to restart it".to_string())
    }

    pub fn close(&self) {
        let Ok(mut held) = self.0.lock() else {
            return;
        };
        let runners: Vec<_> = held.drain().map(|(_, runner)| runner).collect();
        drop(held);
        for runner in &runners {
            let _ = runner.sender.send(Ask::Close);
        }
        for runner in runners {
            let _ = runner.thread.join();
        }
    }
}

fn replayed(history: Vec<Event>) -> Vec<serde_json::Value> {
    history
        .into_iter()
        .flat_map(|event| match event.scrubbed() {
            Event::Prompt(text) => vec![json!({"kind": "user", "text": text})],
            Event::Done { millis, cost, left } => {
                vec![json!({"cost": cost, "kind": "done", "left": left, "millis": millis})]
            }
            Event::Approval { .. } | Event::Questions { .. } | Event::Delta(_) => Vec::new(),
            shown => sombre_pair::shown(&shown),
        })
        .map(|mut item| {
            item["again"] = json!(true);
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Event {
        Event::Text(value.to_string())
    }

    #[test]
    fn a_replayed_chat_shows_prompts_and_replies_but_never_old_decisions() {
        let replay = replayed(vec![
            Event::Prompt("make a note".to_string()),
            Event::Approval {
                id: "1".to_string(),
                tool: "Write".to_string(),
                detail: "note.txt".to_string(),
                reason: String::new(),
                change: Vec::new(),
                input: serde_json::Value::Null,
                always: None,
                rules: serde_json::Value::Null,
            },
            Event::Questions {
                id: "2".to_string(),
                input: serde_json::Value::Null,
                questions: Vec::new(),
            },
            Event::Delta("partial".to_string()),
            text("done \u{1b}[31mred"),
            Event::Done {
                millis: Some(900),
                cost: Some(0.02),
                left: None,
            },
        ]);
        let kinds: Vec<&str> = replay
            .iter()
            .filter_map(|item| item["kind"].as_str())
            .collect();
        assert_eq!(kinds, ["user", "text", "done"]);
        assert!(replay.iter().all(|item| item["again"] == true));
        assert_eq!(replay[0]["text"], "make a note");
        assert!(!replay[1]["text"].as_str().unwrap_or("").contains('\u{1b}'));
    }

    #[test]
    fn a_turn_the_agent_starts_itself_ends_at_its_own_done() {
        let mut gate = Gate::new(Agent::Claude, Path::new("/work"));
        gate.own();
        assert!(matches!(
            gate.pass(Event::Text("finished".to_string())),
            Pass::Show(Update::Text { .. })
        ));
        assert!(matches!(
            gate.pass(Event::Done {
                cost: None,
                left: None,
                millis: None
            }),
            Pass::Done { last: true, .. }
        ));
    }

    #[test]
    fn waits_for_claude_to_echo_the_prompt() {
        let mut gate = Gate::new(Agent::Claude, Path::new("/work"));
        assert_eq!(gate.pass(text("replayed")), Pass::Skip);
        assert_eq!(gate.pass(Event::Echo), Pass::Skip);
        assert_eq!(
            gate.pass(text("fresh")),
            Pass::Show(Update::Text {
                text: "fresh".to_string()
            })
        );
        gate.begin();
        assert_eq!(gate.pass(text("next replay")), Pass::Skip);
    }

    fn done() -> Event {
        Event::Done {
            cost: None,
            left: None,
            millis: None,
        }
    }

    #[test]
    fn a_steer_claude_queued_keeps_the_turn_open_until_it_answers() {
        let mut gate = Gate::new(Agent::Claude, Path::new("/work"));
        gate.begin();
        assert_eq!(gate.pass(Event::Echo), Pass::Skip);
        gate.steered();
        assert!(matches!(gate.pass(done()), Pass::Done { last: false, .. }));
        assert_eq!(gate.pass(Event::Echo), Pass::Skip);
        assert!(matches!(gate.pass(done()), Pass::Done { last: true, .. }));
    }

    #[test]
    fn a_steer_claude_merged_ends_with_the_turn() {
        let mut gate = Gate::new(Agent::Claude, Path::new("/work"));
        gate.begin();
        gate.pass(Event::Echo);
        gate.steered();
        gate.pass(Event::Echo);
        assert!(matches!(gate.pass(done()), Pass::Done { last: true, .. }));
        let mut codex = Gate::new(Agent::Codex, Path::new("/work"));
        codex.steered();
        assert!(matches!(codex.pass(done()), Pass::Done { last: true, .. }));
    }

    #[test]
    fn records_what_a_turn_did_for_the_goal() {
        let mut gate = Gate::new(Agent::Sombre, Path::new("/work"));
        gate.pass(Event::Delta("looking".to_string()));
        gate.pass(Event::Step {
            detail: "ls".to_string(),
            id: "1".to_string(),
            label: "Run".to_string(),
        });
        gate.pass(Event::Delta("goal: ".to_string()));
        gate.pass(Event::Delta("complete".to_string()));
        gate.pass(Event::Text("goal: complete".to_string()));
        assert_eq!(
            gate.pass(Event::Goal {
                reason: "ran it".to_string(),
                status: "complete".to_string(),
            }),
            Pass::Goal("complete".to_string(), "ran it".to_string())
        );
        let outcome = gate.finish();
        assert_eq!(outcome.steps, 1);
        assert_eq!(outcome.said, "goal: complete");
        assert_eq!(
            outcome.goal,
            Some(("complete".to_string(), "ran it".to_string()))
        );
        gate.refuse();
        gate.pass(Event::Failed("broke".to_string()));
        let refused = gate.finish();
        assert!(refused.refused && !refused.failed);
        gate.begin();
        gate.pass(Event::Failed("broke".to_string()));
        assert!(gate.finish().failed);
    }

    #[test]
    fn codex_and_sombre_show_everything_at_once() {
        for agent in [Agent::Codex, Agent::Sombre] {
            let mut gate = Gate::new(agent, Path::new("/work"));
            assert!(matches!(gate.pass(text("hi")), Pass::Show(_)));
        }
    }

    #[test]
    fn a_refusal_hides_the_rest_of_the_turn() {
        let mut gate = Gate::new(Agent::Codex, Path::new("/work"));
        gate.refuse();
        assert_eq!(
            gate.pass(Event::Output {
                failed: false,
                id: "1".to_string(),
                text: "late".to_string()
            }),
            Pass::Skip
        );
        assert_eq!(
            gate.pass(Event::Done {
                cost: None,
                left: None,
                millis: None
            }),
            Pass::Halted
        );
        gate.begin();
        assert!(matches!(
            gate.pass(Event::Done {
                cost: None,
                left: None,
                millis: None
            }),
            Pass::Done { .. }
        ));
    }

    #[test]
    fn questions_wait_for_an_answer_in_every_mode() {
        let mut gate = Gate::new(Agent::Claude, Path::new("/work"));
        let asked = vec![Question {
            choices: Vec::new(),
            header: "Color".to_string(),
            multiple: false,
            text: "Which color?".to_string(),
        }];
        let item = Event::Questions {
            id: "q1".to_string(),
            input: serde_json::Value::Null,
            questions: asked.clone(),
        };
        assert!(!covers(Mode::Yolo, &item, Path::new("/work")));
        assert_eq!(
            gate.pass(item),
            Pass::Ask(Update::Questions {
                id: "q1".to_string(),
                questions: asked
            })
        );
    }

    #[test]
    fn turns_steps_images_and_approvals_into_updates() {
        let mut gate = Gate::new(Agent::Sombre, Path::new("/work"));
        assert_eq!(
            gate.pass(Event::Step {
                detail: "steps".to_string(),
                id: "p".to_string(),
                label: "Plan".to_string()
            }),
            Pass::Skip
        );
        assert_eq!(
            gate.pass(Event::Image("/tmp/shot.png".to_string())),
            Pass::Show(Update::Step {
                detail: "shot.png".to_string(),
                label: "View".to_string(),
                step: "/tmp/shot.png".to_string()
            })
        );
        assert!(matches!(
            gate.pass(Event::Approval {
                always: Some("always allow ls".to_string()),
                change: Vec::new(),
                detail: "ls /work/src".to_string(),
                id: "a1".to_string(),
                input: serde_json::Value::Null,
                reason: String::new(),
                rules: serde_json::Value::Null,
                tool: "Bash".to_string()
            }),
            Pass::Ask(Update::Ask { id, detail, .. }) if id == "a1" && detail == "ls src"
        ));
        assert_eq!(
            gate.pass(Event::Session("s1".to_string())),
            Pass::Session("s1".to_string())
        );
    }

    #[test]
    fn serializes_updates_with_a_kind_tag() {
        assert_eq!(
            serde_json::to_value(Update::State {
                busy: true,
                mode: Mode::Auto,
                model: Some("opus".to_string()),
                session: None,
                effort: Some("high".to_string()),
                levels: vec!["low".to_string(), "high".to_string()],
                cost: Some(0.5),
                left: Some(40),
                folder: "/work".to_string(),
                goal: Some(Aim::from(&Goal::new("tests pass".to_string()))),
            })
            .unwrap_or_default(),
            serde_json::json!({"kind": "state", "busy": true, "mode": "auto", "model": "opus", "session": null, "effort": "high", "levels": ["low", "high"], "cost": 0.5, "left": 40, "folder": "/work", "goal": {"objective": "tests pass", "status": "active", "active": true, "summary": "pursuing the goal, turn 1 of 20"}})
        );
        assert_eq!(
            serde_json::to_value(Update::Granted {
                id: "a1".to_string()
            })
            .unwrap_or_default(),
            serde_json::json!({"kind": "granted", "id": "a1"})
        );
        assert_eq!(
            serde_json::from_value::<Choice>(serde_json::json!("always")).ok(),
            Some(Choice::Always)
        );
    }
}

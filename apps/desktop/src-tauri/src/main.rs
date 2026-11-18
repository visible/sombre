#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod editor;
mod feed;
mod live;
mod settings;
mod shot;
mod term;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use sombre_core::{Agent, Roots, Session};
use sombre_engine::{Engine, fallback, kept, recorded};
use sombre_harness::config::{Config, Mode};
use tauri::ipc::Channel;
use tauri::{Emitter, Manager, RunEvent, State};

use live::{Ask, Choice, Live, Tuning, Update};

#[derive(Default)]
struct Known(Mutex<Vec<Session>>);

impl Known {
    fn find(&self, agent: Agent, id: &str) -> Option<Session> {
        self.0.lock().ok().and_then(|held| {
            held.iter()
                .find(|session| session.agent == agent && session.id == id)
                .cloned()
        })
    }
}

#[derive(Serialize, specta::Type)]
struct Folder {
    path: String,
    place: String,
    project: String,
}

#[derive(Serialize, specta::Type)]
struct Defaults {
    agent: Agent,
    mode: Mode,
    folders: Vec<Folder>,
}

#[derive(Serialize, specta::Type)]
struct Model {
    id: String,
    about: String,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("sombre stopped working on that: {error}"))
}

fn roots() -> Result<Roots, String> {
    Roots::detect().ok_or_else(|| "sombre couldn't find your home folder".to_string())
}

fn gone() -> String {
    "that chat is gone. it may have been archived or moved".to_string()
}

#[tauri::command]
#[specta::specta]
async fn chats(known: State<'_, Known>) -> Result<Vec<feed::Chat>, String> {
    let roots = roots()?;
    let sessions = blocking(move || sombre_core::scan(&roots)).await?;
    let listed = feed::chats(
        &sessions,
        sombre_core::now(),
        std::env::home_dir().as_deref(),
    );
    if let Ok(mut held) = known.0.lock() {
        *held = sessions;
    }
    Ok(listed)
}

#[tauri::command]
#[specta::specta]
async fn chat(
    known: State<'_, Known>,
    agent: Agent,
    id: String,
    whole: bool,
) -> Result<Story, String> {
    let session = known.find(agent, &id).ok_or_else(gone)?;
    let roots = roots()?;
    blocking(move || {
        let (events, whole) = if whole {
            (feed::transcript(&session).as_ref().clone(), true)
        } else {
            sombre_core::recent(&session, RECENT)
        };
        let records = if whole {
            sombre_core::ledger::read(&roots, agent, &id)
        } else {
            Vec::new()
        };
        Story {
            entries: feed::story(events, &session.cwd, records),
            whole,
        }
    })
    .await
}

const RECENT: usize = 40;

#[derive(Serialize, specta::Type)]
struct Story {
    entries: Vec<feed::Entry>,
    whole: bool,
}

fn described(path: &Path, home: Option<&Path>) -> Folder {
    Folder {
        path: path.display().to_string(),
        place: feed::place(path, home),
        project: feed::project(path),
    }
}

#[tauri::command]
#[specta::specta]
fn folder(path: String) -> Result<Folder, String> {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err(format!("{} isn't a folder sombre can open", path.display()));
    }
    Ok(described(&path, std::env::home_dir().as_deref()))
}

#[tauri::command]
#[specta::specta]
fn defaults(known: State<'_, Known>) -> Result<Defaults, String> {
    let roots = roots()?;
    let config = Config::load(&roots.sombre).unwrap_or_default();
    let home = std::env::home_dir();
    let mut seen: Vec<PathBuf> = Vec::new();
    if let Ok(held) = known.0.lock() {
        for session in held.iter() {
            if seen.len() == 8 {
                break;
            }
            if !seen.contains(&session.cwd) && session.cwd.is_dir() {
                seen.push(session.cwd.clone());
            }
        }
    }
    Ok(Defaults {
        agent: config.agent.unwrap_or(Agent::Claude),
        folders: seen
            .iter()
            .map(|path| described(path, home.as_deref()))
            .collect(),
        mode: config.permissions.unwrap_or_default(),
    })
}

#[tauri::command]
#[specta::specta]
async fn models(agent: Agent) -> Result<Vec<Model>, String> {
    let roots = roots()?;
    let (listed, _) = blocking(move || sombre_engine::catalog(&roots, agent)).await??;
    Ok(listed
        .into_iter()
        .map(|model| Model {
            about: model.about,
            id: model.id,
        })
        .collect())
}

fn resumed(session: &Session) -> Result<Engine, String> {
    if !session.cwd.is_dir() {
        return Err(format!(
            "{} no longer exists, so this chat can't continue there",
            session.cwd.display()
        ));
    }
    let roots = Roots::detect();
    let (history, _) = sombre_core::recent(session, 0);
    let chosen = recorded(&history);
    let mut engine = Engine::new(session.agent, session.cwd.clone(), Some(session.id.clone()));
    engine.model = chosen
        .as_deref()
        .and_then(|model| kept(session.agent, model, roots.as_ref()));
    engine.label = if engine.model.is_some() {
        chosen
    } else {
        fallback(roots.as_ref(), session.agent)
    };
    Ok(engine)
}

fn fresh(
    agent: Agent,
    folder: PathBuf,
    model: Option<String>,
    mode: Option<Mode>,
    effort: Option<String>,
) -> Result<Engine, String> {
    if !folder.is_dir() {
        return Err(format!(
            "{} isn't a folder sombre can open",
            folder.display()
        ));
    }
    let mut engine = Engine::new(agent, folder, None);
    engine.label = model
        .clone()
        .or_else(|| fallback(Roots::detect().as_ref(), agent));
    engine.model = model;
    if let Some(mode) = mode {
        engine.mode = mode;
    }
    if let Some(effort) = effort {
        engine.effort = (effort != "default").then_some(effort);
    }
    Ok(engine)
}

#[derive(Deserialize, specta::Type)]
struct Opening {
    key: String,
    agent: Agent,
    id: Option<String>,
    folder: Option<String>,
    model: Option<String>,
    mode: Option<Mode>,
    effort: Option<String>,
}

#[tauri::command]
#[specta::specta]
async fn start(
    known: State<'_, Known>,
    live: State<'_, Live>,
    chat: Opening,
    updates: Channel<Update>,
) -> Result<(), String> {
    if live.attach(&chat.key, updates.clone()) {
        return Ok(());
    }
    let engine = match (chat.id, chat.folder) {
        (Some(id), _) => {
            let session = known.find(chat.agent, &id).ok_or_else(gone)?;
            blocking(move || resumed(&session)).await??
        }
        (None, Some(folder)) => fresh(
            chat.agent,
            PathBuf::from(folder),
            chat.model,
            chat.mode,
            chat.effort,
        )?,
        (None, None) => return Err("pick a folder for the new chat first".to_string()),
    };
    live.spawn(&chat.key, engine, updates)
}

#[tauri::command]
#[specta::specta]
fn send(
    live: State<'_, Live>,
    key: String,
    text: String,
    images: Vec<String>,
) -> Result<(), String> {
    live.tell(
        &key,
        Ask::Send(text, images.into_iter().map(PathBuf::from).collect()),
    )
}

#[derive(Serialize, specta::Type)]
struct Picture {
    path: String,
    src: String,
}

fn pictured(path: &Path) -> Result<Picture, String> {
    Ok(Picture {
        path: path.display().to_string(),
        src: sombre_harness::image::url(path)?,
    })
}

#[tauri::command]
#[specta::specta]
async fn keep(data: String) -> Result<Picture, String> {
    use base64::Engine as _;
    let roots = roots()?;
    blocking(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .map_err(|_| "that image couldn't be read".to_string())?;
        pictured(&sombre_harness::image::store(&bytes, &roots.sombre)?)
    })
    .await?
}

#[tauri::command]
#[specta::specta]
async fn picture(path: String) -> Result<Picture, String> {
    let roots = roots()?;
    blocking(move || {
        pictured(&sombre_harness::image::keep(
            Path::new(&path),
            &roots.sombre,
        )?)
    })
    .await?
}

#[tauri::command]
#[specta::specta]
async fn rename(live: State<'_, Live>, key: String, name: String) -> Result<String, String> {
    let name = sombre_core::named(&name)?;
    let (reply, answer) = mpsc::channel();
    live.tell(&key, Ask::Rename(name, reply))?;
    blocking(move || {
        answer
            .recv()
            .unwrap_or_else(|_| Err("this chat stopped. pick it again".to_string()))
    })
    .await?
}

#[tauri::command]
#[specta::specta]
fn compact(live: State<'_, Live>, key: String) -> Result<(), String> {
    live.tell(&key, Ask::Compact)
}

fn shown(config: &Config, roots: &Roots, folder: Option<&str>) -> settings::Settings {
    settings::shown(
        config,
        roots,
        folder.map(Path::new),
        &sombre_harness::mcp::foreign(),
        editor::found(),
    )
}

#[tauri::command]
#[specta::specta]
fn settings(folder: Option<String>) -> Result<settings::Settings, String> {
    let roots = roots()?;
    let config = Config::load(&roots.sombre)?;
    Ok(shown(&config, &roots, folder.as_deref()))
}

fn edited(
    folder: Option<&str>,
    change: impl FnOnce(&mut Config) -> Result<(), String>,
) -> Result<settings::Settings, String> {
    let roots = roots()?;
    let mut config = Config::load(&roots.sombre)?;
    change(&mut config)?;
    config
        .save(&roots.sombre)
        .map_err(|error| format!("couldn't save your settings: {error}"))?;
    Ok(shown(&config, &roots, folder))
}

#[tauri::command]
#[specta::specta]
fn configure(
    change: settings::Change,
    folder: Option<String>,
) -> Result<settings::Settings, String> {
    let awake = change.awake;
    let saved = edited(folder.as_deref(), |config| {
        settings::apply(config, change);
        Ok(())
    })?;
    if let Some(awake) = awake {
        AWAKE.store(awake, Ordering::Relaxed);
    }
    Ok(saved)
}

#[tauri::command]
#[specta::specta]
fn connect(
    name: String,
    url: String,
    key: String,
    folder: Option<String>,
) -> Result<settings::Settings, String> {
    edited(folder.as_deref(), |config| {
        settings::connect(config, &name, &url, &key)
    })
}

#[tauri::command]
#[specta::specta]
fn rekey(name: String, key: String, folder: Option<String>) -> Result<settings::Settings, String> {
    edited(folder.as_deref(), |config| {
        settings::rekey(config, &name, &key)
    })
}

#[tauri::command]
#[specta::specta]
fn disconnect(name: String, folder: Option<String>) -> Result<settings::Settings, String> {
    edited(folder.as_deref(), |config| {
        config.remove(&name);
        Ok(())
    })
}

#[tauri::command]
#[specta::specta]
fn plug(name: String, run: String, folder: Option<String>) -> Result<settings::Settings, String> {
    edited(folder.as_deref(), |config| {
        settings::plug(config, &name, &run)
    })
}

#[tauri::command]
#[specta::specta]
fn unplug(name: String, folder: Option<String>) -> Result<settings::Settings, String> {
    edited(folder.as_deref(), |config| {
        config.mcp.remove(&name);
        Ok(())
    })
}

#[tauri::command]
#[specta::specta]
fn bring(name: String, folder: Option<String>) -> Result<settings::Settings, String> {
    edited(folder.as_deref(), |config| {
        settings::bring(config, &name, sombre_harness::mcp::foreign())
    })
}

#[tauri::command]
#[specta::specta]
fn goal(live: State<'_, Live>, key: String, text: String) -> Result<(), String> {
    live.tell(&key, Ask::Goal(text))
}

#[tauri::command]
#[specta::specta]
fn editors() -> Vec<editor::Editor> {
    editor::found()
}

#[tauri::command]
#[specta::specta]
async fn launch(path: String, editor: Option<String>) -> Result<(), String> {
    blocking(move || editor::open(Path::new(&path), editor.as_deref())).await?
}

#[tauri::command]
#[specta::specta]
async fn files(folder: String) -> Result<Vec<String>, String> {
    blocking(move || sombre_engine::files::list(Path::new(&folder))).await
}

#[derive(Serialize, specta::Type)]
struct Installed {
    agent: Agent,
    version: Option<String>,
    install: String,
    docs: String,
}

const fn setup(agent: Agent) -> (&'static str, &'static str) {
    match (agent, cfg!(windows)) {
        (Agent::Claude, false) => (
            "curl -fsSL https://claude.ai/install.sh | bash",
            "https://code.claude.com/docs/en/setup",
        ),
        (Agent::Claude, true) => (
            "irm https://claude.ai/install.ps1 | iex",
            "https://code.claude.com/docs/en/setup",
        ),
        (Agent::Codex, false) => (
            "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
            "https://github.com/openai/codex",
        ),
        (Agent::Codex, true) => (
            "powershell -ExecutionPolicy ByPass -c \"irm https://chatgpt.com/codex/install.ps1 | iex\"",
            "https://github.com/openai/codex",
        ),
        (Agent::Sombre, _) => ("", "https://sombre.ai"),
    }
}

#[tauri::command]
#[specta::specta]
async fn agents() -> Result<Vec<Installed>, String> {
    blocking(|| {
        [Agent::Claude, Agent::Codex, Agent::Sombre]
            .into_iter()
            .map(|agent| {
                let (install, docs) = setup(agent);
                Installed {
                    agent,
                    docs: docs.to_string(),
                    install: install.to_string(),
                    version: match agent {
                        Agent::Claude => sombre_engine::carry::installed("claude"),
                        Agent::Codex => sombre_engine::carry::installed("codex"),
                        Agent::Sombre => Some(env!("CARGO_PKG_VERSION").to_string()),
                    },
                }
            })
            .collect()
    })
    .await
}

#[tauri::command]
#[specta::specta]
fn pty(
    terms: State<'_, term::Terms>,
    key: String,
    folder: String,
    cols: u16,
    rows: u16,
    output: Channel<term::Out>,
) -> Result<(), String> {
    terms.open(&key, Path::new(&folder), cols, rows, output)
}

#[tauri::command]
#[specta::specta]
fn input(terms: State<'_, term::Terms>, key: String, data: String) -> Result<(), String> {
    terms.write(&key, &data)
}

#[tauri::command]
#[specta::specta]
fn resize(terms: State<'_, term::Terms>, key: String, cols: u16, rows: u16) -> Result<(), String> {
    terms.resize(&key, cols, rows)
}

#[tauri::command]
#[specta::specta]
fn hangup(terms: State<'_, term::Terms>, key: String) {
    terms.hangup(&key);
}

#[derive(Serialize, specta::Type)]
struct Repo {
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    changed: usize,
    files: Vec<String>,
    remote: Option<String>,
}

#[tauri::command]
#[specta::specta]
async fn repo(folder: String) -> Result<Option<Repo>, String> {
    blocking(move || {
        sombre_engine::git::status(Path::new(&folder)).map(|found| Repo {
            ahead: found.ahead,
            behind: found.behind,
            branch: found.branch,
            changed: found.changed,
            files: found.files,
            remote: found.remote,
            upstream: found.upstream,
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
async fn commit(folder: String, message: String) -> Result<String, String> {
    blocking(move || sombre_engine::git::commit(Path::new(&folder), &message)).await?
}

#[tauri::command]
#[specta::specta]
async fn push(folder: String) -> Result<String, String> {
    blocking(move || sombre_engine::git::push(Path::new(&folder))).await?
}

#[tauri::command]
#[specta::specta]
async fn branch(folder: String, name: String) -> Result<String, String> {
    blocking(move || sombre_engine::git::branch(Path::new(&folder), &name)).await?
}

#[tauri::command]
#[specta::specta]
async fn request(folder: String) -> Result<String, String> {
    blocking(move || sombre_engine::git::request(Path::new(&folder))).await?
}

#[derive(Serialize, specta::Type)]
struct Diff {
    from: String,
    to: String,
    text: String,
    cut: bool,
}

#[tauri::command]
#[specta::specta]
async fn changes(folder: String, from: Option<String>, to: Option<String>) -> Result<Diff, String> {
    blocking(move || {
        let path = PathBuf::from(folder);
        let from = from
            .or_else(|| sombre_engine::git::base(&path))
            .ok_or("this folder isn't a git repository")?;
        let to = to
            .or_else(|| sombre_engine::git::snapshot(&path))
            .ok_or("sombre couldn't read this folder's changes")?;
        let patch = sombre_engine::git::patch(&path, &from, &to)?;
        Ok(Diff {
            cut: patch.cut,
            from,
            text: patch.text,
            to,
        })
    })
    .await?
}

#[tauri::command]
#[specta::specta]
fn record(agent: Agent, id: String, change: sombre_core::ledger::Record) -> Result<(), String> {
    sombre_core::ledger::append(&roots()?, agent, &id, &change)
}

#[tauri::command]
#[specta::specta]
async fn purge(live: State<'_, Live>, folder: String) -> Result<usize, String> {
    let path = PathBuf::from(folder);
    live.clear(&path);
    let roots = roots()?;
    blocking(move || {
        let sessions = sombre_core::scan(&roots);
        sombre_engine::purge::run(&roots, &path, &sessions)
    })
    .await?
}

#[derive(Serialize, specta::Type)]
struct Carried {
    session: String,
    note: String,
}

#[tauri::command]
#[specta::specta]
async fn carry(
    live: State<'_, Live>,
    key: String,
    from: Agent,
    id: String,
    to: Agent,
    folder: String,
) -> Result<Carried, String> {
    if from == to {
        return Err(format!("this chat is already in {}", to.name()));
    }
    live.remove(&key);
    let roots = roots()?;
    blocking(move || {
        let (moved, _) = sombre_engine::carry::run(&roots, from, &id, to, Path::new(&folder))
            .map_err(|problem| format!("{problem}. the chat stayed where it was"))?;
        Ok(Carried {
            note: sombre_engine::carry::note(to, moved.carried),
            session: moved.session,
        })
    })
    .await?
}

#[tauri::command]
#[specta::specta]
async fn steer(live: State<'_, Live>, key: String, text: String) -> Result<bool, String> {
    let (reply, answer) = mpsc::channel();
    live.tell(&key, Ask::Steer(text, reply))?;
    blocking(move || answer.recv().unwrap_or(false)).await
}

#[tauri::command]
#[specta::specta]
fn answer(live: State<'_, Live>, key: String, id: String, choice: Choice) -> Result<(), String> {
    live.tell(&key, Ask::Answer(id, choice.into()))
}

#[tauri::command]
#[specta::specta]
fn reply(
    live: State<'_, Live>,
    key: String,
    id: String,
    answers: Option<Vec<String>>,
) -> Result<(), String> {
    live.tell(&key, Ask::Reply(id, answers))
}

#[derive(Serialize, specta::Type)]
struct Paired {
    link: String,
    code: String,
    reach: &'static str,
    home: bool,
}

#[derive(Serialize, specta::Type)]
struct Phone {
    id: String,
    label: String,
    since: u64,
}

fn code(link: &str) -> Result<String, String> {
    qrcode::QrCode::new(link.as_bytes())
        .map(|code| {
            code.render::<qrcode::render::svg::Color>()
                .quiet_zone(true)
                .dark_color(qrcode::render::svg::Color("#0f1011"))
                .light_color(qrcode::render::svg::Color("#edefef"))
                .build()
        })
        .map_err(|error| format!("couldn't draw the pairing code: {error}"))
}

#[tauri::command]
#[specta::specta]
async fn pair(live: State<'_, Live>, key: String, home: Option<bool>) -> Result<Paired, String> {
    let config = Roots::detect()
        .and_then(|roots| Config::load(&roots.sombre).ok())
        .unwrap_or_default();
    let wanted = if home == Some(true) {
        sombre_pair::Reach::Wifi
    } else {
        sombre_pair::Reach::preferred(&config)
    };
    let relay = config.relay.clone();
    let (link, reach) = blocking(move || {
        let switched = home
            .zip(sombre_pair::reach())
            .is_some_and(|(home, reach)| (reach == sombre_pair::Reach::Wifi) != home);
        if switched {
            sombre_pair::close();
        }
        sombre_pair::invite(wanted, relay.as_deref())
    })
    .await??;
    sombre_pair::follow(&key);
    live.tell(&key, Ask::Share(true))?;
    Ok(Paired {
        code: code(&link)?,
        home: reach == sombre_pair::Reach::Wifi,
        link,
        reach: match reach {
            sombre_pair::Reach::Hosted => "via sombre's relay, sealed end to end",
            sombre_pair::Reach::Relay => "through your own relay",
            sombre_pair::Reach::Tailnet => "over tailscale, on both devices",
            sombre_pair::Reach::Wifi => "only on a wi-fi you trust",
        },
    })
}

#[tauri::command]
#[specta::specta]
async fn phones() -> Result<Vec<Phone>, String> {
    blocking(|| {
        sombre_pair::devices()
            .into_iter()
            .map(|device| Phone {
                id: device.id,
                label: device.label,
                since: device.since,
            })
            .collect()
    })
    .await
}

#[tauri::command]
#[specta::specta]
async fn forget(live: State<'_, Live>, id: String) -> Result<bool, String> {
    let forgot = blocking(move || sombre_pair::forget(&id, &[])).await?;
    if let Some(key) = sombre_pair::followed().filter(|_| forgot) {
        let _ = live.tell(&key, Ask::Share(false));
    }
    Ok(forgot)
}

#[tauri::command]
#[specta::specta]
fn take(live: State<'_, Live>, key: String) -> Result<(), String> {
    sombre_pair::follow(&key);
    sombre_pair::take();
    live.tell(&key, Ask::Share(true))
}

#[tauri::command]
#[specta::specta]
async fn unpair() -> Result<bool, String> {
    blocking(sombre_pair::close).await
}

#[tauri::command]
#[specta::specta]
fn followed() -> Option<String> {
    sombre_pair::paired().then(sombre_pair::followed).flatten()
}

#[tauri::command]
#[specta::specta]
fn stop(live: State<'_, Live>, key: String) -> Result<(), String> {
    live.tell(&key, Ask::Stop)
}

#[tauri::command]
#[specta::specta]
async fn tune(
    live: State<'_, Live>,
    key: String,
    model: Option<String>,
    mode: Option<Mode>,
    effort: Option<String>,
) -> Result<String, String> {
    let (reply, answer) = mpsc::channel();
    live.tell(
        &key,
        Ask::Tune(
            Tuning {
                effort,
                mode,
                model,
            },
            reply,
        ),
    )?;
    blocking(move || {
        answer
            .recv()
            .unwrap_or_else(|_| Err("this chat stopped. pick it again".to_string()))
    })
    .await?
}

const SKIP: [&str; 5] = ["PWD", "OLDPWD", "SHLVL", "_", "TERM"];

fn adopt() {
    if std::env::var_os("TERM").is_some() {
        return;
    }
    for (name, value) in sombre_harness::process::login() {
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        if name == "PATH" || std::env::var_os(&name).is_none() {
            unsafe { std::env::set_var(name, value) };
        }
    }
}

static AWAKE: AtomicBool = AtomicBool::new(false);

fn guard(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            if let Some(chat) = sombre_pair::stand() {
                let _ = app.emit("continue", chat);
            }
            let needed = app.state::<Live>().working() || sombre_pair::paired();
            sombre_harness::power::awake(AWAKE.load(Ordering::Relaxed) && needed);
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

fn bindings() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .dangerously_cast_bigints_to_number()
        .disable_serde_phases()
        .commands(tauri_specta::collect_commands![
            chats,
            chat,
            defaults,
            folder,
            models,
            start,
            send,
            steer,
            answer,
            reply,
            pair,
            phones,
            forget,
            unpair,
            take,
            followed,
            stop,
            tune,
            rename,
            compact,
            keep,
            picture,
            settings,
            configure,
            connect,
            rekey,
            disconnect,
            plug,
            unplug,
            bring,
            goal,
            carry,
            purge,
            record,
            changes,
            editors,
            launch,
            repo,
            files,
            commit,
            push,
            branch,
            request,
            agents,
            pty,
            input,
            resize,
            hangup,
            shot::shot
        ])
}

fn main() {
    adopt();
    sombre_pair::named("desktop-pair.json");
    std::thread::spawn(|| {
        let _ = sombre_pair::resume();
    });
    if let Some(roots) = Roots::detect() {
        AWAKE.store(
            !Config::load(&roots.sombre).unwrap_or_default().sleep,
            Ordering::Relaxed,
        );
    }
    let commands = bindings();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(Known::default())
        .manage(Live::default())
        .manage(term::Terms::default())
        .on_page_load(|webview, payload| shot::script(webview, payload.event()))
        .invoke_handler(commands.invoke_handler())
        .build(tauri::generate_context!());
    match app {
        Ok(app) => {
            guard(app.handle().clone());
            if let Some(roots) = Roots::detect() {
                let _ = app
                    .asset_protocol_scope()
                    .allow_directory(roots.sombre.join("images"), true);
            }
            app.run(|handle, event| {
                if let RunEvent::Exit = event {
                    handle.state::<Live>().close();
                    handle.state::<term::Terms>().close();
                }
            });
        }
        Err(error) => {
            eprintln!("sombre couldn't open its window: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_typescript_bindings_match_the_commands() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
        let before = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        super::bindings()
            .export(specta_typescript::Typescript::default().header(""), &path)
            .unwrap_or_else(|error| panic!("{error}"));
        let after: String = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.starts_with("//") && !line.starts_with("/*"))
            .map(|line| format!("{line}\n"))
            .collect();
        std::fs::write(&path, after.trim_start()).unwrap_or_else(|error| panic!("{error}"));
        let after = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            before == after,
            "the typescript bindings were out of date and are rewritten now, commit apps/desktop/src/lib/bindings.ts"
        );
    }
}

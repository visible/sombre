mod approve;
mod attach;
mod carry;
mod chat;
mod clipboard;
mod complete;
mod compose;
mod dock;
mod install;
mod mark;
mod markdown;
mod menu;
mod pane;
mod provider;
mod question;
mod relay;
mod size;
mod streak;
mod tailscale;
mod view;
mod wire;

use std::env;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use sombre_core::{Agent, Lookup, Roots, Session, ago, find, now, scan, transcript};
use sombre_engine::Engine;

use sombre_harness::config::Config;

use crate::provider::Target;

#[derive(Parser)]
#[command(
    name = "sombre",
    disable_version_flag = true,
    about = "every coding agent session on your computer, in one place",
    long_about = "run `sombre` in a folder to start a chat there. run `sombre resume` to pick up an old chat from any agent."
)]
struct Cli {
    #[arg(short = 'V', long, help = "print the version")]
    version: bool,
    #[arg(
        long,
        value_enum,
        help = "which agent a new chat uses. defaults to your last /provider choice"
    )]
    agent: Option<Source>,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    #[command(about = "list sessions from every agent, newest first")]
    Ls {
        #[arg(long, value_enum, help = "only show one agent")]
        agent: Option<Source>,
        #[arg(long, default_value_t = 20, help = "how many sessions to show")]
        limit: usize,
        #[arg(long, help = "show every session")]
        all: bool,
        #[arg(long, help = "print json for scripts")]
        json: bool,
    },
    #[command(
        about = "run a relay so phones can reach your chats from anywhere. it only sees sealed messages"
    )]
    Relay {
        #[arg(long, default_value_t = 8787, help = "port to listen on")]
        port: u16,
    },
    #[command(about = "continue a session. without an id, pick one from this folder")]
    Resume {
        #[arg(help = "session id or a unique prefix of it")]
        id: Option<String>,
        #[arg(long, help = "pick from every folder, not just this one")]
        all: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Source {
    Claude,
    Codex,
    Sombre,
}

impl Source {
    fn agent(self) -> Agent {
        match self {
            Self::Claude => Agent::Claude,
            Self::Codex => Agent::Codex,
            Self::Sombre => Agent::Sombre,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.version {
        view::version();
        return ExitCode::SUCCESS;
    }
    let Ok(cwd) = env::current_dir() else {
        eprintln!("sombre: can't read the current folder");
        return ExitCode::FAILURE;
    };
    let Some(action) = cli.command else {
        return chat::run(start(cli.agent, cwd), &[]);
    };
    if let Action::Relay { port } = action {
        return match relay::serve(port) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("sombre: couldn't start the relay: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let Some(roots) = Roots::detect() else {
        eprintln!("sombre: couldn't find your home folder, so agent history can't be found");
        return ExitCode::FAILURE;
    };
    let sessions = scan(&roots);
    match action {
        Action::Ls {
            agent,
            limit,
            all,
            json,
        } => list(
            &sessions,
            agent,
            if all { usize::MAX } else { limit },
            json,
            &cwd,
        ),
        Action::Resume { id: Some(id), .. } => resume(&sessions, &id),
        Action::Resume { id: None, all } => {
            chat::pick(&sessions, &cwd, all).map_or(ExitCode::SUCCESS, enter)
        }
        Action::Relay { .. } => ExitCode::SUCCESS,
    }
}

fn list(
    sessions: &[Session],
    agent: Option<Source>,
    limit: usize,
    json: bool,
    cwd: &Path,
) -> ExitCode {
    let matching: Vec<&Session> = sessions
        .iter()
        .filter(|session| agent.is_none_or(|source| session.agent == source.agent()))
        .collect();
    let total = matching.len();
    let shown: Vec<&Session> = matching.into_iter().take(limit).collect();
    if json {
        return match serde_json::to_string_pretty(&shown) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("sombre: couldn't write json: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if shown.is_empty() {
        println!(
            "no sessions yet. sombre reads claude code from ~/.claude and codex from ~/.codex"
        );
        return ExitCode::SUCCESS;
    }
    let time = now();
    if io::stdout().is_terminal() {
        pretty(&shown, total, time, cwd);
        return ExitCode::SUCCESS;
    }
    let width = shown
        .iter()
        .map(|session| project(session).chars().count())
        .max()
        .unwrap_or(0)
        .min(24);
    for session in shown {
        println!(
            "{:<6}  {:<8}  {:>7}  {:<width$}  {}",
            session.agent.name(),
            short(&session.id),
            ago(session.updated, time),
            project(session),
            session.title,
        );
    }
    ExitCode::SUCCESS
}

fn resume(sessions: &[Session], id: &str) -> ExitCode {
    match find(sessions, id) {
        Lookup::Found(session) => enter(session),
        Lookup::Missing => {
            eprintln!("sombre: no session starts with \"{id}\". run `sombre ls` to see ids");
            ExitCode::FAILURE
        }
        Lookup::Ambiguous(matches) => {
            eprintln!(
                "sombre: \"{id}\" matches {} sessions, add more characters:",
                matches.len()
            );
            for session in matches {
                eprintln!(
                    "  {}  {}  {}",
                    session.agent.name(),
                    session.id,
                    session.title
                );
            }
            ExitCode::FAILURE
        }
    }
}

fn start(choice: Option<Source>, cwd: PathBuf) -> Engine {
    if let Some(source) = choice {
        return Engine::new(source.agent(), cwd, None);
    }
    let roots = Roots::detect();
    let saved = roots
        .as_ref()
        .and_then(|roots| Config::load(&roots.sombre).ok())
        .and_then(|config| config.agent);
    let target = match (saved, roots.as_ref()) {
        (Some(agent), _) => Target { agent, model: None },
        (None, Some(roots)) if io::stdin().is_terminal() => {
            provider::onboard(roots).unwrap_or(Target {
                agent: Agent::Claude,
                model: None,
            })
        }
        _ => Target {
            agent: Agent::Claude,
            model: None,
        },
    };
    let mut engine = Engine::new(target.agent, cwd, None);
    engine.model = target.model;
    engine
}

fn pretty(shown: &[&Session], total: usize, time: u64, cwd: &Path) {
    let folders: Vec<String> = shown.iter().map(|session| project(session)).collect();
    let ages: Vec<String> = shown
        .iter()
        .map(|session| ago(session.updated, time))
        .collect();
    let place = folders
        .iter()
        .map(|folder| view::cells(folder))
        .max()
        .unwrap_or(0)
        .min(18);
    let age = ages
        .iter()
        .map(|age| age.chars().count())
        .max()
        .unwrap_or(0);
    let room = view::width()
        .saturating_sub(age + place + 6 + 8 + 14)
        .max(12);
    let here = cwd.to_path_buf();
    let count = if shown.len() < total {
        format!("{} of {total} chats, newest first", shown.len())
    } else {
        format!(
            "{total} {}, newest first",
            if total == 1 { "chat" } else { "chats" }
        )
    };
    println!();
    println!("  {}  {}", view::bold("sombre"), view::dim(&count));
    println!();
    for ((session, folder), when) in shown.iter().zip(&folders).zip(&ages) {
        let dot = if session.cwd == here {
            view::ember("●")
        } else {
            " ".to_string()
        };
        println!(
            "  {dot} {}  {}  {}  {}  {}",
            view::dim(&format!("{when:>age$}")),
            view::dim(&format!("{:<6}", session.agent.name())),
            view::pad(folder, place),
            view::pad(&session.title, room),
            view::dim(short(&session.id)),
        );
    }
    println!();
    let hint = if shown.len() < total {
        "sombre resume <id> continues a chat. --all shows every chat"
    } else {
        "sombre resume <id> continues a chat"
    };
    println!("  {}", view::dim(hint));
    println!();
}

fn enter(session: &Session) -> ExitCode {
    chat::open(session).map_or(ExitCode::FAILURE, |engine| {
        chat::run(engine, &transcript(session))
    })
}

fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn project(session: &Session) -> String {
    view::folder(&session.cwd)
}

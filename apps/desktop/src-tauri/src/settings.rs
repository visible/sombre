use std::path::Path;

use serde::{Deserialize, Serialize};
use sombre_core::{Agent, Roots};
use sombre_harness::config::{Config, GATEWAY, Mode, PRESETS, valid};
use sombre_harness::mcp;

#[derive(Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct Provider {
    pub name: String,
    pub url: String,
    pub key: Option<String>,
    pub builtin: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub url: String,
    pub local: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct Offer {
    pub from: String,
    pub name: String,
    pub run: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct Server {
    pub name: String,
    pub run: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct Skill {
    pub name: String,
    pub about: String,
}

#[derive(Debug, PartialEq, Serialize, specta::Type)]
pub struct Settings {
    pub agent: Agent,
    pub mode: Mode,
    pub effort: Option<String>,
    pub publish: bool,
    pub steer: bool,
    pub awake: bool,
    pub notify: bool,
    pub model: String,
    pub editor: Option<String>,
    pub editors: Vec<crate::editor::Editor>,
    pub providers: Vec<Provider>,
    pub presets: Vec<Preset>,
    pub servers: Vec<Server>,
    pub offered: Vec<Offer>,
    pub skills: Vec<Skill>,
    pub home: String,
}

#[derive(Debug, Default, Deserialize, specta::Type)]
pub struct Change {
    #[specta(optional)]
    pub agent: Option<Agent>,
    #[specta(optional)]
    pub mode: Option<Mode>,
    #[specta(optional)]
    pub effort: Option<String>,
    #[specta(optional)]
    pub publish: Option<bool>,
    #[specta(optional)]
    pub steer: Option<bool>,
    #[specta(optional)]
    pub awake: Option<bool>,
    #[specta(optional)]
    pub notify: Option<bool>,
    #[specta(optional)]
    pub model: Option<String>,
    #[specta(optional)]
    pub editor: Option<String>,
}

fn run(server: &mcp::Server) -> String {
    server.url.clone().unwrap_or_else(|| {
        server
            .command
            .iter()
            .chain(&server.args)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    })
}

pub fn shown(
    config: &Config,
    roots: &Roots,
    folder: Option<&Path>,
    foreign: &[(&str, String, mcp::Server)],
    editors: Vec<crate::editor::Editor>,
) -> Settings {
    Settings {
        agent: config.agent.unwrap_or(Agent::Claude),
        awake: !config.sleep,
        notify: !config.silent,
        editor: config.editor.clone(),
        editors,
        effort: config.effort.clone(),
        home: roots.sombre.display().to_string(),
        mode: config.permissions.unwrap_or_default(),
        model: config.model(),
        providers: config
            .names()
            .into_iter()
            .filter_map(|name| config.endpoint(&name))
            .map(|endpoint| Provider {
                builtin: endpoint.name == GATEWAY,
                key: endpoint.source,
                name: endpoint.name,
                url: endpoint.url,
            })
            .collect(),
        offered: foreign
            .iter()
            .filter(|(_, name, _)| !config.mcp.contains_key(name))
            .map(|(from, name, server)| Offer {
                from: (*from).to_string(),
                name: name.clone(),
                run: run(server),
            })
            .collect(),
        presets: PRESETS
            .iter()
            .map(|(id, label, url, local)| Preset {
                id: (*id).to_string(),
                label: (*label).to_string(),
                local: *local,
                url: (*url).to_string(),
            })
            .collect(),
        publish: config.publish,
        servers: config
            .mcp
            .iter()
            .map(|(name, server)| Server {
                name: name.clone(),
                run: run(server),
            })
            .collect(),
        skills: folder
            .map(sombre_harness::skills::find)
            .unwrap_or_default()
            .into_iter()
            .map(|skill| Skill {
                about: skill.about,
                name: skill.name,
            })
            .collect(),
        steer: config.steer,
    }
}

pub fn apply(config: &mut Config, change: Change) {
    if let Some(agent) = change.agent {
        config.agent = Some(agent);
    }
    if let Some(mode) = change.mode {
        config.permissions = Some(mode);
    }
    if let Some(effort) = change.effort {
        config.effort = (effort != "default").then_some(effort);
    }
    if let Some(publish) = change.publish {
        config.publish = publish;
    }
    if let Some(steer) = change.steer {
        config.steer = steer;
    }
    if let Some(awake) = change.awake {
        config.sleep = !awake;
    }
    if let Some(notify) = change.notify {
        config.silent = !notify;
    }
    if let Some(model) = change.model {
        config.model = Some(model);
    }
    if let Some(editor) = change.editor {
        config.editor = (editor != "default").then_some(editor);
    }
}

fn web(text: &str) -> bool {
    text.starts_with("https://") || text.starts_with("http://")
}

pub fn connect(config: &mut Config, name: &str, url: &str, key: &str) -> Result<(), String> {
    let name = name.trim();
    valid(name, &config.names())?;
    let url = url.trim();
    if !web(url) {
        return Err("the url starts with https:// or http://".to_string());
    }
    let mut provider = sombre_harness::config::Provider {
        url: Some(url.to_string()),
        ..sombre_harness::config::Provider::default()
    };
    let _ = provider.keep(name, key.trim());
    config.providers.insert(name.to_string(), provider);
    Ok(())
}

pub fn rekey(config: &mut Config, name: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("paste the key first".to_string());
    }
    if name != GATEWAY && !config.providers.contains_key(name) {
        return Err(format!("{name} isn't one of your providers"));
    }
    let _ = config
        .providers
        .entry(name.to_string())
        .or_default()
        .keep(name, key);
    Ok(())
}

pub fn plug(config: &mut Config, name: &str, command: &str) -> Result<(), String> {
    let name = name.trim();
    let taken: Vec<String> = config.mcp.keys().cloned().collect();
    valid(name, &taken)?;
    let command = command.trim();
    let server = if web(command) {
        mcp::Server {
            url: Some(command.to_string()),
            ..mcp::Server::default()
        }
    } else {
        let mut words = command.split_whitespace().map(str::to_string);
        let program = words
            .next()
            .ok_or("type the command that starts the server, or its url")?;
        mcp::Server {
            args: words.collect(),
            command: Some(program),
            ..mcp::Server::default()
        }
    };
    config.mcp.insert(name.to_string(), server);
    Ok(())
}

pub fn bring(
    config: &mut Config,
    name: &str,
    foreign: Vec<(&str, String, mcp::Server)>,
) -> Result<(), String> {
    let (_, _, server) = foreign
        .into_iter()
        .find(|(_, found, _)| found == name)
        .ok_or_else(|| format!("{name} isn't set up in another agent anymore"))?;
    config.mcp.insert(name.to_string(), server);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn roots() -> Roots {
        Roots {
            claude: PathBuf::from("/home/sam/.claude"),
            codex: PathBuf::from("/home/sam/.codex"),
            sombre: PathBuf::from("/home/sam/.sombre"),
        }
    }

    #[test]
    fn applies_only_what_changed() {
        let mut config = Config::default();
        apply(
            &mut config,
            Change {
                awake: Some(false),
                effort: Some("high".to_string()),
                mode: Some(Mode::Auto),
                ..Change::default()
            },
        );
        assert!(config.sleep);
        assert_eq!(config.effort.as_deref(), Some("high"));
        assert_eq!(config.permissions, Some(Mode::Auto));
        assert_eq!(config.agent, None);
        apply(
            &mut config,
            Change {
                effort: Some("default".to_string()),
                ..Change::default()
            },
        );
        assert_eq!(config.effort, None);
    }

    #[test]
    fn adds_providers_and_servers_with_checked_names() {
        let mut config = Config::default();
        assert!(connect(&mut config, "Local LLM", "http://localhost:1234/v1", "").is_err());
        assert!(connect(&mut config, "local", "localhost:1234", "").is_err());
        assert!(connect(&mut config, "gateway", "https://x.dev/v1", "").is_err());
        assert_eq!(
            connect(&mut config, "local", "http://localhost:1234/v1", ""),
            Ok(())
        );
        assert_eq!(
            config
                .providers
                .get("local")
                .and_then(|provider| provider.url.as_deref()),
            Some("http://localhost:1234/v1")
        );
        assert!(rekey(&mut config, "nowhere", "sk-1").is_err());
        assert!(rekey(&mut config, "local", " ").is_err());
        assert_eq!(
            plug(&mut config, "files", "npx -y files-mcp ~/notes"),
            Ok(())
        );
        assert_eq!(plug(&mut config, "docs", "https://docs.dev/mcp"), Ok(()));
        assert!(plug(&mut config, "files", "npx other").is_err());
        assert!(plug(&mut config, "empty", "  ").is_err());
        let settings = shown(&config, &roots(), None, &[], Vec::new());
        assert_eq!(
            settings.servers,
            [
                Server {
                    name: "docs".to_string(),
                    run: "https://docs.dev/mcp".to_string()
                },
                Server {
                    name: "files".to_string(),
                    run: "npx -y files-mcp ~/notes".to_string()
                }
            ]
        );
    }

    #[test]
    fn offers_servers_from_other_agents_once() {
        let mut config = Config::default();
        let foreign = || {
            vec![(
                "claude code",
                "github".to_string(),
                mcp::Server {
                    command: Some("gh-mcp".to_string()),
                    ..mcp::Server::default()
                },
            )]
        };
        let before = shown(&config, &roots(), None, &foreign(), Vec::new());
        assert_eq!(
            before.offered,
            [Offer {
                from: "claude code".to_string(),
                name: "github".to_string(),
                run: "gh-mcp".to_string()
            }]
        );
        assert_eq!(bring(&mut config, "github", foreign()), Ok(()));
        assert!(bring(&mut config, "missing", foreign()).is_err());
        assert!(
            shown(&config, &roots(), None, &foreign(), Vec::new())
                .offered
                .is_empty()
        );
    }

    #[test]
    fn shows_defaults_and_never_the_key_itself() {
        let config = Config::default();
        let settings = shown(&config, &roots(), None, &[], Vec::new());
        assert_eq!(settings.agent, Agent::Claude);
        assert_eq!(settings.mode, Mode::default());
        assert!(settings.awake);
        assert_eq!(settings.home, "/home/sam/.sombre");
        assert!(
            settings
                .providers
                .iter()
                .any(|provider| provider.name == "gateway")
        );
        let text = serde_json::to_string(&settings).unwrap_or_default();
        assert!(!text.contains("\"key\":\"sk-"), "{text}");
    }
}

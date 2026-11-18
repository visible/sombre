use sombre_core::{Agent, Roots};
use sombre_harness::config::{Config, DEFAULT, GATEWAY, PRESETS, Provider, split, valid};

use crate::{install, view};

const ADD: &str = "+add";
const EDIT: &str = "+edit";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub agent: Agent,
    pub model: Option<String>,
}

pub fn active(agent: Agent, model: Option<&str>) -> String {
    match agent {
        Agent::Claude => "claude".to_string(),
        Agent::Codex => "codex".to_string(),
        Agent::Sombre => split(model.unwrap_or(GATEWAY)).0.to_string(),
    }
}

pub fn onboard(roots: &Roots) -> Option<Target> {
    if Config::exists(&roots.sombre) {
        return None;
    }
    println!();
    view::note("welcome to sombre. pick what new chats use. /provider changes it any time");
    let target = pick(roots, None)?;
    remember(roots, &target);
    Some(target)
}

pub fn pick(roots: &Roots, current: Option<String>) -> Option<Target> {
    let mut config = match Config::load(&roots.sombre) {
        Ok(config) => config,
        Err(problem) => {
            view::failed(&problem);
            return None;
        }
    };
    let mark = |name: &str, hint: String| {
        if current.as_deref() == Some(name) {
            format!("{hint}  current")
        } else {
            hint
        }
    };
    let local = |agent: Agent, label: &str, hint: &str| {
        if install::installed(agent) {
            (label.to_string(), hint.to_string())
        } else {
            (
                view::dim(label),
                "not installed. pick it to install".to_string(),
            )
        }
    };
    let (claude, about) = local(Agent::Claude, "claude code", "your claude code login");
    let mut menu = crate::menu::select("what should this chat use?").item(
        "claude".to_string(),
        claude,
        mark("claude", about),
    );
    let (codex, about) = local(Agent::Codex, "codex", "your codex login");
    menu = menu.item("codex".to_string(), codex, mark("codex", about));
    for name in config.names() {
        let endpoint = config.endpoint(&name);
        let local = endpoint
            .as_ref()
            .is_some_and(sombre_harness::config::Endpoint::local);
        let status = match endpoint.and_then(|endpoint| endpoint.source) {
            Some(source) if source == "saved key" => "sombre's agent, key saved".to_string(),
            Some(source) if source == "keychain" => {
                "sombre's agent, key in your keychain".to_string()
            }
            Some(source) => format!("sombre's agent, key from {source}"),
            None if local => "sombre's agent, runs on this computer".to_string(),
            None => "sombre's agent, needs a key".to_string(),
        };
        let label = if name == GATEWAY {
            "ai gateway".to_string()
        } else {
            name.clone()
        };
        menu = menu.item(name.clone(), label, mark(&name, status));
    }
    menu = menu.item(
        ADD.to_string(),
        "add a provider",
        "any openai-compatible endpoint",
    );
    if !config.providers.is_empty() {
        menu = menu.item(EDIT.to_string(), "edit or remove a provider", "");
    }
    if let Some(current) = current.filter(|current| *current != ADD) {
        menu = menu.initial_value(current);
    }
    let choice = menu.interact().ok()?;
    match choice.as_str() {
        "claude" => install::ready(Agent::Claude, "claude code").then_some(Target {
            agent: Agent::Claude,
            model: None,
        }),
        "codex" => install::ready(Agent::Codex, "codex").then_some(Target {
            agent: Agent::Codex,
            model: None,
        }),
        ADD => {
            let name = create(&mut config)?;
            save(roots, &config, &format!("added {name}"))?;
            target(&config, &name)
        }
        EDIT => edit(&mut config)
            .and_then(|message| save(roots, &config, &message))
            .and(None),
        name => {
            if config
                .endpoint(name)
                .is_some_and(|endpoint| endpoint.key.is_none() && !endpoint.local())
            {
                rekey(&mut config, name)?;
                save(
                    roots,
                    &config,
                    &format!("saved the key for {}", title(name)),
                )?;
            }
            target(&config, name)
        }
    }
}

pub fn remember(roots: &Roots, target: &Target) {
    let saved = Config::load(&roots.sombre).and_then(|mut config| {
        config.agent = Some(target.agent);
        if let Some(model) = &target.model {
            config.model = Some(model.clone());
        }
        config
            .save(&roots.sombre)
            .map_err(|error| format!("couldn't save the choice: {error}"))
    });
    if let Err(problem) = saved {
        view::failed(&problem);
    }
}

fn title(name: &str) -> &str {
    if name == GATEWAY { "ai gateway" } else { name }
}

fn save(roots: &Roots, config: &Config, message: &str) -> Option<()> {
    match config.save(&roots.sombre) {
        Ok(()) => {
            view::note(message);
            Some(())
        }
        Err(error) => {
            view::failed(&format!("couldn't save the provider: {error}"));
            None
        }
    }
}

fn target(config: &Config, name: &str) -> Option<Target> {
    let model = config
        .model
        .clone()
        .filter(|model| split(model).0 == name)
        .or_else(|| (name == GATEWAY).then(|| DEFAULT.to_string()))
        .or_else(|| {
            config
                .providers
                .get(name)
                .and_then(|provider| provider.models.first())
                .map(|model| format!("{name}:{model}"))
        })
        .or_else(|| choose(config, name))?;
    Some(Target {
        agent: Agent::Sombre,
        model: Some(model),
    })
}

fn choose(config: &Config, name: &str) -> Option<String> {
    let endpoint = config.endpoint(name)?;
    view::spin();
    let listed = sombre_harness::provider::models(&endpoint);
    view::clear();
    let picked = match listed {
        Ok(listed) if !listed.is_empty() => listed
            .iter()
            .fold(
                crate::menu::select(format!("model for {}", title(name))),
                |menu, model| menu.item(model.id.clone(), &model.id, &model.name),
            )
            .filter_mode()
            .max_rows(12)
            .interact()
            .ok()?,
        listed => {
            if let Err(problem) = listed {
                view::note(&problem);
            }
            crate::menu::input(format!("model id for {}", title(name)))
                .placeholder("type the model id")
                .interact()
                .ok()?
        }
    };
    Some(format!("{name}:{picked}"))
}

fn create(config: &mut Config) -> Option<String> {
    let taken = config.names();
    let kind = PRESETS
        .iter()
        .fold(
            crate::menu::select("what are you connecting?"),
            |menu, (id, label, url, local)| {
                let hint = if *local {
                    format!("{url}, runs on your computer")
                } else {
                    (*url).to_string()
                };
                menu.item(Some(*id), *label, hint)
            },
        )
        .item(None, "something else", "any openai-compatible endpoint")
        .interact()
        .ok()?;
    let preset = kind.and_then(|id| PRESETS.iter().find(|(preset, ..)| *preset == id));
    let suggested = preset
        .map(|(id, ..)| (*id).to_string())
        .filter(|id| valid(id, &taken).is_ok())
        .unwrap_or_default();
    let name: String = crate::menu::input("name")
        .placeholder(if suggested.is_empty() {
            "my-endpoint"
        } else {
            &suggested
        })
        .default_input(&suggested)
        .validate(move |name: &String| valid(name, &taken))
        .interact()
        .ok()?;
    let url = match preset {
        Some((_, _, url, _)) => (*url).to_string(),
        None => crate::menu::input("base url")
            .placeholder("https://api.example.com/v1")
            .validate(|url: &String| {
                if url.starts_with("https://") || url.starts_with("http://") {
                    Ok(())
                } else {
                    Err("start with https:// or http://")
                }
            })
            .interact()
            .ok()?,
    };
    let key = if preset.is_some_and(|(.., local)| *local) {
        String::new()
    } else {
        crate::menu::password("api key")
            .mask('•')
            .allow_empty()
            .interact()
            .ok()?
    };
    let mut provider = Provider {
        url: Some(url.trim().to_string()),
        ..Provider::default()
    };
    if let Some(problem) = provider.keep(&name, key.trim()) {
        view::note(&problem);
    }
    config.providers.insert(name.clone(), provider);
    Some(name)
}

fn rekey(config: &mut Config, name: &str) -> Option<()> {
    let key = crate::menu::password(format!("api key for {}", title(name)))
        .mask('•')
        .validate(|key: &String| {
            if key.trim().is_empty() {
                Err("paste the key, or press esc to go back")
            } else {
                Ok(())
            }
        })
        .interact()
        .ok()?;
    if let Some(problem) = config
        .providers
        .entry(name.to_string())
        .or_default()
        .keep(name, key.trim())
    {
        view::note(&problem);
    }
    Some(())
}

fn edit(config: &mut Config) -> Option<String> {
    let name = config
        .providers
        .keys()
        .fold(crate::menu::select("which provider?"), |menu, name| {
            menu.item(name.clone(), title(name), "")
        })
        .interact()
        .ok()?;
    let action = crate::menu::select(title(&name))
        .item("key", "replace the key", "")
        .item("remove", "remove it", "")
        .interact()
        .ok()?;
    if action == "key" {
        rekey(config, &name)?;
        return Some(format!("saved the key for {}", title(&name)));
    }
    config.remove(&name);
    Some(format!("removed {}", title(&name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_active_choice() {
        assert_eq!(active(Agent::Claude, None), "claude");
        assert_eq!(active(Agent::Sombre, None), "gateway");
        assert_eq!(
            active(Agent::Sombre, Some("openai/gpt-oss-120b")),
            "gateway"
        );
        assert_eq!(active(Agent::Sombre, Some("custom:model-a")), "custom");
    }
}

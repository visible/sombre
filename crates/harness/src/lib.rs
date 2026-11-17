pub mod config;
mod fetch;
pub mod image;
pub mod keychain;
pub mod mcp;
mod patch;
pub mod power;
pub mod process;
mod prompt;
pub mod provider;
mod run;
mod salvage;
pub mod sandbox;
mod search;
pub mod skills;
pub mod tools;

pub use run::{Answer, Setup, Shared, Turn, squash, start};

use config::Config;
use provider::Listed;

pub fn catalog(config: &Config) -> (Vec<Listed>, Vec<String>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    for name in config.names() {
        let Some(endpoint) = config.endpoint(&name) else {
            continue;
        };
        let saved = config
            .providers
            .get(&name)
            .map(|provider| provider.models.clone())
            .unwrap_or_default();
        let listed = if saved.is_empty() {
            match provider::models(&endpoint) {
                Ok(listed) => listed,
                Err(problem) => {
                    problems.push(problem);
                    continue;
                }
            }
        } else {
            saved
                .into_iter()
                .map(|id| Listed {
                    name: id.clone(),
                    id,
                })
                .collect()
        };
        found.extend(listed.into_iter().map(|model| Listed {
            id: format!("{name}:{}", model.id),
            name: model.name,
        }));
    }
    (found, problems)
}

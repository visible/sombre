use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sombre_core::Roots;
use sombre_engine::{Engine, git};
use sombre_harness::config::Config;

use crate::view;

pub const PARTS: [&str; 8] = [
    "agent",
    "model",
    "permissions",
    "context",
    "cost",
    "branch",
    "pr",
    "folder",
];

type Key = (PathBuf, String);
type Opened = HashMap<Key, Option<(u64, String)>>;

static OPENED: Mutex<Option<Opened>> = Mutex::new(None);
static ASKING: Mutex<Option<HashSet<Key>>> = Mutex::new(None);
static CHANGED: AtomicBool = AtomicBool::new(false);

pub fn changed() -> bool {
    CHANGED.swap(false, Ordering::SeqCst)
}

#[derive(Default)]
pub struct Values {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub permissions: Option<String>,
    pub context: Option<String>,
    pub cost: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub folder: Option<String>,
}

pub fn assemble(order: &[String], values: &Values) -> Vec<String> {
    let order: Vec<&str> = if order.is_empty() {
        PARTS.to_vec()
    } else {
        order.iter().map(String::as_str).collect()
    };
    order
        .into_iter()
        .filter_map(|name| match name {
            "agent" => values.agent.clone(),
            "model" => values.model.clone(),
            "permissions" => values.permissions.clone(),
            "context" => values.context.clone(),
            "cost" => values.cost.clone(),
            "branch" => values.branch.clone(),
            "pr" => values.pr.clone(),
            "folder" => values.folder.clone(),
            _ => None,
        })
        .collect()
}

fn opened(cwd: &Path, branch: &str) -> Option<(u64, String)> {
    let key = (cwd.to_path_buf(), branch.to_string());
    if let Some(found) = OPENED
        .lock()
        .ok()
        .and_then(|known| known.as_ref()?.get(&key).cloned())
    {
        return found;
    }
    let fresh = ASKING
        .lock()
        .is_ok_and(|mut asking| asking.get_or_insert_with(HashSet::new).insert(key.clone()));
    if fresh {
        thread::spawn(move || {
            let found = git::opened(&key.0);
            let shown = found.is_some();
            if let Ok(mut known) = OPENED.lock() {
                known.get_or_insert_with(HashMap::new).insert(key, found);
            }
            if shown {
                CHANGED.store(true, Ordering::SeqCst);
            }
        });
    }
    None
}

pub fn parts(engine: &Engine) -> Vec<String> {
    let order = Roots::detect()
        .and_then(|roots| Config::load(&roots.sombre).ok())
        .map(|config| config.footer)
        .unwrap_or_default();
    let wanted = |name: &str| order.is_empty() || order.iter().any(|part| part == name);
    let branch = (wanted("branch") || wanted("pr"))
        .then(|| git::head(&engine.cwd))
        .flatten();
    let pr = branch
        .as_deref()
        .filter(|_| wanted("pr"))
        .and_then(|branch| opened(&engine.cwd, branch))
        .map(|(number, url)| view::link(&format!("PR #{number}"), &url));
    let values = Values {
        agent: Some(engine.name().to_string()),
        branch,
        context: engine.left.map(|left| format!("{left}% left")),
        cost: engine.cost.map(view::money),
        folder: Some(view::home(&engine.cwd)),
        model: engine.label.as_deref().map(short),
        permissions: Some(engine.mode.name().to_string()),
        pr,
    };
    assemble(&order, &values)
}

fn short(model: &str) -> String {
    model
        .strip_prefix("claude-")
        .map_or_else(|| model.to_string(), |rest| rest.replacen('-', " ", 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_models_drop_their_family_prefix() {
        assert_eq!(short("claude-opus-5-5"), "opus 5-5");
        assert_eq!(short("claude-sonnet-5-5[1m]"), "sonnet 5-5[1m]");
        assert_eq!(short("opus"), "opus");
        assert_eq!(short("gpt-6.2-sol"), "gpt-6.2-sol");
        assert_eq!(
            short("gateway:anthropic/claude-sonnet-5.5"),
            "gateway:anthropic/claude-sonnet-5.5"
        );
    }

    fn values() -> Values {
        Values {
            agent: Some("claude code".to_string()),
            branch: Some("main".to_string()),
            context: Some("82% left".to_string()),
            cost: Some("$0.14".to_string()),
            folder: Some("~/code/app".to_string()),
            model: Some("opus".to_string()),
            permissions: Some("ask".to_string()),
            pr: None,
        }
    }

    #[test]
    fn the_footer_follows_the_order_in_the_config() {
        let order: Vec<String> = ["cost", "branch", "nonsense", "pr", "model"]
            .iter()
            .map(|part| (*part).to_string())
            .collect();
        assert_eq!(assemble(&order, &values()), ["$0.14", "main", "opus"]);
        assert_eq!(
            assemble(&[], &values()),
            [
                "claude code",
                "opus",
                "ask",
                "82% left",
                "$0.14",
                "main",
                "~/code/app"
            ]
        );
    }

    #[test]
    fn links_stay_whole_and_count_only_their_text() {
        let pr = view::link("PR #25177", "https://github.com/visible/sombre/pull/25177");
        assert_eq!(crate::dock::visible(&pr), 9);
        let parts = vec![
            "claude code".to_string(),
            pr.clone(),
            "~/a/very/long/folder/name/here".to_string(),
        ];
        for columns in [80, 45, 30] {
            let line = view::footer(&parts, columns);
            assert!(crate::dock::visible(&line) <= columns);
            if line.contains('\u{1b}') && line.contains("]8;;https") {
                assert!(line.contains(&pr));
            }
        }
    }
}

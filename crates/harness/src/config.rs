use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sombre_core::Agent;

pub const GATEWAY: &str = "gateway";
pub const DEFAULT: &str = "gateway:anthropic/claude-sonnet-5.5";
const URL: &str = "https://ai-gateway.vercel.sh/v1";
const KEYS: [&str; 2] = ["AI_GATEWAY_API_KEY", "VERCEL_OIDC_TOKEN"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Mode {
    #[default]
    Ask,
    Auto,
    Yolo,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Ask, Self::Auto, Self::Yolo];

    pub fn name(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Yolo => "yolo",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Self::Ask => "ask before edits and commands",
            Self::Auto => "edit files freely, ask before commands",
            Self::Yolo => "never ask. full access to your computer",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == text)
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Ask => Self::Auto,
            Self::Auto => Self::Yolo,
            Self::Yolo => Self::Ask,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Agent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Mode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub publish: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub steer: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scrollback: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sleep: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub silent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tailscale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footer: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, Provider>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mcp: BTreeMap<String, crate::mcp::Server>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Api {
    Chat,
    Responses,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keychain: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<Api>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    pub key: Option<String>,
    pub source: Option<String>,
    pub responses: bool,
}

impl Endpoint {
    pub fn local(&self) -> bool {
        let host = self
            .url
            .split_once("://")
            .map_or(self.url.as_str(), |(_, rest)| rest);
        ["localhost", "127.", "0.0.0.0", "[::1]"]
            .iter()
            .any(|start| host.starts_with(start))
    }
}

pub fn file(root: &Path) -> PathBuf {
    root.join("config.toml")
}

#[cfg(unix)]
pub fn secret(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temporary = path.with_extension("new");
    let _ = fs::remove_file(&temporary);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}

#[cfg(not(unix))]
pub fn secret(path: &Path, text: &str) -> io::Result<()> {
    fs::write(path, text)
}

pub const LEVELS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];

pub fn level<'a>(effort: &str, allowed: &[&'a str]) -> Option<&'a str> {
    let rank = |name: &str| LEVELS.iter().position(|level| *level == name);
    let wanted = rank(effort)?;
    allowed
        .iter()
        .rev()
        .find(|level| rank(level).is_some_and(|found| found <= wanted))
        .or_else(|| allowed.first())
        .copied()
}

const RESERVED: [&str; 3] = ["claude", "codex", GATEWAY];

pub const PRESETS: [(&str, &str, &str, bool); 7] = [
    ("ollama", "ollama", "http://localhost:11434/v1", true),
    ("lmstudio", "lm studio", "http://localhost:1234/v1", true),
    ("cliproxy", "cliproxyapi", "http://localhost:8317/v1", false),
    ("openai", "openai", "https://api.openai.com/v1", false),
    (
        "anthropic",
        "anthropic",
        "https://api.anthropic.com/v1",
        false,
    ),
    (
        "openrouter",
        "openrouter",
        "https://openrouter.ai/api/v1",
        false,
    ),
    ("groq", "groq", "https://api.groq.com/openai/v1", false),
];

pub fn valid(name: &str, taken: &[String]) -> Result<(), &'static str> {
    if name.is_empty()
        || !name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        Err("use lowercase letters, digits, and dashes")
    } else if RESERVED.contains(&name) || taken.iter().any(|other| other == name) {
        Err("that name is taken")
    } else {
        Ok(())
    }
}

pub fn split(model: &str) -> (&str, &str) {
    model.split_once(':').unwrap_or((GATEWAY, model))
}

impl Provider {
    pub fn keep(&mut self, name: &str, key: &str) -> Option<String> {
        if key.is_empty() {
            return None;
        }
        match crate::keychain::save(name, key) {
            Ok(()) => {
                self.keychain = true;
                self.key = None;
                None
            }
            Err(problem) => {
                self.keychain = false;
                self.key = Some(key.to_string());
                Some(format!(
                    "{problem}, so the key is saved in the config file instead"
                ))
            }
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = file(root);
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|error| format!("{} has a mistake: {error}", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("couldn't read {}: {error}", path.display())),
        }
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        fs::create_dir_all(root)?;
        let path = file(root);
        let text = toml::to_string(self).map_err(io::Error::other)?;
        secret(&path, &text)
    }

    pub fn model(&self) -> String {
        self.model.clone().unwrap_or_else(|| DEFAULT.to_string())
    }

    pub fn exists(root: &Path) -> bool {
        file(root).exists()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names = vec![GATEWAY.to_string()];
        names.extend(
            self.providers
                .keys()
                .filter(|name| name.as_str() != GATEWAY)
                .cloned(),
        );
        names
    }

    pub fn remove(&mut self, name: &str) {
        self.providers.remove(name);
        crate::keychain::forget(name);
        if self
            .model
            .as_deref()
            .is_some_and(|model| split(model).0 == name)
        {
            self.model = None;
        }
    }

    pub fn endpoint(&self, name: &str) -> Option<Endpoint> {
        let provider = self.providers.get(name);
        let url = match provider.and_then(|provider| provider.url.clone()) {
            Some(url) => url,
            None if name == GATEWAY => URL.to_string(),
            None => return None,
        };
        let saved = provider
            .and_then(|provider| provider.key.clone())
            .filter(|key| !key.is_empty())
            .map(|key| (key, "saved key".to_string()));
        let locked = provider
            .filter(|provider| provider.keychain)
            .and_then(|_| crate::keychain::load(name))
            .map(|key| (key, "keychain".to_string()));
        let named = provider
            .and_then(|provider| provider.env.clone())
            .and_then(|var| env::var(&var).ok().map(|key| (key, var)));
        let fallback = (name == GATEWAY)
            .then(|| {
                KEYS.iter()
                    .find_map(|var| env::var(var).ok().map(|key| (key, (*var).to_string())))
            })
            .flatten();
        let found = saved
            .or(locked)
            .or(named)
            .or(fallback)
            .filter(|(key, _)| !key.trim().is_empty());
        let api = provider.and_then(|provider| provider.api);
        Some(Endpoint {
            key: found.as_ref().map(|(key, _)| key.clone()),
            name: name.to_string(),
            responses: api == Some(Api::Responses) || (name == GATEWAY && api.is_none()),
            source: found.map(|(_, source)| source),
            url: url.trim_end_matches('/').to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_valid_names_and_urls() {
        for (id, _, url, _) in PRESETS {
            assert_eq!(valid(id, &[]), Ok(()));
            assert!(url.starts_with("http") && url.ends_with("/v1"));
        }
    }

    #[test]
    fn accepts_short_lowercase_names() {
        assert_eq!(valid("abliteration", &[]), Ok(()));
        assert_eq!(valid("my-llm-2", &[]), Ok(()));
    }

    #[test]
    fn rejects_bad_or_taken_names() {
        assert!(valid("Bad Name", &[]).is_err());
        assert!(valid("", &[]).is_err());
        assert!(valid("codex", &[]).is_err());
        assert!(valid("gateway", &[]).is_err());
        assert!(valid("local", &["local".to_string()]).is_err());
    }

    #[test]
    fn cycles_permissions_in_menu_order() {
        assert_eq!(Mode::Ask.next(), Mode::Auto);
        assert_eq!(Mode::Auto.next(), Mode::Yolo);
        assert_eq!(Mode::Yolo.next(), Mode::Ask);
    }

    #[test]
    fn splits_provider_and_model() {
        assert_eq!(
            split("gateway:anthropic/claude-sonnet-5.5"),
            ("gateway", "anthropic/claude-sonnet-5.5")
        );
        assert_eq!(
            split("openai/gpt-oss-120b"),
            ("gateway", "openai/gpt-oss-120b")
        );
        assert_eq!(split("local:qwen3"), ("local", "qwen3"));
    }

    #[test]
    fn names_each_permission_mode() {
        assert_eq!(Mode::default(), Mode::Ask);
        assert_eq!(Mode::parse("yolo"), Some(Mode::Yolo));
        assert_eq!(Mode::parse("auto"), Some(Mode::Auto));
        assert_eq!(Mode::parse("full"), None);
    }

    #[test]
    fn starts_empty_when_there_is_no_file() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let config = Config::load(dir.path()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(config, Config::default());
        assert_eq!(config.model(), DEFAULT);
        assert_eq!(config.names(), ["gateway"]);
    }

    #[test]
    fn knows_which_endpoints_run_on_this_mac() {
        let endpoint = |url: &str| Endpoint {
            key: None,
            name: "x".to_string(),
            responses: false,
            source: None,
            url: url.to_string(),
        };
        assert!(endpoint("http://localhost:11434/v1").local());
        assert!(endpoint("http://127.0.0.1:1234/v1").local());
        assert!(!endpoint("https://api.abliteration.ai/v1").local());
    }

    #[test]
    fn picks_the_api_per_provider() {
        let config: Config = toml::from_str(
            "[providers.abliteration]\nurl = \"https://api.abliteration.ai/v1\"\napi = \"responses\"\n\n[providers.local]\nurl = \"http://localhost:11434/v1\"\n\n[providers.gateway]\napi = \"chat\"\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            config
                .endpoint("abliteration")
                .is_some_and(|endpoint| endpoint.responses)
        );
        assert!(
            config
                .endpoint("local")
                .is_some_and(|endpoint| !endpoint.responses)
        );
        assert!(
            config
                .endpoint("gateway")
                .is_some_and(|endpoint| !endpoint.responses)
        );
        assert!(
            Config::default()
                .endpoint("gateway")
                .is_some_and(|endpoint| endpoint.responses)
        );
    }

    #[test]
    fn maps_effort_to_the_nearest_level_an_agent_takes() {
        let claude = ["low", "medium", "high", "xhigh", "max"];
        let codex = ["minimal", "low", "medium", "high", "xhigh"];
        assert_eq!(level("max", &codex), Some("xhigh"));
        assert_eq!(level("minimal", &claude), Some("low"));
        assert_eq!(level("high", &claude), Some("high"));
        assert_eq!(level("loud", &claude), None);
    }

    #[test]
    fn reads_the_phone_settings_written_by_hand() {
        let config: Config =
            toml::from_str("tailscale = true\nrelay = \"http://100.101.102.103:8787/\"\n")
                .unwrap_or_else(|error| panic!("{error}"));
        assert!(config.tailscale);
        assert_eq!(
            config.relay.as_deref(),
            Some("http://100.101.102.103:8787/")
        );
        let plain = toml::to_string(&Config::default()).unwrap_or_else(|error| panic!("{error}"));
        assert!(!plain.contains("tailscale"));
    }

    #[test]
    fn saves_privately_and_reads_back() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let mut config = Config {
            agent: Some(Agent::Sombre),
            permissions: Some(Mode::Yolo),
            model: Some("custom:model-a".to_string()),
            ..Config::default()
        };
        config.providers.insert(
            "custom".to_string(),
            Provider {
                key: Some("secret".to_string()),
                models: vec!["model-a".to_string()],
                url: Some("https://api.example.com/v1/".to_string()),
                ..Provider::default()
            },
        );
        config
            .save(dir.path())
            .unwrap_or_else(|error| panic!("{error}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(file(dir.path()))
                .map(|meta| meta.permissions().mode() & 0o777)
                .unwrap_or_default();
            assert_eq!(mode, 0o600);
        }
        let back = Config::load(dir.path()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(back, config);
        assert_eq!(back.names(), ["gateway", "custom"]);
        let endpoint = back
            .endpoint("custom")
            .unwrap_or_else(|| panic!("no endpoint"));
        assert_eq!(endpoint.url, "https://api.example.com/v1");
        assert_eq!(endpoint.key.as_deref(), Some("secret"));
    }

    #[test]
    fn knows_the_gateway_without_any_config() {
        let config = Config::default();
        let endpoint = config
            .endpoint("gateway")
            .unwrap_or_else(|| panic!("no gateway"));
        assert_eq!(endpoint.url, "https://ai-gateway.vercel.sh/v1");
        assert_eq!(config.endpoint("missing"), None);
    }

    #[test]
    fn reports_mistakes_in_the_file() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        fs::write(file(dir.path()), "model = ").unwrap_or_else(|error| panic!("{error}"));
        assert!(Config::load(dir.path()).is_err());
    }
}

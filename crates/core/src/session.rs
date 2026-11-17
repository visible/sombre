use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Agent {
    Claude,
    Codex,
    Sombre,
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Sombre => "sombre",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub agent: Agent,
    pub id: String,
    pub title: String,
    pub cwd: PathBuf,
    pub updated: u64,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub about: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Lookup<'a> {
    Found(&'a Session),
    Missing,
    Ambiguous(Vec<&'a Session>),
}

const LIMIT: usize = 80;

pub fn named(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    let unquoted = ['"', '\'']
        .iter()
        .find_map(|quote| {
            trimmed
                .strip_prefix(*quote)
                .and_then(|rest| rest.strip_suffix(*quote))
                .filter(|inner| !inner.contains(*quote))
        })
        .unwrap_or(trimmed)
        .trim();
    if unquoted.is_empty() {
        return Err("give the chat a name, like /rename deploy fix");
    }
    if unquoted.contains(['\n', '\r']) {
        return Err("keep the name on one line");
    }
    if unquoted.chars().any(char::is_control) {
        return Err("names can't have control characters");
    }
    if unquoted.chars().count() > LIMIT {
        return Err("keep the name under 80 characters");
    }
    Ok(unquoted.to_string())
}

const TAIL: u64 = 256 * 1024;
const DEEPEST: u64 = 16 * 1024 * 1024;

pub fn tail(path: &Path, window: u64) -> (Vec<String>, bool) {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = fs::File::open(path) else {
        return (Vec::new(), true);
    };
    let size = file.metadata().map_or(0, |meta| meta.len());
    let start = size.saturating_sub(window);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return (Vec::new(), true);
    }
    let lines = String::from_utf8_lossy(&bytes)
        .lines()
        .skip(usize::from(start > 0))
        .map(str::to_string)
        .collect();
    (lines, start == 0)
}

pub fn latest(session: &Session) -> Option<String> {
    let asked = match session.agent {
        Agent::Claude => crate::claude::asked,
        Agent::Codex => crate::codex::asked,
        Agent::Sombre => crate::sombre::asked,
    };
    let mut window = TAIL;
    loop {
        let (lines, whole) = tail(&session.path, window);
        if let Some(found) = lines.iter().rev().find_map(|line| asked(line)) {
            return Some(found);
        }
        if whole || window >= DEEPEST {
            return None;
        }
        window *= 4;
    }
}

pub fn title(text: &str) -> Option<String> {
    let clean = crate::scrub::scrub(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.is_empty() {
        return None;
    }
    if clean.chars().count() <= LIMIT {
        return Some(clean);
    }
    let cut: String = clean.chars().take(LIMIT - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

pub fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn now() -> u64 {
    seconds(SystemTime::now())
}

pub fn ago(updated: u64, now: u64) -> String {
    let delta = now.saturating_sub(updated);
    match delta {
        0..60 => "now".to_string(),
        60..3_600 => format!("{}m ago", delta / 60),
        3_600..86_400 => format!("{}h ago", delta / 3_600),
        86_400..604_800 => format!("{}d ago", delta / 86_400),
        _ => format!("{}w ago", delta / 604_800),
    }
}

pub fn find<'a>(sessions: &'a [Session], prefix: &str) -> Lookup<'a> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Lookup::Missing;
    }
    if let Some(exact) = sessions.iter().find(|session| session.id == prefix) {
        return Lookup::Found(exact);
    }
    let matches: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.id.starts_with(prefix))
        .collect();
    match matches.as_slice() {
        [] => Lookup::Missing,
        [only] => Lookup::Found(only),
        _ => Lookup::Ambiguous(matches),
    }
}

pub fn here<'a>(sessions: &'a [Session], dir: &Path) -> Vec<&'a Session> {
    let target = resolve(dir);
    sessions
        .iter()
        .filter(|session| resolve(&session.cwd) == target)
        .collect()
}

fn resolve(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(agent: Agent, path: &Path) -> Session {
        Session {
            agent,
            cwd: PathBuf::from("/work"),
            id: "s1".to_string(),
            path: path.to_path_buf(),
            title: "first prompt".to_string(),
            updated: 0,
        }
    }

    #[test]
    fn the_latest_prompt_comes_from_the_end_of_each_agents_file() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let claude = dir.path().join("claude.jsonl");
        let padding = format!(
            "{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}]}}}}\n",
            "x".repeat(4000)
        );
        let mut text = String::from(
            "{\"type\":\"user\",\"cwd\":\"/work\",\"message\":{\"content\":\"first prompt\"}}\n",
        );
        for _ in 0..80 {
            text.push_str(&padding);
        }
        text.push_str("{\"type\":\"user\",\"message\":{\"content\":\"now add the tests\"}}\n");
        text.push_str("{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"content\":\"ok\"}]}}\n");
        for _ in 0..80 {
            text.push_str(&padding);
        }
        fs::write(&claude, text).unwrap_or_else(|error| panic!("{error}"));
        assert!(fs::metadata(&claude).map(|meta| meta.len()).unwrap_or(0) > TAIL);
        assert_eq!(
            latest(&chat(Agent::Claude, &claude)).as_deref(),
            Some("now add the tests")
        );
        let codex = dir.path().join("codex.jsonl");
        fs::write(
            &codex,
            concat!(
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first prompt"}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>"}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"ship it"}]}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}}"#,
                "\n"
            ),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            latest(&chat(Agent::Codex, &codex)).as_deref(),
            Some("ship it")
        );
        let own = dir.path().join("sombre.jsonl");
        fs::write(
            &own,
            concat!(
                r#"{"type":"message","message":{"role":"user","content":"first prompt"}}"#,
                "\n",
                r#"{"type":"message","message":{"role":"user","content":"rename the button"}}"#,
                "\n",
                r#"{"type":"message","message":{"role":"assistant","content":"renamed"}}"#,
                "\n"
            ),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            latest(&chat(Agent::Sombre, &own)).as_deref(),
            Some("rename the button")
        );
    }

    fn session(agent: Agent, id: &str) -> Session {
        Session {
            agent,
            cwd: PathBuf::from("/work/atlas"),
            id: id.to_string(),
            path: PathBuf::new(),
            title: "fix the flaky test".to_string(),
            updated: 0,
        }
    }

    #[test]
    fn checks_names_like_fx_does() {
        assert_eq!(named("  \"deploy fix\"  "), Ok("deploy fix".to_string()));
        assert_eq!(named("'it's'"), Ok("'it's'".to_string()));
        assert_eq!(named("plain"), Ok("plain".to_string()));
        assert!(named("   ").is_err());
        assert!(named("two\nlines").is_err());
        assert!(named("bell\u{7}").is_err());
        assert!(named(&"x".repeat(81)).is_err());
    }

    #[test]
    fn title_collapses_whitespace() {
        assert_eq!(
            title("  fix   the\n\nflaky  test "),
            Some("fix the flaky test".to_string())
        );
    }

    #[test]
    fn title_rejects_empty_text() {
        assert_eq!(title(" \n\t "), None);
    }

    #[test]
    fn title_truncates_long_text_on_a_character_boundary() {
        let long = "é".repeat(200);
        let result = title(&long).unwrap_or_default();
        assert_eq!(result.chars().count(), LIMIT);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn ago_reads_naturally() {
        assert_eq!(ago(1_000, 1_030), "now");
        assert_eq!(ago(0, 125), "2m ago");
        assert_eq!(ago(0, 7_300), "2h ago");
        assert_eq!(ago(0, 200_000), "2d ago");
        assert_eq!(ago(0, 1_300_000), "2w ago");
    }

    #[test]
    fn ago_handles_clock_skew() {
        assert_eq!(ago(500, 100), "now");
    }

    #[test]
    fn find_prefers_an_exact_id() {
        let sessions = [session(Agent::Claude, "ab"), session(Agent::Codex, "abc")];
        assert_eq!(find(&sessions, "ab"), Lookup::Found(&sessions[0]));
    }

    #[test]
    fn find_accepts_a_unique_prefix() {
        let sessions = [
            session(Agent::Claude, "abc1"),
            session(Agent::Codex, "def2"),
        ];
        assert_eq!(find(&sessions, "de"), Lookup::Found(&sessions[1]));
    }

    #[test]
    fn find_reports_ambiguous_prefixes() {
        let sessions = [
            session(Agent::Claude, "abc1"),
            session(Agent::Codex, "abc2"),
        ];
        assert_eq!(
            find(&sessions, "abc"),
            Lookup::Ambiguous(vec![&sessions[0], &sessions[1]])
        );
    }

    #[test]
    fn find_reports_missing_and_blank_ids() {
        let sessions = [session(Agent::Claude, "abc1")];
        assert_eq!(find(&sessions, "zzz"), Lookup::Missing);
        assert_eq!(find(&sessions, "  "), Lookup::Missing);
    }

    #[test]
    fn here_keeps_only_sessions_from_this_folder() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let mut inside = session(Agent::Claude, "in");
        inside.cwd = dir.path().to_path_buf();
        let outside = session(Agent::Codex, "out");
        let sessions = [inside, outside];
        assert_eq!(here(&sessions, dir.path()), vec![&sessions[0]]);
    }

    #[cfg(unix)]
    #[test]
    fn here_matches_folders_reached_through_a_symlink() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        fs::create_dir(&real).unwrap_or_else(|error| panic!("{error}"));
        std::os::unix::fs::symlink(&real, &link).unwrap_or_else(|error| panic!("{error}"));
        let mut inside = session(Agent::Claude, "in");
        inside.cwd = real;
        let sessions = [inside];
        assert_eq!(here(&sessions, &link).len(), 1);
    }
}

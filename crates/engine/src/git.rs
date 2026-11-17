use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const EMPTY: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const LIMIT: usize = 4_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Count {
    pub path: String,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub text: String,
    pub cut: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Repo {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: usize,
    pub files: Vec<String>,
    pub remote: Option<String>,
}

const LISTED: usize = 50;

fn entry(part: &str) -> Option<String> {
    let skipped = match part.chars().next()? {
        '1' => 8,
        '2' => 9,
        'u' => 10,
        '?' | '!' => 1,
        _ => return None,
    };
    let path: Vec<&str> = part.splitn(skipped + 1, ' ').collect();
    path.get(skipped).map(|text| (*text).to_string())
}

fn git(cwd: &Path) -> Command {
    tool(Path::new("git"), cwd)
}

fn tool(program: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn run(command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("couldn't run git: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        Err(said
            .trim()
            .lines()
            .last()
            .unwrap_or("git failed")
            .to_string())
    }
}

pub fn root(cwd: &Path) -> Option<PathBuf> {
    run(git(cwd).args(["rev-parse", "--show-toplevel"]))
        .ok()
        .map(|text| PathBuf::from(text.trim()))
}

pub fn base(cwd: &Path) -> Option<String> {
    root(cwd)?;
    Some(
        run(git(cwd).args(["rev-parse", "--verify", "--quiet", "HEAD^{tree}"]))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|tree| !tree.is_empty())
            .unwrap_or_else(|| EMPTY.to_string()),
    )
}

pub fn snapshot(cwd: &Path) -> Option<String> {
    let top = root(cwd)?;
    let index = run(git(&top).args(["rev-parse", "--git-path", "index"])).ok()?;
    let index = top.join(index.trim());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let scratch = std::env::temp_dir().join(format!("sombre-index-{}-{stamp}", std::process::id()));
    if index.is_file() && fs::copy(&index, &scratch).is_err() {
        return None;
    }
    let _ = run(git(&top)
        .env("GIT_INDEX_FILE", &scratch)
        .args(["add", "-A", "--ignore-errors"]));
    let tree = run(git(&top).env("GIT_INDEX_FILE", &scratch).arg("write-tree"));
    let _ = fs::remove_file(&scratch);
    tree.ok()
        .map(|text| text.trim().to_string())
        .filter(|tree| !tree.is_empty())
}

pub fn counts(cwd: &Path, from: &str, to: &str) -> Result<Vec<Count>, String> {
    let top = root(cwd).ok_or("this folder isn't a git repository")?;
    let text = run(git(&top).args(["diff", "--numstat", "-z", "-M", from, to]))?;
    Ok(numstat(&text))
}

pub fn patch(cwd: &Path, from: &str, to: &str) -> Result<Patch, String> {
    let top = root(cwd).ok_or("this folder isn't a git repository")?;
    let text = run(git(&top).args(["diff", "--no-color", "--no-ext-diff", "-M", "-U3", from, to]))?;
    let cut = text.len() > LIMIT;
    let text = if cut {
        let mut end = LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let kept = text[..end].rfind("\ndiff --git ").unwrap_or(end);
        text[..kept].to_string()
    } else {
        text
    };
    Ok(Patch { cut, text })
}

pub fn listed(cwd: &Path) -> Option<Vec<String>> {
    root(cwd)?;
    let text = run(git(cwd).args([
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "--full-name",
        "-z",
    ]))
    .ok()?;
    let prefix = run(git(cwd).args(["rev-parse", "--show-prefix"]))
        .ok()
        .map(|text| text.trim().to_string())
        .unwrap_or_default();
    let mut found: Vec<String> = text
        .split('\0')
        .filter(|path| !path.is_empty())
        .filter_map(|path| path.strip_prefix(prefix.as_str()).map(str::to_string))
        .collect();
    found.sort();
    found.dedup();
    Some(found)
}

pub fn status(cwd: &Path) -> Option<Repo> {
    let top = root(cwd)?;
    let text = run(git(&top).args([
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=all",
        "-z",
    ]))
    .ok()?;
    let mut repo = porcelain(&text);
    repo.remote = run(git(&top).arg("remote"))
        .ok()
        .and_then(|names| names.lines().next().map(str::to_string))
        .filter(|name| !name.is_empty());
    Some(repo)
}

fn porcelain(text: &str) -> Repo {
    let mut repo = Repo::default();
    let mut parts = text.split('\0').filter(|part| !part.is_empty());
    while let Some(part) = parts.next() {
        if let Some(head) = part.strip_prefix("# branch.head ") {
            repo.branch = (head != "(detached)").then(|| head.to_string());
        } else if let Some(upstream) = part.strip_prefix("# branch.upstream ") {
            repo.upstream = Some(upstream.to_string());
        } else if let Some(counts) = part.strip_prefix("# branch.ab ") {
            let mut numbers = counts
                .split_whitespace()
                .map(|value| value.trim_start_matches(['+', '-']).parse().unwrap_or(0));
            repo.ahead = numbers.next().unwrap_or(0);
            repo.behind = numbers.next().unwrap_or(0);
        } else if !part.starts_with('#') {
            repo.changed += 1;
            if repo.files.len() < LISTED
                && let Some(path) = entry(part)
            {
                repo.files.push(path);
            }
            if part.starts_with("2 ") {
                let _ = parts.next();
            }
        }
    }
    repo
}

pub fn commit(cwd: &Path, message: &str) -> Result<String, String> {
    let top = root(cwd).ok_or("this folder isn't a git repository")?;
    let message = message.trim();
    if message.is_empty() {
        return Err("write a commit message first".to_string());
    }
    run(git(&top).args(["add", "-A"]))?;
    run(git(&top).args(["commit", "-q", "-m", message]))?;
    Ok(run(git(&top).args(["rev-parse", "--short", "HEAD"]))?
        .trim()
        .to_string())
}

pub fn push(cwd: &Path) -> Result<String, String> {
    let top = root(cwd).ok_or("this folder isn't a git repository")?;
    let repo = status(&top).unwrap_or_default();
    let branch = repo.branch.ok_or("check out a branch before pushing")?;
    if repo.upstream.is_some() {
        run(git(&top).arg("push"))?;
    } else {
        let remote = repo
            .remote
            .ok_or("this repository has no remote to push to")?;
        run(git(&top).args(["push", "-u", &remote, &branch]))?;
    }
    Ok(branch)
}

pub fn branch(cwd: &Path, name: &str) -> Result<String, String> {
    let top = root(cwd).ok_or("this folder isn't a git repository")?;
    let name = name.trim();
    run(git(&top).args(["check-ref-format", "--branch", name]))
        .map_err(|_| format!("{name} isn't a valid branch name"))?;
    run(git(&top).args(["switch", "-c", name]))?;
    Ok(name.to_string())
}

pub fn head(cwd: &Path) -> Option<String> {
    let top = root(cwd)?;
    run(git(&top).args(["symbolic-ref", "--quiet", "--short", "HEAD"]))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|name| !name.is_empty())
}

pub fn opened(cwd: &Path) -> Option<(u64, String)> {
    let top = root(cwd)?;
    let program = sombre_harness::process::find("gh")?;
    let text = run(tool(&program, &top).args([
        "pr",
        "view",
        "--json",
        "number,url,state",
        "-q",
        r#"select(.state == "OPEN") | "\(.number) \(.url)""#,
    ]))
    .ok()?;
    let (number, url) = text.trim().split_once(' ')?;
    let url = url.trim();
    (url.starts_with("https://") && !url.chars().any(char::is_control))
        .then(|| Some((number.parse().ok()?, url.to_string())))
        .flatten()
}

pub fn request(cwd: &Path) -> Result<String, String> {
    let top = root(cwd).ok_or("this folder isn't a git repository")?;
    let program = sombre_harness::process::find("gh")
        .ok_or("install the github cli (gh) and sign in to open pull requests")?;
    let found = run(tool(&program, &top).args(["pr", "view", "--json", "url", "-q", ".url"]))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|url| !url.is_empty());
    if let Some(url) = found {
        return Ok(url);
    }
    let made = run(tool(&program, &top).args(["pr", "create", "--fill"]))?;
    made.lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("http"))
        .map(str::to_string)
        .ok_or_else(|| "gh opened the pull request but didn't say where".to_string())
}

fn numstat(text: &str) -> Vec<Count> {
    let mut parts = text.split('\0').filter(|part| !part.is_empty());
    let mut found = Vec::new();
    while let Some(part) = parts.next() {
        let mut fields = part.splitn(3, '\t');
        let added = fields.next().and_then(|value| value.parse().ok());
        let removed = fields.next().and_then(|value| value.parse().ok());
        let path = match fields.next() {
            Some("") => {
                let _ = parts.next();
                parts.next().unwrap_or_default().to_string()
            }
            Some(path) => path.to_string(),
            None => continue,
        };
        found.push(Count {
            added,
            path,
            removed,
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "test@sombre.ai"],
            vec!["config", "user.name", "test"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            run(git(dir.path()).args(args)).unwrap_or_else(|error| panic!("{error}"));
        }
        dir
    }

    fn write(dir: &Path, name: &str, text: &str) {
        fs::write(dir.join(name), text).unwrap_or_else(|error| panic!("{error}"));
    }

    #[test]
    fn reads_numstat_with_renames_and_binaries() {
        let text = "3\t1\tsrc/a.rs\0-\t-\tlogo.png\0\
                    2\t0\t\0old.rs\0new.rs\0";
        assert_eq!(
            numstat(text),
            [
                Count {
                    added: Some(3),
                    path: "src/a.rs".to_string(),
                    removed: Some(1)
                },
                Count {
                    added: None,
                    path: "logo.png".to_string(),
                    removed: None
                },
                Count {
                    added: Some(2),
                    path: "new.rs".to_string(),
                    removed: Some(0)
                },
            ]
        );
    }

    #[test]
    fn snapshots_see_new_and_edited_files_without_touching_the_index() {
        let dir = repo();
        write(dir.path(), "a.txt", "one\n");
        run(git(dir.path()).args(["add", "a.txt"])).unwrap_or_else(|error| panic!("{error}"));
        run(git(dir.path()).args(["commit", "-qm", "first"]))
            .unwrap_or_else(|error| panic!("{error}"));
        let before = snapshot(dir.path()).unwrap_or_else(|| panic!("no snapshot"));
        assert_eq!(base(dir.path()).as_deref(), Some(before.as_str()));
        write(dir.path(), "a.txt", "one\ntwo\n");
        write(dir.path(), "b.txt", "new\n");
        let after = snapshot(dir.path()).unwrap_or_else(|| panic!("no snapshot"));
        let found = counts(dir.path(), &before, &after).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].added, Some(1));
        assert_eq!(found[1].path, "b.txt");
        let shown = patch(dir.path(), &before, &after).unwrap_or_else(|error| panic!("{error}"));
        assert!(shown.text.contains("+two"));
        assert!(!shown.cut);
        let staged = run(git(dir.path()).args(["diff", "--cached", "--name-only"]))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(staged.trim().is_empty());
    }

    #[test]
    fn reads_branch_tracking_and_changed_files() {
        let text = "# branch.oid 1a2b\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
                    1 .M N... 100644 100644 100644 aaa bbb notes.md\0\
                    2 R. N... 100644 100644 100644 ccc ddd R100 new.rs\0old.rs\0\
                    ? hello.txt\0";
        assert_eq!(
            porcelain(text),
            Repo {
                ahead: 2,
                behind: 1,
                branch: Some("main".to_string()),
                changed: 3,
                files: vec![
                    "notes.md".to_string(),
                    "new.rs".to_string(),
                    "hello.txt".to_string()
                ],
                remote: None,
                upstream: Some("origin/main".to_string()),
            }
        );
        assert_eq!(porcelain("# branch.head (detached)\0").branch, None);
    }

    #[test]
    fn commits_everything_and_makes_branches() {
        let dir = repo();
        write(dir.path(), "a.txt", "one\n");
        assert!(commit(dir.path(), "  ").is_err());
        let sha = commit(dir.path(), "add a").unwrap_or_else(|error| panic!("{error}"));
        assert!(!sha.is_empty());
        let after = status(dir.path()).unwrap_or_else(|| panic!("no status"));
        assert_eq!(after.changed, 0);
        assert_eq!(after.branch.as_deref(), Some("main"));
        assert_eq!(after.remote, None);
        assert!(push(dir.path()).is_err());
        assert!(branch(dir.path(), "bad name").is_err());
        assert_eq!(branch(dir.path(), "fix/thing").as_deref(), Ok("fix/thing"));
        assert_eq!(
            status(dir.path()).and_then(|repo| repo.branch).as_deref(),
            Some("fix/thing")
        );
        let remote = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        run(git(remote.path()).args(["init", "-q", "--bare"]))
            .unwrap_or_else(|error| panic!("{error}"));
        let place = remote.path().display().to_string();
        run(git(dir.path()).args(["remote", "add", "origin", &place]))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(push(dir.path()).as_deref(), Ok("fix/thing"));
        let published = status(dir.path()).unwrap_or_else(|| panic!("no status"));
        assert_eq!(published.upstream.as_deref(), Some("origin/fix/thing"));
        assert_eq!(published.remote.as_deref(), Some("origin"));
        write(dir.path(), "a.txt", "two\n");
        commit(dir.path(), "change a").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(status(dir.path()).map(|repo| repo.ahead), Some(1));
        assert_eq!(push(dir.path()).as_deref(), Ok("fix/thing"));
        assert_eq!(status(dir.path()).map(|repo| repo.ahead), Some(0));
    }

    #[test]
    fn lists_tracked_and_new_files_but_not_ignored_ones() {
        let dir = repo();
        write(dir.path(), ".gitignore", "secret.txt\n");
        write(dir.path(), "a.txt", "one\n");
        write(dir.path(), "secret.txt", "shh\n");
        commit(dir.path(), "first").unwrap_or_else(|error| panic!("{error}"));
        write(dir.path(), "b.txt", "new\n");
        assert_eq!(
            listed(dir.path()),
            Some(vec![
                ".gitignore".to_string(),
                "a.txt".to_string(),
                "b.txt".to_string()
            ])
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_does_not_sink_the_snapshot() {
        use std::os::unix::fs::PermissionsExt;
        let dir = repo();
        write(dir.path(), "a.txt", "one\n");
        write(dir.path(), "locked.txt", "secret\n");
        let locked = dir.path().join("locked.txt");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
            .unwrap_or_else(|error| panic!("{error}"));
        if fs::read(&locked).is_ok() {
            return;
        }
        let taken = snapshot(dir.path());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o644))
            .unwrap_or_else(|error| panic!("{error}"));
        let tree = taken.unwrap_or_else(|| panic!("no snapshot"));
        let found = counts(dir.path(), EMPTY, &tree).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "a.txt");
    }

    #[test]
    fn a_repo_without_commits_starts_from_the_empty_tree() {
        let dir = repo();
        assert_eq!(base(dir.path()).as_deref(), Some(EMPTY));
        let plain = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(base(plain.path()), None);
        assert_eq!(snapshot(plain.path()), None);
    }
}

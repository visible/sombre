use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crossterm::cursor::MoveTo;
use crossterm::execute;
use crossterm::style::{Attribute, Color, Stylize};
use crossterm::terminal::{self, ClearType};
use sombre_core::Event as Item;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::mark;
use crate::markdown::{Block, Reader, Span};

const EMBER: Color = Color::Rgb {
    r: 255,
    g: 91,
    b: 20,
};
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const RECENT: usize = 10;

const LABELS: [&str; 12] = [
    "Run", "Read", "Edit", "Find", "Web", "List", "Plan", "Goal", "MCP", "Task", "Test", "View",
];

const KEPT: usize = 4_000;
const ROWS: usize = 2_000;

static STEPPING: AtomicBool = AtomicBool::new(false);
static STORY: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static EARLIER: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static SINK: Mutex<Option<Vec<String>>> = Mutex::new(None);
static RESIZED: AtomicBool = AtomicBool::new(false);
static BLANKS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static TEE: AtomicUsize = AtomicUsize::new(usize::MAX);
static NARROW: AtomicUsize = AtomicUsize::new(0);
static COLUMNS: AtomicUsize = AtomicUsize::new(0);
const PHONE: usize = 44;
const HELD: usize = 20_000;

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Said(String),
    Text(String),
    Step(String, String, PathBuf),
    Tally(Vec<(String, usize)>),
    Output(String, bool),
    Plan(Vec<(String, String)>, PathBuf),
    Patch(Vec<String>),
    Note(String),
    Failed(String),
    Done(String, Option<String>, Option<u64>, Option<f64>, Option<u8>),
    Table(Vec<(String, String)>),
    Beside(Vec<String>),
    Help,
    Line(String),
    Steered(String),
    Pairing(String, crate::relay::Reach),
    Typed(bool, String),
    Stepped,
}

fn replaying() -> bool {
    SINK.lock().is_ok_and(|sink| sink.is_some())
}

fn keep(entry: Entry) {
    if replaying() {
        return;
    }
    if let Ok(mut story) = STORY.lock() {
        story.push(entry);
        if story.len() > KEPT {
            let extra = story.len() - KEPT;
            story.drain(..extra);
        }
    }
}

pub fn tee() {
    let kept = STORY.lock().map_or(0, |story| story.len());
    TEE.store(kept, Ordering::Relaxed);
}

pub fn teeing() -> bool {
    TEE.load(Ordering::Relaxed) != usize::MAX
}

pub fn untee() -> Option<String> {
    let mark = TEE.swap(usize::MAX, Ordering::Relaxed);
    let added: Vec<Entry> = STORY
        .lock()
        .ok()?
        .iter()
        .skip(mark)
        .filter(|entry| !matches!(entry, Entry::Typed(..)))
        .cloned()
        .collect();
    if let Ok(mut sink) = SINK.lock() {
        *sink = Some(Vec::new());
    }
    NARROW.store(PHONE, Ordering::Relaxed);
    for entry in added {
        show(entry);
    }
    NARROW.store(0, Ordering::Relaxed);
    let lines: Vec<String> = SINK
        .lock()
        .ok()
        .and_then(|mut sink| sink.take())?
        .iter()
        .map(|line| crate::dock::bare(line))
        .collect();
    let shared = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    let text = lines
        .iter()
        .map(|line| line.get(shared..).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim_matches('\n');
    (!text.trim().is_empty()).then(|| text.to_string())
}

fn put(line: &str) {
    if let Ok(mut sink) = SINK.lock()
        && let Some(sink) = sink.as_mut()
    {
        sink.push(line.to_string());
        return;
    }
    if line.is_empty() && crate::pane::pinned() {
        BLANKS.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let blanks = BLANKS.swap(0, Ordering::Relaxed);
    let mut added = vec![String::new(); blanks];
    added.push(line.to_string());
    let rows = record(LIVE.load(Ordering::Relaxed), &added);
    if !crate::pane::following() {
        LIVE.store(0, Ordering::Relaxed);
        crate::pane::grew(rows);
        return;
    }
    crate::pane::leave();
    print!("{}{}", unlive(), "\n".repeat(blanks));
    println!("{line}");
}

fn record(replaced: usize, added: &[String]) -> usize {
    let columns = width();
    let Ok(mut lines) = LINES.lock() else {
        return 0;
    };
    let keep = lines.len().saturating_sub(replaced);
    lines.truncate(keep);
    lines.extend(added.iter().cloned());
    if lines.len() > HELD {
        let extra = lines.len() - HELD;
        lines.drain(..extra);
    }
    added
        .iter()
        .map(|line| crate::dock::visible(line).div_ceil(columns.max(1)).max(1))
        .sum()
}

pub fn chop(styled: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut rows = vec![String::new()];
    let mut used = 0;
    let mut characters = styled.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            let Some(current) = rows.last_mut() else {
                break;
            };
            current.push(character);
            if let Some(kind) = characters.next() {
                current.push(kind);
                if kind == ']' {
                    while let Some(code) = characters.next() {
                        current.push(code);
                        if code == '\u{7}' {
                            break;
                        }
                        if code == '\u{1b}' && characters.peek() == Some(&'\\') {
                            current.extend(characters.next());
                            break;
                        }
                    }
                } else if kind == '[' {
                    for code in characters.by_ref() {
                        current.push(code);
                        if ('@'..='~').contains(&code) {
                            break;
                        }
                    }
                }
            }
            continue;
        }
        let size = character.width().unwrap_or(0);
        if used + size > columns {
            rows.push(String::new());
            used = 0;
        }
        used += size;
        if let Some(current) = rows.last_mut() {
            current.push(character);
        }
    }
    rows
}

pub fn rows(want: usize, columns: usize) -> usize {
    LINES.lock().map_or(0, |lines| {
        let mut total = 0;
        for line in lines.iter().rev() {
            total += crate::dock::visible(line).div_ceil(columns.max(1)).max(1);
            if total >= want {
                break;
            }
        }
        total
    })
}

pub fn window(offset: usize, room: usize, columns: usize) -> Vec<String> {
    LINES
        .lock()
        .map(|lines| windowed(&lines, offset, room, columns))
        .unwrap_or_default()
}

fn windowed(lines: &[String], offset: usize, room: usize, columns: usize) -> Vec<String> {
    let want = offset + room;
    let mut rows: Vec<String> = Vec::new();
    for line in lines.iter().rev() {
        let mut chopped = chop(line, columns);
        chopped.reverse();
        rows.extend(chopped);
        if rows.len() >= want {
            break;
        }
    }
    rows.into_iter().skip(offset).take(room).rev().collect()
}

pub fn line(text: &str) {
    keep(Entry::Line(text.to_string()));
    put(text);
}

pub fn recent(count: usize) -> Vec<serde_json::Value> {
    let mut all = EARLIER
        .lock()
        .map(|earlier| earlier.clone())
        .unwrap_or_default();
    all.extend(STORY.lock().map(|story| story.clone()).unwrap_or_default());
    shared(all, count)
}

fn shared(story: Vec<Entry>, count: usize) -> Vec<serde_json::Value> {
    use serde_json::json;
    let shown: Vec<serde_json::Value> = story
        .into_iter()
        .filter_map(|entry| match entry {
Entry::Typed(false, text) if text.starts_with('/') => None,
            Entry::Said(text) | Entry::Typed(false, text) => Some(json!({"kind": "user", "text": text})),
            Entry::Typed(true, text) => Some(json!({"kind": "step", "label": "Shell", "detail": text})),
            Entry::Text(text) => Some(json!({"kind": "text", "text": text})),
            Entry::Step(label, detail, _) => Some(json!({"kind": "step", "label": label, "detail": detail})),
            Entry::Tally(counts) => Some(json!({
                "kind": "step",
                "label": counts.iter().map(|(label, count)| format!("{label} ×{count}")).collect::<Vec<_>>().join("  "),
                "detail": "",
            })),
            Entry::Output(text, true) => Some(json!({"kind": "output", "text": text})),
            Entry::Note(text) => Some(json!({"kind": "note", "text": text})),
            Entry::Failed(text) => Some(json!({"kind": "note", "text": text})),
            Entry::Done(agent, model, millis, cost, left) => Some(json!({
                "agent": agent,
                "cost": cost,
                "kind": "done",
                "left": left,
                "millis": millis,
                "model": model,
            })),
            _ => None,
        })
        .collect();
    let skip = shown.len().saturating_sub(count);
    shown.into_iter().skip(skip).collect()
}

pub fn link(text: &str, url: &str) -> String {
    format!("\u{1b}]8;;{url}\u{1b}\\{text}\u{1b}]8;;\u{1b}\\")
}

pub fn pairing(url: &str, reach: crate::relay::Reach) {
    keep(Entry::Pairing(url.to_string(), reach));
    gap();
    let room = width().saturating_sub(4).max(10);
    let code = qrcode::QrCode::new(url.as_bytes()).map(|code| {
        code.render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .dark_color(qrcode::render::unicode::Dense1x2::Light)
            .light_color(qrcode::render::unicode::Dense1x2::Dark)
            .build()
    });
    let shown = code
        .ok()
        .filter(|code| code.lines().all(|row| cells(row) <= room));
    if let Some(code) = &shown {
        for row in code.lines() {
            put(&format!("  {row}"));
        }
        put("");
    }
    let lead = if shown.is_some() {
        "scan this with your phone's camera"
    } else {
        "open this link on your phone. a wider window shows a code to scan"
    };
    for row in wrap(lead, room) {
        put(&format!("  {}", bold(&row)));
    }
    let about = match reach {
        crate::relay::Reach::Hosted => {
            "it opens this chat in your phone's browser, from anywhere. sombre's relay only passes sealed messages"
        }
        crate::relay::Reach::Relay => {
            "it opens this chat in your phone's browser, from anywhere, through your relay"
        }
        crate::relay::Reach::Tailnet => {
            "it opens this chat in your phone's browser, from anywhere, while tailscale is on on both"
        }
        crate::relay::Reach::Wifi => {
            "it opens this chat in your phone's browser. you both need the same wi-fi"
        }
    };
    for text in [
        about,
        "the link holds the key, so only that phone can read the chat. it works once. /pair list shows paired phones",
    ] {
        for row in wrap(text, room) {
            put(&format!("  {}", dim(&row)));
        }
    }
    let characters: Vec<char> = url.chars().collect();
    for piece in characters.chunks(room) {
        put(&format!(
            "  {}",
            dim(&link(&piece.iter().collect::<String>(), url))
        ));
    }
    put("");
}

pub fn steered(text: &str) {
    keep(Entry::Steered(text.to_string()));
    for row in steering(text, width()) {
        put(&row);
    }
}

fn steering(text: &str, columns: usize) -> Vec<String> {
    let room = columns.saturating_sub(6).max(10);
    crate::compose::rows(text, room)
        .into_iter()
        .enumerate()
        .map(|(index, range)| {
            let lead = if index == 0 {
                ember("↳")
            } else {
                " ".to_string()
            };
            format!("  {lead} {}", &text[range]).trim_end().to_string()
        })
        .collect()
}

pub fn typed(shell: bool, text: &str) {
    keep(Entry::Typed(shell, text.to_string()));
    for line in crate::compose::lines(text, shell, None, width()) {
        put(&line);
    }
}

pub fn resize() {
    RESIZED.store(true, Ordering::SeqCst);
}

pub fn resized() -> bool {
    RESIZED.swap(false, Ordering::SeqCst)
}

fn show(entry: Entry) {
    match entry {
        Entry::Said(body) => said(&body),
        Entry::Text(body) => text(&body),
        Entry::Step(label, detail, cwd) => step(&label, &detail, &cwd),
        Entry::Tally(counts) => tally(&counts),
        Entry::Output(body, failed) => output(&body, failed),
        Entry::Plan(steps, cwd) => plan(&steps, &cwd),
        Entry::Patch(lines) => patch(&lines),
        Entry::Note(message) => note(&message),
        Entry::Failed(message) => failed(&message),
        Entry::Done(agent, model, millis, cost, left) => {
            done(&agent, model.as_deref(), millis, cost, left);
        }
        Entry::Table(rows) => {
            let rows: Vec<(&str, String)> = rows
                .iter()
                .map(|(label, value)| (label.as_str(), value.clone()))
                .collect();
            table(&rows);
        }
        Entry::Beside(side) => beside(&side),
        Entry::Help => help(),
        Entry::Line(text) => put(&text),
        Entry::Steered(text) => steered(&text),
        Entry::Pairing(url, reach) => pairing(&url, reach),
        Entry::Typed(shell, text) => typed(shell, &text),
        Entry::Stepped => stepped(),
    }
}

pub fn frame(lines: &[String]) -> String {
    LIVE.store(0, Ordering::Relaxed);
    let mut end = lines.len();
    if crate::pane::pinned() {
        end = lines
            .iter()
            .rposition(|line| !line.is_empty())
            .map_or(0, |last| last + 1);
        BLANKS.store(lines.len() - end, Ordering::Relaxed);
    }
    if let Ok(mut held) = LINES.lock() {
        *held = lines[end.saturating_sub(HELD)..end].to_vec();
    }
    let start = end.saturating_sub(ROWS);
    let shown = if crate::pane::inside() {
        String::new()
    } else {
        lines[start..end].join("\n")
    };
    format!(
        "\u{1b}[?2026h{}\u{1b}[H\u{1b}[2J\u{1b}[3J{shown}\u{1b}[?2026l\n",
        crate::pane::reset(),
    )
}

pub fn replay(live: Option<&Prose>) -> String {
    frame(&rendered(live))
}

pub fn transcript() -> Vec<String> {
    let mut lines = rendered(None);
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(0, |last| last + 1);
    lines.truncate(end);
    lines
}

fn rendered(live: Option<&Prose>) -> Vec<String> {
    let _held = hold();
    let story = STORY.lock().map(|story| story.clone()).unwrap_or_default();
    let stepping = STEPPING.load(Ordering::Relaxed);
    if let Ok(mut sink) = SINK.lock() {
        *sink = Some(Vec::new());
    }
    STEPPING.store(false, Ordering::Relaxed);
    for entry in story {
        show(entry);
    }
    if let Some(live) = live {
        let mut again = Prose::default();
        again.push(live.emitted());
    }
    STEPPING.store(stepping, Ordering::Relaxed);
    SINK.lock()
        .ok()
        .and_then(|mut sink| sink.take())
        .unwrap_or_default()
}

pub fn gap() {
    if STEPPING.swap(false, Ordering::Relaxed) {
        put("");
    }
}

fn plain() -> bool {
    static PLAIN: OnceLock<bool> = OnceLock::new();
    *PLAIN.get_or_init(|| env::var_os("NO_COLOR").is_some() || !ansi())
}

#[cfg(windows)]
fn ansi() -> bool {
    crossterm::ansi_support::supports_ansi()
}

#[cfg(not(windows))]
const fn ansi() -> bool {
    true
}

pub fn ember(text: &str) -> String {
    if plain() {
        text.to_string()
    } else {
        text.with(EMBER).to_string()
    }
}

pub fn dim(text: &str) -> String {
    if plain() {
        text.to_string()
    } else {
        text.attribute(Attribute::Dim).to_string()
    }
}

pub fn caret(text: &str) -> String {
    if plain() {
        text.to_string()
    } else {
        text.attribute(Attribute::Reverse).to_string()
    }
}

pub fn bold(text: &str) -> String {
    if plain() {
        text.to_string()
    } else {
        text.attribute(Attribute::Bold).to_string()
    }
}

pub fn width() -> usize {
    match (
        NARROW.load(Ordering::Relaxed),
        COLUMNS.load(Ordering::Relaxed),
    ) {
        (0, 0) => measured(),
        (0, held) => held,
        (narrow, _) => narrow,
    }
}

pub fn measured() -> usize {
    terminal::size().map_or(80, |(columns, _)| usize::from(columns))
}

struct Hold(usize);

impl Drop for Hold {
    fn drop(&mut self) {
        COLUMNS.store(self.0, Ordering::Relaxed);
    }
}

fn hold() -> Hold {
    let before = COLUMNS.load(Ordering::Relaxed);
    if before == 0 {
        COLUMNS.store(measured(), Ordering::Relaxed);
    }
    Hold(before)
}

fn breadth() -> usize {
    width().saturating_sub(4).max(20)
}

pub fn cells(text: &str) -> usize {
    text.width()
}

pub fn fit(text: &str, room: usize) -> String {
    if text.width() <= room {
        return text.to_string();
    }
    let mut used = 0;
    let mut kept: String = text
        .chars()
        .take_while(|character| {
            used += character.width().unwrap_or(0);
            used < room
        })
        .collect();
    kept.push('…');
    kept
}

pub fn pad(text: &str, room: usize) -> String {
    let fitted = fit(text, room);
    let gap = room.saturating_sub(fitted.width());
    format!("{fitted}{}", " ".repeat(gap))
}

pub fn cut<'a>(lines: &[&'a str], keep: usize, tail: bool) -> (Vec<&'a str>, usize) {
    if lines.len() <= keep {
        return (lines.to_vec(), 0);
    }
    let hidden = lines.len() - keep;
    let shown = if tail {
        lines[hidden..].to_vec()
    } else {
        lines[..keep].to_vec()
    };
    (shown, hidden)
}

fn more(hidden: usize) -> String {
    dim(&format!(
        "… +{hidden} {}",
        if hidden == 1 { "line" } else { "lines" }
    ))
}

pub fn wipe() {
    STEPPING.store(false, Ordering::Relaxed);
    if let Ok(mut story) = STORY.lock() {
        story.clear();
    }
    BLANKS.store(0, Ordering::Relaxed);
    if let Ok(mut held) = LINES.lock() {
        held.clear();
    }
    if let Ok(mut earlier) = EARLIER.lock() {
        earlier.clear();
    }
    print!("{}", crate::pane::reset());
    let _ = execute!(
        io::stdout(),
        terminal::Clear(ClearType::All),
        terminal::Clear(ClearType::Purge),
        MoveTo(0, 0)
    );
}

pub fn folder(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub fn home(path: &Path) -> String {
    let home = sombre_harness::process::home()
        .map(|home| home.display().to_string())
        .unwrap_or_default();
    let place = path.display().to_string();
    if !home.is_empty() && place.starts_with(&home) {
        format!("~{}", &place[home.len()..])
    } else {
        place
    }
}

pub fn money(cost: f64) -> String {
    if cost > 0.0 && cost < 0.01 {
        "<$0.01".to_string()
    } else {
        format!("${cost:.2}")
    }
}

pub fn table(rows: &[(&str, String)]) {
    keep(Entry::Table(
        rows.iter()
            .map(|(label, value)| ((*label).to_string(), value.clone()))
            .collect(),
    ));
    let rows: Vec<(&str, &str)> = rows
        .iter()
        .map(|(label, value)| (*label, value.as_str()))
        .collect();
    pairs(&rows, dim);
    put("");
}

pub fn notify(message: &str) {
    if plain() {
        print!("\x07");
    } else {
        print!("\x1b]9;sombre: {message}\x07");
    }
    let _ = io::stdout().flush();
}

pub fn wrap(text: &str, room: usize) -> Vec<String> {
    let options =
        textwrap::Options::new(room.max(10)).word_separator(textwrap::WordSeparator::AsciiSpace);
    textwrap::wrap(text, options)
        .into_iter()
        .map(|line| line.into_owned())
        .collect()
}

fn pairs(rows: &[(&str, &str)], lead: fn(&str) -> String) {
    let room = width().saturating_sub(4).max(20);
    let left = rows.iter().map(|(key, _)| key.width()).max().unwrap_or(0);
    let stacked = room < left + 2 + 24;
    for (key, about) in rows {
        if stacked {
            put(&format!("  {}", lead(key)));
            for line in wrap(about, room.saturating_sub(2)) {
                put(&format!("    {}", dim(&line)));
            }
            continue;
        }
        for (index, line) in wrap(about, room - left - 2).iter().enumerate() {
            let key = if index == 0 {
                lead(&pad(key, left))
            } else {
                " ".repeat(left)
            };
            put(&format!("  {key}  {}", dim(line)));
        }
    }
}

pub fn clip(styled: &str, room: usize) -> String {
    if crate::dock::visible(styled) <= room {
        return styled.to_string();
    }
    let mut kept = String::new();
    let mut used = 0;
    let mut characters = styled.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            kept.push(character);
            for code in characters.by_ref() {
                kept.push(code);
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        let size = character.width().unwrap_or(0);
        if used + size >= room {
            break;
        }
        used += size;
        kept.push(character);
    }
    format!("{kept}…\u{1b}[0m")
}

pub fn beside(side: &[String]) {
    keep(Entry::Beside(side.to_vec()));
    put("");
    let room = width().saturating_sub(16).max(10);
    for (line, text) in mark::lines().iter().zip(side.iter()) {
        put(&format!("  {}   {}", ember(line), clip(text, room)));
    }
    put("");
}

pub fn version() {
    let number = env!("CARGO_PKG_VERSION");
    if !io::stdout().is_terminal() {
        put(&format!("sombre {number}"));
        return;
    }
    let arch = match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    beside(&[
        format!("{} {}", bold("sombre"), dim(number)),
        dim(&format!("{} {arch}", env::consts::OS)),
        dim("sombre.ai"),
    ]);
}

pub fn banner(agent: &str, model: Option<&str>, mode: &str, cwd: &Path) {
    let place = home(cwd);
    let mode = match mode {
        "ask" => String::new(),
        "yolo" => format!("  {}", bold("yolo")),
        other => format!("  {}", dim(other)),
    };
    let side = [
        bold("sombre"),
        match model {
            Some(model) => format!("{agent}  {}{mode}", dim(&format!("{model}  in {place}"))),
            None => format!("{agent} {}{mode}", dim(&format!("in {place}"))),
        },
        dim("/resume  /new  /provider  /model  /help  /exit"),
    ];
    beside(&side);
}

pub fn history(items: &[Item], cwd: &Path) {
    let _held = hold();
    let prompts: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, Item::Prompt(_)))
        .map(|(index, _)| index)
        .collect();
    let start = prompts
        .len()
        .checked_sub(RECENT)
        .and_then(|skip| prompts.get(skip))
        .copied()
        .unwrap_or(0);
    if let Ok(mut earlier) = EARLIER.lock() {
        *earlier = items[..start]
            .iter()
            .cloned()
            .map(Item::scrubbed)
            .filter_map(|item| match item {
                Item::Prompt(body) => Some(Entry::Said(body)),
                Item::Text(body) => Some(Entry::Text(body)),
                Item::Step { label, detail, .. } => {
                    Some(Entry::Step(label, detail, cwd.to_path_buf()))
                }
                _ => None,
            })
            .collect();
    }
    if start > 0 {
        note(&format!(
            "earlier messages hidden, showing the last {RECENT}"
        ));
    }
    for item in items[start..].iter().cloned().map(Item::scrubbed) {
        match item {
            Item::Prompt(body) => said(&body),
            Item::Text(body) => text(&body),
            Item::Step { label, detail, .. } => step(&label, &detail, cwd),
            _ => {}
        }
    }
    gap();
}

fn said(body: &str) {
    keep(Entry::Said(body.to_string()));
    gap();
    let limit = breadth();
    let lines: Vec<String> = body
        .lines()
        .flat_map(|line| textwrap::wrap(line, limit))
        .map(|line| line.into_owned())
        .collect();
    for (index, line) in lines.iter().take(6).enumerate() {
        let lead = if index == 0 {
            ember("❯")
        } else {
            " ".to_string()
        };
        put(&format!("{lead} {line}"));
    }
    if lines.len() > 6 {
        put(&format!("  {}", dim("…")));
    }
    put("");
}

pub fn text(body: &str) {
    let mut prose = Prose::default();
    prose.push(body);
    prose.finish();
}

#[derive(Default)]
pub struct Prose {
    reader: Reader,
    partial: String,
    source: String,
    started: bool,
}

impl Prose {
    pub fn push(&mut self, chunk: &str) {
        if !self.started {
            gap();
            self.started = true;
        }
        self.partial.push_str(chunk);
        self.source.push_str(chunk);
        while let Some(end) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=end).collect();
            self.emit(line.trim_end_matches('\n'));
        }
    }

    pub fn finish(&mut self) {
        if !self.partial.is_empty() {
            let line = std::mem::take(&mut self.partial);
            self.emit(&line);
        }
        if self.started {
            put("");
            keep(Entry::Text(std::mem::take(&mut self.source)));
        }
        *self = Self::default();
    }

    pub fn emitted(&self) -> &str {
        &self.source[..self.source.len() - self.partial.len()]
    }

    fn emit(&mut self, line: &str) {
        let limit = breadth();
        let wrap = |lead: &str, rest: &str, body: &str| {
            let options = textwrap::Options::new(limit + 2)
                .initial_indent(lead)
                .subsequent_indent(rest);
            for line in textwrap::wrap(body, options) {
                put(&line);
            }
        };
        match self.reader.line(line) {
            Block::Heading(spans) => wrap("  ", "  ", &bold(&paint(&spans))),
            Block::Bullet(marker, spans) => {
                let lead = format!("  {marker} ");
                let rest = " ".repeat(lead.width());
                wrap(&lead, &rest, &paint(&spans));
            }
            Block::Quote(spans) => {
                let lead = format!("  {} ", dim("│"));
                wrap(&lead, &lead, &dim(&paint(&spans)));
            }
            Block::Fence(_) => {}
            Block::Code(code) => {
                put(&format!(
                    "  {} {}",
                    dim("▏"),
                    fit(&code, limit.saturating_sub(2))
                ));
            }
            Block::Rule => put(&format!("  {}", dim(&"─".repeat(limit.min(40))))),
            Block::Blank => put(""),
            Block::Text(spans) => wrap("  ", "  ", &paint(&spans)),
        }
    }
}

fn italic(text: &str) -> String {
    if plain() {
        text.to_string()
    } else {
        text.attribute(Attribute::Italic).to_string()
    }
}

fn paint(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|span| match span {
            Span::Plain(text) => text.clone(),
            Span::Strong(text) | Span::Code(text) => bold(text),
            Span::Emphasis(text) => italic(text),
            Span::Link(label, url) if url.starts_with("https://") || url.starts_with("http://") => {
                format!("{label} {}", dim(&format!("({url})")))
            }
            Span::Link(label, _) => label.clone(),
        })
        .collect()
}

pub fn relative(text: &str, cwd: &Path) -> String {
    let root = format!("{}/", cwd.display());
    text.replace(&root, "")
}

pub fn row(label: &str, detail: &str, cwd: &Path) -> String {
    let detail = relative(detail, cwd);
    let detail = detail.lines().next().unwrap_or_default();
    let column = LABELS
        .iter()
        .map(|known| known.width())
        .chain([label.width()])
        .max()
        .unwrap_or_default();
    let room = width().saturating_sub(column + 8).max(10);
    format!(
        "  {} {}  {}",
        ember("■"),
        bold(&pad(label, column)),
        dim(&fit(detail, room))
    )
}

pub fn rule(label: &str, columns: usize) -> String {
    let room = columns.saturating_sub(1);
    if label.is_empty() {
        return dim(&"─".repeat(room));
    }
    let used = crate::dock::visible(label) + 4;
    format!(
        "{} {label} {}",
        dim("──"),
        dim(&"─".repeat(room.saturating_sub(used)))
    )
}

fn shorten(part: &str, room: usize) -> String {
    if part.width() <= room || !part.contains('/') {
        return fit(part, room);
    }
    let mut used = 1;
    let tail: Vec<char> = part
        .chars()
        .rev()
        .take_while(|character| {
            used += character.width().unwrap_or(0);
            used <= room
        })
        .collect();
    format!("…{}", tail.into_iter().rev().collect::<String>())
}

pub fn footer(parts: &[String], columns: usize) -> String {
    let room = columns.saturating_sub(3);
    let size = |part: &str| crate::dock::visible(part);
    let mut parts = parts.to_vec();
    loop {
        let used =
            parts.iter().map(|part| size(part)).sum::<usize>() + 2 * parts.len().saturating_sub(1);
        let Some((index, widest)) = parts
            .iter()
            .enumerate()
            .filter(|(_, part)| !part.contains('\u{1b}'))
            .max_by_key(|(_, part)| size(part))
            .map(|(index, part)| (index, size(part)))
        else {
            break;
        };
        if used <= room || widest <= 8 {
            break;
        }
        parts[index] = shorten(&parts[index], widest.saturating_sub(used - room).max(8));
    }
    let line = parts.join("  ");
    let line = if size(&line) > room {
        fit(&crate::dock::bare(&line), room)
    } else {
        line
    };
    format!("  {}", dim(&line))
}

pub fn step(label: &str, detail: &str, cwd: &Path) {
    keep(Entry::Step(
        label.to_string(),
        detail.to_string(),
        cwd.to_path_buf(),
    ));
    put(&row(label, detail, cwd));
    STEPPING.store(true, Ordering::Relaxed);
}

pub fn tallied(counts: &[(String, usize)]) -> String {
    let parts: Vec<String> = counts
        .iter()
        .map(|(label, count)| format!("{} {}", bold(label), dim(&format!("×{count}"))))
        .collect();
    format!("  {} {}", ember("■"), parts.join("  "))
}

pub fn tally(counts: &[(String, usize)]) {
    keep(Entry::Tally(counts.to_vec()));
    put(&tallied(counts));
    STEPPING.store(true, Ordering::Relaxed);
}

fn unlive() -> String {
    clearing(LIVE.swap(0, Ordering::Relaxed))
}

fn clearing(count: usize) -> String {
    if count == 0 {
        return String::new();
    }
    format!(
        "\r\u{1b}[{count}A{}\u{1b}[{count}A",
        "\u{1b}[2K\u{1b}[1B".repeat(count)
    )
}

pub fn live(rows: &[String]) {
    if replaying() {
        return;
    }
    let blanks = if LIVE.load(Ordering::Relaxed) == 0 {
        BLANKS.swap(0, Ordering::Relaxed)
    } else {
        0
    };
    let mut added = vec![String::new(); blanks];
    added.extend(rows.iter().cloned());
    let shown = record(LIVE.load(Ordering::Relaxed), &added);
    if !crate::pane::following() {
        LIVE.store(rows.len(), Ordering::Relaxed);
        crate::pane::grew(shown);
        return;
    }
    crate::pane::leave();
    let room = width().saturating_sub(1);
    let mut out = format!("\u{1b}[?2026h{}{}", unlive(), "\n".repeat(blanks));
    for row in rows {
        out.push_str(&clip(row, room));
        out.push_str("\u{1b}[K\n");
    }
    out.push_str("\u{1b}[?2026l");
    LIVE.store(rows.len(), Ordering::Relaxed);
    print!("{out}");
    let _ = io::stdout().flush();
}

pub fn stepped() {
    keep(Entry::Stepped);
    STEPPING.store(true, Ordering::Relaxed);
}

pub fn output(text: &str, failed: bool) {
    keep(Entry::Output(text.to_string(), failed));
    let lines: Vec<&str> = text.lines().collect();
    let (shown, hidden) = cut(&lines, if failed { 4 } else { 3 }, failed);
    let room = width().saturating_sub(8).max(10);
    if shown.is_empty() {
        put(&format!("    {} failed", dim("└")));
    }
    for (index, line) in shown.iter().enumerate() {
        let lead = if index == 0 {
            dim("└")
        } else {
            " ".to_string()
        };
        let body = fit(line, room);
        put(&format!(
            "    {lead} {}",
            if failed { body } else { dim(&body) }
        ));
    }
    if hidden > 0 {
        put(&format!("      {}", more(hidden)));
    }
    STEPPING.store(true, Ordering::Relaxed);
}

pub fn planned(steps: &[(String, String)], cwd: &Path) -> Vec<String> {
    let done = steps
        .iter()
        .filter(|(_, status)| status == "completed")
        .count();
    let mut lines = vec![row("Plan", &format!("{done} of {} done", steps.len()), cwd)];
    let room = width().saturating_sub(10).max(10);
    for (index, (step, status)) in steps.iter().enumerate() {
        let lead = if index == 0 {
            dim("└")
        } else {
            " ".to_string()
        };
        let text = fit(step, room);
        let line = match status.as_str() {
            "completed" => format!("{} {}", ember("✔"), dim(&text)),
            "in_progress" => format!("{} {}", ember("›"), bold(&text)),
            _ => dim(&format!("○ {text}")),
        };
        lines.push(format!("    {lead} {line}"));
    }
    lines
}

pub fn plan(steps: &[(String, String)], cwd: &Path) {
    keep(Entry::Plan(steps.to_vec(), cwd.to_path_buf()));
    for line in planned(steps, cwd) {
        put(&line);
    }
    STEPPING.store(true, Ordering::Relaxed);
}

pub fn patch(lines: &[String]) {
    keep(Entry::Patch(lines.to_vec()));
    let added = lines.iter().filter(|line| line.starts_with('+')).count();
    let removed = lines.iter().filter(|line| line.starts_with('-')).count();
    if added > 0 && added == lines.len() {
        let noun = if added == 1 { "line" } else { "lines" };
        put(&format!(
            "    {} {}",
            dim("└"),
            dim(&format!("added {added} {noun}"))
        ));
        STEPPING.store(true, Ordering::Relaxed);
        return;
    }
    put(&format!(
        "    {} {}",
        dim("└"),
        dim(&format!("+{added} -{removed}"))
    ));
    for line in diff(lines, 8) {
        put(&format!("      {line}"));
    }
    STEPPING.store(true, Ordering::Relaxed);
}

pub fn diff(lines: &[String], keep: usize) -> Vec<String> {
    let all: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (shown, hidden) = cut(&all, keep, false);
    let room = width().saturating_sub(12).max(10);
    let mut styled: Vec<String> = shown
        .iter()
        .map(|line| match line.strip_prefix('+') {
            Some(rest) => format!("{} {}", ember("+"), fit(rest, room)),
            None => dim(&format!(
                "- {}",
                fit(line.get(1..).unwrap_or_default(), room)
            )),
        })
        .collect();
    if hidden > 0 {
        styled.push(more(hidden));
    }
    styled
}

pub fn note(message: &str) {
    keep(Entry::Note(message.to_string()));
    gap();
    for line in wrap(message, width().saturating_sub(4)) {
        put(&format!("  {}", dim(&line)));
    }
    put("");
}

pub fn failed(message: &str) {
    keep(Entry::Failed(message.to_string()));
    gap();
    for (index, line) in wrap(message, width().saturating_sub(6)).iter().enumerate() {
        let lead = if index == 0 {
            bold("✕")
        } else {
            " ".to_string()
        };
        put(&format!("  {lead} {line}"));
    }
    put("");
}

pub fn done(
    agent: &str,
    model: Option<&str>,
    millis: Option<u64>,
    cost: Option<f64>,
    left: Option<u8>,
) {
    keep(Entry::Done(
        agent.to_string(),
        model.map(str::to_string),
        millis,
        cost,
        left,
    ));
    gap();
    let mut parts = vec![agent.to_string()];
    parts.extend(model.map(str::to_string));
    if let Some(millis) = millis {
        parts.push(format!("{:.1}s", millis as f64 / 1000.0));
    }
    if let Some(cost) = cost.filter(|cost| *cost > 0.0) {
        parts.push(money(cost));
    }
    if let Some(left) = left {
        parts.push(format!("{left}% context left"));
    }
    for line in packed(&parts, width().saturating_sub(3)) {
        put(&format!("  {}", dim(&line)));
    }
    put("");
}

fn packed(parts: &[String], room: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for part in parts {
        match lines.last_mut() {
            Some(line) if line.width() + 2 + part.width() <= room => {
                line.push_str("  ");
                line.push_str(part);
            }
            _ => lines.push(fit(part, room)),
        }
    }
    lines
}

pub fn spin(frame: usize, seconds: u64) {
    let glyph = SPINNER[frame % SPINNER.len()].to_string();
    print!(
        "\r  {} {}",
        ember(&glyph),
        dim(&format!("working  {seconds}s  ctrl-c to stop"))
    );
    let _ = io::stdout().flush();
}

pub fn clear() {
    print!("\r\x1b[2K");
    let _ = io::stdout().flush();
}

pub fn help() {
    keep(Entry::Help);
    let rows = [
        ("/resume", "pick a chat from this folder, from any agent"),
        ("/resume all", "pick a chat from any folder"),
        ("/status", "agent, model, permissions, context, and cost"),
        ("/compact", "summarize the chat to free up context"),
        ("/skills", "skills sombre's agent can use"),
        (
            "/mcp",
            "mcp servers for sombre's agent, and add ones from other agents",
        ),
        (
            "/settings",
            "agent, model, permissions, effort, queue or steer, phone relay, publishing, mcp",
        ),
        (
            "/device",
            "keep this computer awake, even with the lid closed",
        ),
        (
            "/goal <done when>",
            "keep working until it's done. /goal shows progress",
        ),
        ("/new", "start a fresh chat"),
        (
            "/pair",
            "continue this chat from your phone. /pair list, /pair forget <n>, /pair off",
        ),
        (
            "/rename <name>",
            "name this chat. the name shows in /resume, ls, and the agent's own list",
        ),
        ("/model", "pick the model for this agent"),
        ("/model <name>", "switch straight to a model"),
        ("/agent claude", "switch to claude code"),
        ("/agent codex", "switch to codex"),
        ("/agent sombre", "switch to sombre's own agent"),
        (
            "/provider",
            "switch between claude code, codex, ai gateway, or your own",
        ),
        (
            "/permissions",
            "ask, auto, or yolo. shift tab switches between them",
        ),
        ("/effort", "how hard the model thinks, from minimal to max"),
        (
            "!command",
            "run a shell command here. the agent sees the output next turn",
        ),
        ("/exit", "quit, or press ctrl-c twice"),
    ];
    pairs(&rows, ember);
    put("");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_links_show_by_name_and_web_links_keep_their_address() {
        let local = paint(&[Span::Link(
            "notes.md".to_string(),
            "/work/notes.md".to_string(),
        )]);
        assert_eq!(sombre_core::scrub::scrub(&local), "notes.md");
        let web = paint(&[Span::Link(
            "docs".to_string(),
            "https://sombre.ai/docs".to_string(),
        )]);
        assert_eq!(
            sombre_core::scrub::scrub(&web),
            "docs (https://sombre.ai/docs)"
        );
    }

    #[test]
    fn steered_messages_wrap_under_their_marker() {
        let rows: Vec<String> = steering("first line here\n\nsecond one", 20)
            .iter()
            .map(|row| sombre_core::scrub::scrub(row))
            .collect();
        assert_eq!(rows, ["  ↳ first line", "    here", "", "    second one"]);
    }

    #[test]
    fn wraps_at_spaces_so_commands_stay_whole() {
        assert_eq!(
            wrap("it works once. /pair list shows paired phones", 20),
            ["it works once. /pair", "list shows paired", "phones"]
        );
        assert_eq!(
            wrap("run it with /pair forget <number> later", 16),
            ["run it with", "/pair forget", "<number> later"]
        );
    }

    #[test]
    fn chops_styled_lines_into_rows_that_fit() {
        let line = format!("{}cdefg", ember("ab"));
        let rows: Vec<String> = chop(&line, 3)
            .iter()
            .map(|row| crate::dock::bare(row))
            .collect();
        assert_eq!(rows, ["abc", "def", "g"]);
        assert_eq!(chop("", 5), [""]);
        assert_eq!(chop("日本語", 4).len(), 2);
    }

    #[test]
    fn windows_count_rows_up_from_the_latest_line() {
        let lines: Vec<String> = ["one", "two", "three", "abcdefgh"]
            .map(str::to_string)
            .to_vec();
        assert_eq!(windowed(&lines, 0, 3, 5), ["three", "abcde", "fgh"]);
        assert_eq!(windowed(&lines, 2, 2, 5), ["two", "three"]);
        assert_eq!(windowed(&lines, 0, 10, 80).len(), 4);
        assert!(windowed(&lines, 9, 3, 80).is_empty());
    }

    #[test]
    fn clearing_a_live_row_returns_to_where_it_began() {
        assert_eq!(clearing(0), "");
        assert_eq!(
            clearing(2),
            "\r\u{1b}[2A\u{1b}[2K\u{1b}[1B\u{1b}[2K\u{1b}[1B\u{1b}[2A"
        );
    }

    #[test]
    fn summaries_wrap_between_parts_and_never_inside_one() {
        let parts: Vec<String> = [
            "claude code",
            "claude-opus-5-5",
            "5.7s",
            "$0.24",
            "97% context left",
        ]
        .map(str::to_string)
        .to_vec();
        assert_eq!(
            packed(&parts, 42),
            [
                "claude code  claude-opus-5-5  5.7s  $0.24",
                "97% context left"
            ]
        );
        assert_eq!(packed(&parts, 200).len(), 1);
        assert_eq!(packed(&parts[..1], 6), ["claud…"]);
    }

    #[test]
    fn footers_shrink_the_widest_part_and_keep_the_folder_name() {
        let parts = [
            "sombre".to_string(),
            "abliteration:abliterated-model-large-v2".to_string(),
            "yolo".to_string(),
            "~/Desktop/sombre".to_string(),
        ];
        let wide = footer(&parts, 100);
        assert!(wide.contains("abliterated-model-large-v2  yolo  ~/Desktop/sombre"));
        let narrow = footer(&parts, 45);
        assert!(crate::dock::visible(&narrow) <= 44, "{narrow}");
        assert!(narrow.contains("~/Desktop/sombre") && narrow.contains("sombre  abliter"));
        let tiny = footer(&parts, 30);
        assert!(crate::dock::visible(&tiny) <= 29, "{tiny}");
        assert!(tiny.contains("sombre") && tiny.contains('…'));
    }

    #[test]
    fn shares_recent_messages_with_a_new_phone() {
        let story = vec![
            Entry::Typed(false, "fix the test".to_string()),
            Entry::Typed(false, "/pair".to_string()),
            Entry::Step(
                "Run".to_string(),
                "cargo test".to_string(),
                PathBuf::from("/w"),
            ),
            Entry::Help,
            Entry::Text("fixed".to_string()),
        ];
        let kinds: Vec<String> = shared(story, 3)
            .iter()
            .filter_map(|item| item["kind"].as_str().map(str::to_string))
            .collect();
        assert_eq!(kinds, ["user", "step", "text"]);
    }

    #[test]
    fn replays_what_was_shown_into_one_frame() {
        let lines: Vec<String> = (0..ROWS + 5).map(|index| index.to_string()).collect();
        let frame = frame(&lines);
        assert!(frame.starts_with("\u{1b}[?2026h\u{1b}[H\u{1b}[2J\u{1b}[3J5\n"));
        assert!(frame.ends_with(&format!("{}\u{1b}[?2026l\n", ROWS + 4)));
    }

    #[test]
    fn remembers_how_far_a_streaming_reply_got() {
        let mut prose = Prose {
            started: true,
            ..Prose::default()
        };
        prose.partial.push_str("half");
        prose.source.push_str("first line\nhalf");
        assert_eq!(prose.emitted(), "first line\n");
    }

    #[test]
    fn cut_keeps_the_head_by_default() {
        assert_eq!(
            cut(&["1", "2", "3", "4", "5"], 3, false),
            (vec!["1", "2", "3"], 2)
        );
    }

    #[test]
    fn cut_keeps_the_tail_for_failures() {
        assert_eq!(
            cut(&["1", "2", "3", "4", "5"], 2, true),
            (vec!["4", "5"], 3)
        );
    }

    #[test]
    fn cut_leaves_short_output_alone() {
        assert_eq!(cut(&["1", "2"], 3, false), (vec!["1", "2"], 0));
    }

    #[test]
    fn relative_drops_the_project_folder() {
        let cwd = Path::new("/work/atlas");
        assert_eq!(relative("/work/atlas/src/app.ts", cwd), "src/app.ts");
        assert_eq!(relative("rm /work/atlas/a.txt", cwd), "rm a.txt");
        assert_eq!(relative("/work/other/x.ts", cwd), "/work/other/x.ts");
    }

    #[test]
    fn clips_styled_text_without_breaking_its_colors() {
        let styled = format!("{} and more", bold("sombre"));
        let clipped = clip(&styled, 8);
        assert_eq!(crate::dock::visible(&clipped), 8);
        assert!(clipped.ends_with("…\u{1b}[0m"));
        assert_eq!(clip("short", 10), "short");
    }

    #[test]
    fn measures_wide_characters_by_the_columns_they_take() {
        assert_eq!(fit("日本語テキスト", 5), "日本…");
        assert_eq!(cells("日本"), 4);
        assert_eq!(pad("日本", 6), "日本  ");
        assert_eq!(pad("long title", 5), "long…");
    }

    #[test]
    fn fit_trims_long_text_on_a_character_boundary() {
        assert_eq!(fit("héllo world", 6), "héllo…");
        assert_eq!(fit("short", 10), "short");
    }
}

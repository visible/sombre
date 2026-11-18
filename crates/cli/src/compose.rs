use std::io::{self, Write};
use std::ops::Range;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{execute, terminal};
use unicode_width::UnicodeWidthChar;

use crate::dock::paint;
use crate::view;

const LONG: usize = 1_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Key {
    Edited,
    Submit,
    Interrupt,
    Eof,
    Shell(bool),
    Image,
    Refresh,
    Nothing,
}

#[derive(Debug, Default)]
pub struct Composer {
    pub text: String,
    pub cursor: usize,
    pub pastes: Vec<(String, String)>,
    pub history: Vec<String>,
    place: Option<usize>,
    saved: String,
}

fn before(text: &str, at: usize) -> usize {
    text[..at]
        .chars()
        .next_back()
        .map_or(0, |character| at - character.len_utf8())
}

fn after(text: &str, at: usize) -> usize {
    text[at..]
        .chars()
        .next()
        .map_or(text.len(), |character| at + character.len_utf8())
}

fn starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

impl Composer {
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
    }

    pub fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.place = None;
    }

    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text: String = text
            .chars()
            .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
            .collect();
        if text.chars().count() > LONG {
            let lines = text.trim_end().lines().count().max(1);
            let noun = if lines == 1 { "line" } else { "lines" };
            let label = format!("[Pasted text #{}, {lines} {noun}]", self.pastes.len() + 1);
            self.insert(&label);
            self.pastes.push((label, text));
        } else {
            self.insert(&text);
        }
    }

    pub fn expand(&self) -> String {
        self.pastes
            .iter()
            .filter(|(label, _)| self.text.contains(label))
            .fold(self.text.clone(), |text, (label, full)| {
                text.replacen(label, full.trim_end(), 1)
            })
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.pastes.clear();
        self.place = None;
    }

    pub fn remember(&mut self, line: &str) {
        if !line.trim().is_empty() && self.history.last().is_none_or(|last| last != line) {
            self.history.push(line.to_string());
        }
        self.place = None;
    }

    fn word_left(&self) -> usize {
        let head = &self.text[..self.cursor];
        let trimmed = head.trim_end();
        trimmed.rfind(char::is_whitespace).map_or(0, |index| {
            index + trimmed[index..].chars().next().map_or(1, char::len_utf8)
        })
    }

    fn word_right(&self) -> usize {
        let tail = &self.text[self.cursor..];
        let skipped = tail.len() - tail.trim_start().len();
        let rest = &tail[skipped..];
        self.cursor + skipped + rest.find(char::is_whitespace).unwrap_or(rest.len())
    }

    fn line(&self) -> (usize, Vec<usize>) {
        let lines = starts(&self.text);
        let index = lines
            .iter()
            .rposition(|start| *start <= self.cursor)
            .unwrap_or(0);
        (index, lines)
    }

    fn vertical(&mut self, down: bool) -> bool {
        let (index, lines) = self.line();
        let target = if down {
            index + 1
        } else {
            index.wrapping_sub(1)
        };
        let Some(start) = lines.get(target).copied() else {
            return false;
        };
        let column = self.text[lines[index]..self.cursor].chars().count();
        let end = lines
            .get(target + 1)
            .map_or(self.text.len(), |next| next - 1);
        self.cursor = self.text[start..end]
            .char_indices()
            .nth(column)
            .map_or(end, |(offset, _)| start + offset);
        true
    }

    fn browse(&mut self, older: bool) {
        if self.history.is_empty() {
            return;
        }
        let next = match (self.place, older) {
            (None, true) => {
                self.saved = self.text.clone();
                Some(self.history.len() - 1)
            }
            (None, false) => None,
            (Some(place), true) => Some(place.saturating_sub(1)),
            (Some(place), false) if place + 1 < self.history.len() => Some(place + 1),
            (Some(_), false) => {
                let saved = std::mem::take(&mut self.saved);
                self.set(&saved);
                self.place = None;
                return;
            }
        };
        if let Some(place) = next {
            let line = self.history[place].clone();
            self.set(&line);
            self.place = Some(place);
        }
    }

    pub fn press(&mut self, code: KeyCode, modifiers: KeyModifiers, shell: bool) -> Key {
        let control = modifiers.contains(KeyModifiers::CONTROL);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        match code {
            KeyCode::Enter if shift || alt => self.insert("\n"),
            KeyCode::Enter if self.text[..self.cursor].ends_with('\\') => {
                self.cursor -= 1;
                self.text.remove(self.cursor);
                self.insert("\n");
            }
            KeyCode::Char('j') if control => self.insert("\n"),
            KeyCode::Enter => return Key::Submit,
            KeyCode::Char('c') if control => {
                if self.text.is_empty() {
                    return Key::Interrupt;
                }
                self.clear();
            }
            KeyCode::Char('d') if control && self.text.is_empty() => return Key::Eof,
            KeyCode::Char('d') if control => {
                let end = after(&self.text, self.cursor);
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Char('v') if control => return Key::Image,
            KeyCode::Char('l') if control => return Key::Refresh,
            KeyCode::Char('a') if control => self.cursor = self.line().1[self.line().0],
            KeyCode::Home => self.cursor = self.line().1[self.line().0],
            KeyCode::Char('e') if control => self.cursor = self.end(),
            KeyCode::End => self.cursor = self.end(),
            KeyCode::Char('u') if control => {
                let start = self.line().1[self.line().0];
                self.text.replace_range(start..self.cursor, "");
                self.cursor = start;
            }
            KeyCode::Char('k') if control => {
                let end = self.end();
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Char('w') if control => self.delete_word(),
            KeyCode::Backspace if alt || control => self.delete_word(),
            KeyCode::Char('b') if alt => self.cursor = self.word_left(),
            KeyCode::Char('f') if alt => self.cursor = self.word_right(),
            KeyCode::Left if alt || control => self.cursor = self.word_left(),
            KeyCode::Right if alt || control => self.cursor = self.word_right(),
            KeyCode::Left => self.cursor = before(&self.text, self.cursor),
            KeyCode::Right => self.cursor = after(&self.text, self.cursor),
            KeyCode::Up => {
                if !self.vertical(false) {
                    self.browse(true);
                }
            }
            KeyCode::Down => {
                if !self.vertical(true) {
                    self.browse(false);
                }
            }
            KeyCode::Backspace if self.text.is_empty() && shell => return Key::Shell(false),
            KeyCode::Backspace => {
                let start = before(&self.text, self.cursor);
                self.text.replace_range(start..self.cursor, "");
                self.cursor = start;
            }
            KeyCode::Delete => {
                let end = after(&self.text, self.cursor);
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Char('!') if self.text.is_empty() && !shell => return Key::Shell(true),
            KeyCode::Char(character) if !control => {
                let mut buffer = [0; 4];
                self.insert(character.encode_utf8(&mut buffer));
            }
            KeyCode::Tab => return Key::Nothing,
            _ => return Key::Nothing,
        }
        Key::Edited
    }

    fn end(&self) -> usize {
        let (index, lines) = self.line();
        lines
            .get(index + 1)
            .map_or(self.text.len(), |next| next - 1)
    }

    fn delete_word(&mut self) {
        let start = self.word_left();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }
}

pub fn rows(text: &str, room: usize) -> Vec<Range<usize>> {
    let room = room.max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    let mut space: Option<(usize, usize)> = None;
    for (index, character) in text.char_indices() {
        if character == '\n' {
            rows.push(start..index);
            start = index + 1;
            used = 0;
            space = None;
            continue;
        }
        let size = character.width().unwrap_or(0);
        if used + size > room {
            match space.filter(|(at, _)| *at > start) {
                Some((at, before)) => {
                    rows.push(start..at);
                    start = at;
                    used -= before;
                }
                None => {
                    rows.push(start..index);
                    start = index;
                    used = 0;
                }
            }
            space = None;
        }
        used += size;
        if character == ' ' {
            space = Some((index + 1, used));
        }
    }
    rows.push(start..text.len());
    rows
}

pub fn spot(text: &str, rows: &[Range<usize>], cursor: usize) -> (usize, usize) {
    let row = rows
        .iter()
        .rposition(|range| range.start <= cursor)
        .unwrap_or(0);
    let column = text[rows[row].start..cursor.min(rows[row].end).max(rows[row].start)]
        .chars()
        .filter_map(UnicodeWidthChar::width)
        .sum();
    (row, column)
}

fn replay() {
    let _ = terminal::disable_raw_mode();
    print!("{}", view::replay(None));
    let _ = io::stdout().flush();
    let _ = terminal::enable_raw_mode();
}

fn tmux() -> bool {
    static CSI: OnceLock<bool> = OnceLock::new();
    *CSI.get_or_init(|| {
        std::env::var_os("TMUX").is_some()
            && Command::new("tmux")
                .args(["show", "-sv", "extended-keys-format"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "csi-u")
    })
}

#[derive(Debug)]
pub struct Keys {
    tmux: bool,
}

impl Keys {
    pub fn new() -> Self {
        let mut out = io::stdout();
        let _ = execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
        let tmux = tmux();
        if tmux {
            print!("\u{1b}[>4;2m");
            let _ = out.flush();
        }
        Self { tmux }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        let mut out = io::stdout();
        if self.tmux {
            print!("\u{1b}[>4;0m");
        }
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
}

pub fn scrolled(key: event::KeyEvent) -> bool {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::PageUp => crate::pane::page(true),
        KeyCode::PageDown => crate::pane::page(false),
        KeyCode::Home if control => crate::pane::edge(true),
        KeyCode::End if control => crate::pane::edge(false),
        _ => false,
    }
}

pub enum Read {
    Line(String),
    Remote(String, Vec<std::path::PathBuf>),
    Interrupted,
    Cycle,
    Woke,
    Stale,
    Eof,
}

pub fn lines(text: &str, shell: bool, hint: Option<&str>, columns: usize) -> Vec<String> {
    let lead = if shell {
        view::ember("!")
    } else {
        view::ember("┃")
    };
    let room = columns.saturating_sub(3).max(10);
    let found = rows(text, room);
    let mut shown: Vec<String> = found
        .iter()
        .map(|range| format!("{lead} {}", &text[range.clone()]))
        .collect();
    if let (Some(hint), Some(last), Some(range)) = (hint, shown.last_mut(), found.last()) {
        let used = text[range.clone()]
            .chars()
            .filter_map(UnicodeWidthChar::width)
            .sum::<usize>();
        last.push_str(&view::dim(&view::fit(hint, room.saturating_sub(used))));
    }
    shown
}

pub fn marked(
    text: &str,
    cursor: usize,
    hint: Option<&str>,
    columns: usize,
) -> (Vec<String>, usize) {
    let room = columns.saturating_sub(4).max(10);
    let found = rows(text, room);
    let (row, _) = spot(text, &found, cursor);
    let shown = found
        .iter()
        .enumerate()
        .map(|(index, range)| {
            let lead = format!("{} ", view::ember("┃"));
            if index != row {
                return format!("{lead}{}", &text[range.clone()]);
            }
            let at = cursor.clamp(range.start, range.end);
            let under = text[at..range.end].chars().next();
            let ghost = hint
                .filter(|_| under.is_none())
                .map(|hint| view::fit(hint, room.saturating_sub(view::cells(&text[range.clone()]))))
                .filter(|ghost| !ghost.is_empty());
            if let Some(ghost) = ghost {
                let mut letters = ghost.chars();
                let first = letters.next().map(String::from).unwrap_or_default();
                return format!(
                    "{lead}{}{}{}",
                    &text[range.start..at],
                    view::caret(&view::dim(&first)),
                    view::dim(letters.as_str())
                );
            }
            let next = under.map_or(at, |character| at + character.len_utf8());
            format!(
                "{lead}{}{}{}",
                &text[range.start..at],
                view::caret(&under.map_or_else(|| " ".to_string(), String::from)),
                &text[next..range.end]
            )
        })
        .collect();
    (shown, row)
}

pub fn framed(rows: Vec<String>, footer: &[String], columns: usize) -> Vec<String> {
    let mut frame: Vec<String> = crate::pane::behind()
        .map(|label| view::clip(&label, columns.saturating_sub(1)))
        .into_iter()
        .collect();
    frame.extend(rows);
    frame.push(String::new());
    frame.push(view::footer(footer, columns));
    frame
}

pub fn idle(footer: &[String], columns: usize) -> Vec<String> {
    framed(lines("", false, None, columns), footer, columns)
}

pub fn read(
    composer: &mut Composer,
    shell: &mut bool,
    models: &[String],
    footer: &[String],
    grab: &mut dyn FnMut() -> Option<String>,
    cycle: &mut dyn FnMut() -> Option<Vec<String>>,
    woke: &dyn Fn() -> bool,
) -> Read {
    if terminal::enable_raw_mode().is_err() {
        return Read::Eof;
    }
    let mut current = footer.to_vec();
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    let keys = Keys::new();
    let pinned = crate::pane::pinned();
    let mut top = 0;
    let mut drawn = false;
    let mut remote: Option<crate::relay::Arrival> = None;
    let mut cycled = false;
    let mut woken = false;
    let mut stale = false;
    let outcome = loop {
        let columns = view::width();
        let hint = (composer.cursor == composer.text.len() && !composer.text.contains('\n'))
            .then(|| crate::complete::suggest(&composer.text, models))
            .flatten();
        let shown = lines(&composer.text, *shell, hint.as_deref(), columns);
        let found = rows(&composer.text, columns.saturating_sub(3).max(10));
        let (row, column) = spot(&composer.text, &found, composer.cursor);
        let height = usize::from(terminal::size().map_or(24, |(_, rows)| rows))
            .saturating_sub(4)
            .max(1);
        let first = (row + 1).saturating_sub(height);
        let visible: Vec<String> = shown.iter().skip(first).take(height).cloned().collect();
        let count = visible.len();
        let window = framed(visible, &current, columns);
        let at = window.len().saturating_sub(2 + count) + row - first;
        if pinned {
            crate::pane::draw(&window, Some((at, column + 2)));
        } else {
            let up = window.len().saturating_sub(1) - at;
            print!(
                "{}{}\r\u{1b}[{}G\u{1b}[?25h",
                paint(&window, top, drawn, true),
                if up > 0 {
                    format!("\u{1b}[{up}A")
                } else {
                    String::new()
                },
                column + 3
            );
            let _ = io::stdout().flush();
            top = at;
            drawn = true;
        }
        let marker = crate::pane::behind();
        let event = loop {
            if crate::pane::behind() != marker {
                break None;
            }
            if sombre_harness::process::hung() {
                break Some(Event::FocusLost);
            }
            if let Some(arrival) = crate::relay::message() {
                remote = Some(arrival);
                break Some(Event::FocusGained);
            }
            if woke() {
                woken = true;
                break Some(Event::FocusGained);
            }
            if view::resized() {
                replay();
                drawn = false;
                break None;
            }
            if crate::footer::changed() {
                stale = true;
                break Some(Event::FocusLost);
            }
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(event) => break Some(event),
                    Err(_) => break Some(Event::FocusLost),
                },
                Ok(false) => {}
                Err(_) => break Some(Event::FocusLost),
            }
        };
        match event {
            Some(Event::FocusLost) => break None,
            Some(Event::FocusGained) if remote.is_some() => break Some(true),
            Some(Event::FocusGained) if woken => break None,
            Some(Event::Paste(text)) => composer.paste(&text),
            Some(Event::Mouse(mouse)) => {
                crate::pane::mouse(mouse);
            }
            Some(Event::Key(key)) if key.kind != KeyEventKind::Release && scrolled(key) => {}
            Some(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::BackTab {
                    if let Some(next) = cycle() {
                        current = next;
                        continue;
                    }
                    cycled = true;
                    break None;
                }
                if key.code == KeyCode::Tab {
                    if let Some(hint) = &hint {
                        composer.insert(hint);
                    }
                    continue;
                }
                match composer.press(key.code, key.modifiers, *shell) {
                    Key::Submit if event::poll(Duration::from_millis(2)).unwrap_or(false) => {
                        composer.insert("\n");
                    }
                    Key::Submit => break Some(true),
                    Key::Interrupt => break Some(false),
                    Key::Eof => break None,
                    Key::Shell(on) => *shell = on,
                    Key::Image => {
                        if let Some(label) = grab() {
                            composer.insert(&format!("{label} "));
                        }
                    }
                    Key::Refresh => {
                        replay();
                        drawn = false;
                    }
                    Key::Edited | Key::Nothing => {}
                }
            }
            _ => {}
        }
    };
    if pinned {
        if outcome == Some(true) {
            crate::pane::edge(false);
        }
        crate::pane::draw(&idle(&current, view::width()), None);
    } else {
        print!(
            "\r{}\u{1b}[J",
            if top > 0 {
                format!("\u{1b}[{top}A")
            } else {
                String::new()
            }
        );
        let _ = io::stdout().flush();
    }
    drop(keys);
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    let _ = terminal::disable_raw_mode();
    if cycled {
        return Read::Cycle;
    }
    if woken {
        return Read::Woke;
    }
    if stale {
        return Read::Stale;
    }
    if let (Some(true), Some(arrival)) = (outcome, remote) {
        view::typed(false, &arrival.text);
        return Read::Remote(arrival.text, arrival.images);
    }
    match outcome {
        Some(true) => {
            view::typed(*shell, &composer.text);
            let full = composer.expand();
            composer.remember(&if *shell {
                format!("!{full}")
            } else {
                full.clone()
            });
            composer.clear();
            Read::Line(full)
        }
        Some(false) => {
            composer.clear();
            Read::Interrupted
        }
        None => Read::Eof,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> Composer {
        let mut composer = Composer::default();
        composer.set(text);
        composer
    }

    fn press(composer: &mut Composer, code: KeyCode, modifiers: KeyModifiers) -> Key {
        composer.press(code, modifiers, false)
    }

    #[test]
    fn edits_like_a_shell_line() {
        let mut composer = typed("fix the flaky test");
        press(&mut composer, KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(composer.text, "fix the flaky ");
        press(&mut composer, KeyCode::Left, KeyModifiers::ALT);
        assert_eq!(composer.cursor, "fix the ".len());
        press(&mut composer, KeyCode::Char('a'), KeyModifiers::CONTROL);
        press(&mut composer, KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert!(composer.text.is_empty());
        assert_eq!(
            press(&mut composer, KeyCode::Char('d'), KeyModifiers::CONTROL),
            Key::Eof
        );
    }

    #[test]
    fn shift_enter_adds_a_line_and_enter_sends() {
        let mut composer = typed("one");
        press(&mut composer, KeyCode::Enter, KeyModifiers::SHIFT);
        press(&mut composer, KeyCode::Char('t'), KeyModifiers::NONE);
        assert_eq!(composer.text, "one\nt");
        assert_eq!(
            press(&mut composer, KeyCode::Enter, KeyModifiers::NONE),
            Key::Submit
        );
    }

    #[test]
    fn marks_the_caret_on_its_row_without_changing_widths() {
        let (shown, row) = marked("one two", 3, None, 40);
        assert_eq!(row, 0);
        assert!(shown[0].contains("one") && shown[0].ends_with("two"));
        assert_eq!(
            crate::dock::visible(&shown[0]),
            crate::dock::visible("┃ one two")
        );
        let (end, row) = marked("one\ntwo", 7, None, 40);
        assert_eq!(row, 1);
        assert_eq!(
            crate::dock::visible(&end[1]),
            crate::dock::visible("  two ")
        );
    }

    #[test]
    fn shows_the_completion_after_the_caret_while_queuing() {
        let (shown, _) = marked("/mo", 3, Some("del"), 40);
        assert_eq!(
            crate::dock::visible(&shown[0]),
            crate::dock::visible("┃ /model")
        );
        assert!(crate::dock::bare(&shown[0]).ends_with("/model"));
        let (inside, _) = marked("/mo", 1, Some("del"), 40);
        assert!(!crate::dock::bare(&inside[0]).contains("del"));
        let (tight, _) = marked("/mo", 3, Some("del"), 8);
        assert!(crate::dock::visible(&tight[0]) <= 8);
    }

    #[test]
    fn a_backslash_before_enter_adds_a_line_in_any_terminal() {
        let mut composer = typed("first\\");
        assert_eq!(
            press(&mut composer, KeyCode::Enter, KeyModifiers::NONE),
            Key::Edited
        );
        assert_eq!(composer.text, "first\n");
        assert_eq!(composer.cursor, composer.text.len());
        composer.set("ends with a\\ slash");
        assert_eq!(
            press(&mut composer, KeyCode::Enter, KeyModifiers::NONE),
            Key::Submit
        );
    }

    #[test]
    fn long_pastes_collapse_and_expand_on_send() {
        let mut composer = typed("look: ");
        let log = "error at line\n".repeat(100);
        composer.paste(&log);
        assert_eq!(composer.text, "look: [Pasted text #1, 100 lines]");
        assert_eq!(composer.expand(), format!("look: {}", log.trim_end()));
        composer.paste("short\r\npaste");
        assert!(composer.text.ends_with("short\npaste"));
    }

    #[test]
    fn up_moves_between_lines_then_walks_history() {
        let mut composer = Composer::default();
        composer.remember("first");
        composer.remember("second");
        composer.set("a\nbc");
        press(&mut composer, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(composer.cursor, 1);
        press(&mut composer, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(composer.text, "second");
        press(&mut composer, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(composer.text, "first");
        press(&mut composer, KeyCode::Down, KeyModifiers::NONE);
        press(&mut composer, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(composer.text, "a\nbc");
    }

    #[test]
    fn bang_on_an_empty_line_switches_to_the_shell() {
        let mut composer = Composer::default();
        assert_eq!(
            composer.press(KeyCode::Char('!'), KeyModifiers::NONE, false),
            Key::Shell(true)
        );
        assert_eq!(
            composer.press(KeyCode::Backspace, KeyModifiers::NONE, true),
            Key::Shell(false)
        );
        assert_eq!(
            press(&mut composer, KeyCode::Char('c'), KeyModifiers::CONTROL),
            Key::Interrupt
        );
    }

    #[test]
    fn wraps_by_columns_and_finds_the_cursor() {
        let text = "日本語 text\nnext";
        let found = rows(text, 7);
        let shown: Vec<&str> = found.iter().map(|range| &text[range.clone()]).collect();
        assert_eq!(shown, ["日本語 ", "text", "next"]);
        assert_eq!(spot(text, &found, text.len()), (2, 4));
        assert_eq!(spot(text, &found, "日本".len()), (0, 4));
        let words = "word word word end";
        let wrapped: Vec<&str> = rows(words, 10)
            .iter()
            .map(|range| &words[range.clone()])
            .collect();
        assert_eq!(wrapped, ["word word ", "word end"]);
        let long = "abcdefghijkl";
        let split: Vec<&str> = rows(long, 5)
            .iter()
            .map(|range| &long[range.clone()])
            .collect();
        assert_eq!(split, ["abcde", "fghij", "kl"]);
    }
}

use std::io::{self, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossterm::cursor::{MoveToColumn, MoveUp};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{self, ClearType};

use crate::compose::{Composer, Key, Keys, marked};
use crate::view::{dim, ember, fit, width};

const SHOWN: usize = 3;

pub fn bare(line: &str) -> String {
    let mut kept = String::new();
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            kept.push(character);
            continue;
        }
        match characters.next() {
            Some('[') => {
                for code in characters.by_ref() {
                    if ('@'..='~').contains(&code) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(code) = characters.next() {
                    if code == '\u{7}' || (code == '\u{1b}' && characters.next() == Some('\\')) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    kept
}

pub fn visible(line: &str) -> usize {
    bare(line)
        .chars()
        .map(|character| unicode_width::UnicodeWidthChar::width(character).unwrap_or(0))
        .sum()
}

pub fn paint(lines: &[String], up: usize, drawn: bool, raw: bool) -> String {
    let mut frame = String::from("\u{1b}[?2026h\u{1b}[?25l");
    if drawn {
        frame.push('\r');
        if up > 0 {
            frame.push_str(&format!("\u{1b}[{up}A"));
        }
    }
    frame.push_str(&lines.join(if raw { "\u{1b}[K\r\n" } else { "\u{1b}[K\n" }));
    frame.push_str(if raw {
        "\u{1b}[K\u{1b}[J\u{1b}[?2026l"
    } else {
        "\u{1b}[K\u{1b}[J\u{1b}[?25h\u{1b}[?2026l"
    });
    frame
}

fn command(text: &str) -> bool {
    text.starts_with('/') || text.starts_with('!')
}

pub fn rows(widths: &[usize], columns: usize) -> usize {
    let columns = columns.max(1);
    widths
        .iter()
        .map(|width| width.div_ceil(columns).max(1))
        .sum()
}

#[derive(Debug, Default)]
pub struct Draft {
    pub editor: Composer,
    pub queued: Vec<String>,
    pub steer: bool,
    pub images: Vec<std::path::PathBuf>,
    pub models: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Stop,
    Typed,
    Steer(Vec<String>),
    Image,
    Cycle,
    Nothing,
}

const TALLEST: usize = 6;

impl Draft {
    pub fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Action {
        let control = modifiers.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Esc => return Action::Stop,
            KeyCode::Char('c') if control => return Action::Stop,
            KeyCode::Char('v') if control => return Action::Image,
            KeyCode::BackTab => return Action::Cycle,
            KeyCode::Tab => match self.hint() {
                Some(hint) => self.editor.insert(&hint),
                None => return self.send(false),
            },
            KeyCode::Up if self.editor.text.is_empty() => {
                if let Some(last) = self.queued.pop() {
                    self.editor.set(&last);
                }
            }
            _ => match self.editor.press(code, modifiers, false) {
                Key::Submit => return self.send(true),
                Key::Shell(_) => self.editor.insert("!"),
                _ => {}
            },
        }
        Action::Nothing
    }

    fn hint(&self) -> Option<String> {
        let editor = &self.editor;
        (editor.cursor == editor.text.len() && !editor.text.contains('\n'))
            .then(|| crate::complete::suggest(&editor.text, &self.models))
            .flatten()
    }

    fn send(&mut self, enter: bool) -> Action {
        let text = self.editor.expand().trim().to_string();
        self.editor.clear();
        let now = enter == self.steer;
        if text.is_empty() {
            if !now {
                return Action::Nothing;
            }
            let (commands, messages): (Vec<String>, Vec<String>) = std::mem::take(&mut self.queued)
                .into_iter()
                .partition(|queued| command(queued));
            self.queued = commands;
            if messages.is_empty() {
                return Action::Nothing;
            }
            return Action::Steer(messages);
        }
        if now && !command(&text) {
            return Action::Steer(vec![text]);
        }
        self.queued.push(text);
        Action::Nothing
    }

    pub fn lines(&self, since: Duration, columns: usize) -> Vec<String> {
        let room = columns.saturating_sub(6).max(10);
        let status = fit(
            &format!(
                "working {} · esc to stop",
                crate::view::took(u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
            ),
            columns.saturating_sub(4),
        );
        let marker =
            crate::pane::behind().map_or_else(String::new, |marker| format!(" · {marker}"));
        let mut lines = vec![crate::view::clip(
            &format!("{} {}{marker}", crate::view::pulse(since), dim(&status)),
            columns.saturating_sub(1),
        )];
        lines.push(String::new());
        for message in self.queued.iter().take(SHOWN) {
            let message = message.replace('\n', " ");
            lines.push(dim(&fit(&format!("┋ {message}"), room)));
        }
        if self.queued.len() > SHOWN {
            lines.push(dim(&format!("┋ … {} more", self.queued.len() - SHOWN)));
        }
        if self.editor.text.is_empty() {
            let keys = if self.steer {
                "enter sends it now, tab queues it for after"
            } else {
                "enter queues it for after, tab sends it now"
            };
            let now = if self.steer { "enter" } else { "tab" };
            let hint = match self.queued.len() {
                0 => format!("type a message. {keys}"),
                1 => format!("{now} sends it now. ↑ edits it"),
                _ => format!("{now} sends them now. ↑ edits the last"),
            };
            lines.push(format!(
                "{} {}",
                ember("┃"),
                dim(&fit(&hint, room.saturating_sub(2)))
            ));
            return lines;
        }
        let hint = self.hint();
        let (rows, row) = marked(
            &self.editor.text,
            self.editor.cursor,
            hint.as_deref(),
            columns,
        );
        let first = (row + 1).saturating_sub(TALLEST);
        lines.extend(rows.into_iter().skip(first).take(TALLEST));
        lines
    }
}

#[cfg(unix)]
type Saved = nix::sys::termios::Termios;

#[cfg(not(unix))]
type Saved = ();

#[cfg(unix)]
fn quiet() -> Option<Saved> {
    use nix::sys::termios::{self, LocalFlags, SetArg};
    let original = termios::tcgetattr(io::stdin()).ok()?;
    let mut quiet = original.clone();
    quiet
        .local_flags
        .remove(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::IEXTEN);
    termios::tcsetattr(io::stdin(), SetArg::TCSANOW, &quiet).ok()?;
    Some(original)
}

#[cfg(not(unix))]
fn quiet() -> Option<Saved> {
    terminal::enable_raw_mode().ok()
}

#[cfg(unix)]
fn loud(original: &Saved) {
    use nix::sys::termios::{self, SetArg};
    let _ = termios::tcsetattr(io::stdin(), SetArg::TCSANOW, original);
}

#[cfg(not(unix))]
fn loud(_: &Saved) {
    let _ = terminal::disable_raw_mode();
}

static HUSHED: Mutex<Option<Saved>> = Mutex::new(None);

pub struct Hushed;

impl Drop for Hushed {
    fn drop(&mut self) {
        aloud();
    }
}

pub fn hushed() -> Hushed {
    hush();
    Hushed
}

pub fn hush() {
    if cfg!(unix)
        && let Ok(mut held) = HUSHED.lock()
        && held.is_none()
    {
        *held = quiet();
    }
}

pub fn aloud() -> bool {
    let saved = HUSHED.lock().ok().and_then(|mut held| held.take());
    if let Some(saved) = &saved {
        loud(saved);
    }
    saved.is_some()
}

pub struct Dock {
    saved: Option<Saved>,
    footer: Vec<String>,
    tallest: usize,
    keys: Option<Keys>,
    widths: Vec<usize>,
    started: Instant,
    drawn: Instant,
    plan: Vec<String>,
    pub draft: Draft,
}

impl Dock {
    pub fn open(steer: bool, footer: Vec<String>, models: Vec<String>) -> Self {
        let saved = quiet();
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        Self {
            footer,
            tallest: 0,
            keys: Some(Keys::new()),
            draft: Draft {
                steer,
                models,
                ..Draft::default()
            },
            drawn: Instant::now(),
            saved,
            started: Instant::now(),
            plan: Vec::new(),
            widths: Vec::new(),
        }
    }

    fn grab(&self) -> Option<std::path::PathBuf> {
        let root = sombre_core::Roots::detect()?.sombre.join("images");
        std::fs::create_dir_all(&root).ok()?;
        let path = root.join(format!("{}.png", sombre_core::clock::uuid()));
        crate::clipboard::image(&path).ok()?;
        Some(path)
    }

    pub fn track(&mut self, plan: Vec<String>) {
        self.plan = plan;
        self.drawn = Instant::now() - Duration::from_secs(1);
    }

    pub fn wait(&mut self) {
        self.drawn = Instant::now() - Duration::from_secs(1);
        if !crate::pane::pinned() {
            return;
        }
        let columns = width();
        let mut lines = vec![
            format!("• {}", dim("waiting for your answer")),
            String::new(),
            crate::view::footer(&self.footer, columns),
        ];
        self.tallest = self.tallest.max(lines.len());
        let padding = vec![String::new(); self.tallest - lines.len()];
        lines.splice(0..0, padding);
        crate::pane::draw(&lines, None);
    }

    pub fn stale(&mut self) {
        self.drawn = Instant::now() - Duration::from_secs(1);
    }

    pub fn relabel(&mut self, footer: Vec<String>) {
        self.footer = footer;
        self.drawn = Instant::now() - Duration::from_secs(1);
    }

    pub fn keys(&mut self) -> Action {
        let mut typed = false;
        while event::poll(Duration::ZERO).unwrap_or(false) {
            let key = match event::read() {
                Ok(Event::Key(key)) => key,
                Ok(Event::Resize(..)) => {
                    typed = true;
                    continue;
                }
                Ok(Event::Paste(text)) => {
                    self.draft.editor.paste(&text);
                    typed = true;
                    continue;
                }
                Ok(Event::Mouse(mouse)) => {
                    typed |= crate::pane::mouse(mouse);
                    continue;
                }
                Err(_) => return Action::Stop,
                _ => continue,
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if crate::compose::scrolled(key) {
                typed = true;
                continue;
            }
            match self.draft.press(key.code, key.modifiers) {
                Action::Stop => return Action::Stop,
                Action::Cycle => return Action::Cycle,
                Action::Steer(text) => {
                    self.drawn = Instant::now() - Duration::from_secs(1);
                    return Action::Steer(text);
                }
                Action::Image => {
                    if let Some(path) = self.grab() {
                        self.draft.images.push(path);
                        let label = format!("[Image #{}] ", self.draft.images.len());
                        self.draft.editor.insert(&label);
                    }
                }
                Action::Typed | Action::Nothing => {}
            }
            typed = true;
        }
        if typed {
            self.drawn = Instant::now() - Duration::from_secs(1);
            Action::Typed
        } else {
            Action::Nothing
        }
    }

    pub fn draw(&mut self) {
        if self.drawn.elapsed() < Duration::from_millis(80) && !self.widths.is_empty() {
            return;
        }
        let columns = width();
        let mut lines: Vec<String> = self.plan.clone();
        lines.extend(self.draft.lines(self.started.elapsed(), columns));
        lines.push(String::new());
        lines.push(crate::view::footer(&self.footer, columns));
        if crate::pane::pinned() {
            self.tallest = self.tallest.max(lines.len());
            let padding = vec![String::new(); self.tallest - lines.len()];
            lines.splice(0..0, padding);
            crate::pane::draw(&lines, None);
            self.widths = vec![0];
            self.drawn = Instant::now();
            return;
        }
        let up = rows(&self.widths, width()).saturating_sub(1);
        print!("{}", paint(&lines, up, !self.widths.is_empty(), false));
        let _ = io::stdout().flush();
        self.widths = lines.iter().map(|line| visible(line)).collect();
        self.drawn = Instant::now();
    }

    pub fn hide(&mut self) {
        if self.widths.is_empty() || crate::pane::pinned() {
            return;
        }
        let up = u16::try_from(rows(&self.widths, width()).saturating_sub(1)).unwrap_or(u16::MAX);
        let mut out = io::stdout();
        let _ = execute!(out, MoveToColumn(0));
        if up > 0 {
            let _ = execute!(out, MoveUp(up));
        }
        let _ = execute!(out, terminal::Clear(ClearType::FromCursorDown));
        self.widths.clear();
    }

    pub fn close(mut self) -> Draft {
        if crate::pane::pinned() {
            crate::pane::draw(&crate::compose::idle(&self.footer, width()), None);
        }
        self.hide();
        self.restore();
        std::mem::take(&mut self.draft)
    }

    fn restore(&mut self) {
        self.keys = None;
        if let Some(original) = self.saved.take() {
            let _ = execute!(io::stdout(), DisableBracketedPaste);
            loud(&original);
        }
    }
}

impl Drop for Dock {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(draft: &mut Draft, code: KeyCode) -> Action {
        draft.press(code, KeyModifiers::NONE)
    }

    fn typed(draft: &mut Draft, text: &str) {
        for character in text.chars() {
            press(draft, KeyCode::Char(character));
        }
    }

    #[test]
    fn measures_lines_without_their_colors() {
        assert_eq!(visible("\u{1b}[38;5;208m❯\u{1b}[0m hi"), 4);
        assert_eq!(
            visible("\u{1b}]8;;https://sombre.ai\u{7}link\u{1b}]8;;\u{7}"),
            4
        );
        assert_eq!(visible("plain"), 5);
        assert_eq!(visible("日本 ok"), 7);
    }

    #[test]
    fn counts_the_rows_lines_take_after_the_window_narrows() {
        assert_eq!(rows(&[30, 10], 100), 2);
        assert_eq!(rows(&[90, 10], 45), 3);
        assert_eq!(rows(&[0], 40), 1);
        assert_eq!(rows(&[46], 45), 2);
    }

    #[test]
    fn repaints_in_place_inside_one_synchronized_frame() {
        let lines = ["one".to_string(), "two".to_string()];
        let frame = paint(&lines, 1, true, false);
        assert!(frame.starts_with("\u{1b}[?2026h\u{1b}[?25l\r\u{1b}[1A"));
        assert!(frame.contains("one\u{1b}[K\ntwo\u{1b}[K\u{1b}[J"));
        assert!(frame.ends_with("\u{1b}[?25h\u{1b}[?2026l"));
        assert!(!paint(&lines, 0, false, false).contains('\r'));
        assert!(paint(&lines, 0, false, true).contains("one\u{1b}[K\r\ntwo"));
    }

    #[test]
    fn typing_then_enter_queues_a_message() {
        let mut draft = Draft::default();
        typed(&mut draft, "run test");
        press(&mut draft, KeyCode::Backspace);
        typed(&mut draft, "ts");
        assert_eq!(draft.editor.text, "run tests");
        press(&mut draft, KeyCode::Enter);
        assert_eq!(draft.queued, ["run tests"]);
        assert!(draft.editor.text.is_empty());
    }

    #[test]
    fn shift_enter_and_a_trailing_backslash_add_lines_to_a_queued_message() {
        let mut draft = Draft::default();
        typed(&mut draft, "first");
        draft.press(KeyCode::Enter, KeyModifiers::SHIFT);
        typed(&mut draft, "second\\");
        press(&mut draft, KeyCode::Enter);
        typed(&mut draft, "third");
        assert!(draft.queued.is_empty());
        press(&mut draft, KeyCode::Enter);
        assert_eq!(draft.queued, ["first\nsecond\nthird"]);
    }

    #[test]
    fn edits_mid_line_like_the_prompt() {
        let mut draft = Draft::default();
        typed(&mut draft, "fix tests");
        draft.press(KeyCode::Left, KeyModifiers::ALT);
        typed(&mut draft, "the ");
        assert_eq!(draft.editor.text, "fix the tests");
        draft.press(KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(draft.editor.text, "fix tests");
        typed(&mut draft, "!");
        assert_eq!(draft.editor.text, "fix !tests");
    }

    #[test]
    fn enter_and_tab_pick_queue_or_steer_by_the_setting() {
        let mut queue = Draft::default();
        queue.editor.set("add tests");
        assert_eq!(press(&mut queue, KeyCode::Enter), Action::Nothing);
        assert_eq!(queue.queued, ["add tests"]);
        queue.editor.set("now");
        assert_eq!(
            press(&mut queue, KeyCode::Tab),
            Action::Steer(vec!["now".to_string()])
        );
        let mut steer = Draft {
            steer: true,
            ..Draft::default()
        };
        steer.editor.set("now");
        assert_eq!(
            press(&mut steer, KeyCode::Enter),
            Action::Steer(vec!["now".to_string()])
        );
        steer.editor.set("later");
        press(&mut steer, KeyCode::Tab);
        assert_eq!(steer.queued, ["later"]);
    }

    #[test]
    fn long_pastes_become_a_placeholder_that_expands_on_send() {
        let mut draft = Draft::default();
        draft.editor.paste("look: ");
        let log = "error at line\n".repeat(100);
        draft.editor.paste(&log);
        assert_eq!(draft.editor.text, "look: [Pasted text #1, 100 lines]");
        press(&mut draft, KeyCode::Enter);
        assert_eq!(draft.queued, [format!("look: {}", log.trim_end())]);
    }

    #[test]
    fn the_send_now_key_on_an_empty_draft_sends_the_queue() {
        let mut draft = Draft {
            queued: vec!["one".to_string(), "two".to_string()],
            ..Draft::default()
        };
        assert_eq!(press(&mut draft, KeyCode::Enter), Action::Nothing);
        assert_eq!(draft.queued.len(), 2);
        assert_eq!(
            press(&mut draft, KeyCode::Tab),
            Action::Steer(vec!["one".to_string(), "two".to_string()])
        );
        assert!(draft.queued.is_empty());
        let mut steer = Draft {
            steer: true,
            queued: vec!["later".to_string()],
            ..Draft::default()
        };
        assert_eq!(press(&mut steer, KeyCode::Tab), Action::Nothing);
        assert_eq!(
            press(&mut steer, KeyCode::Enter),
            Action::Steer(vec!["later".to_string()])
        );
    }

    #[test]
    fn tab_completes_a_command_before_it_sends() {
        let mut draft = Draft {
            models: vec!["opus".to_string()],
            ..Draft::default()
        };
        typed(&mut draft, "/mod");
        assert_eq!(press(&mut draft, KeyCode::Tab), Action::Nothing);
        assert_eq!(draft.editor.text, "/model");
        typed(&mut draft, " op");
        press(&mut draft, KeyCode::Tab);
        assert_eq!(draft.editor.text, "/model opus");
        assert_eq!(press(&mut draft, KeyCode::Tab), Action::Nothing);
        assert_eq!(draft.queued, ["/model opus"]);
        draft.editor.set("and tests");
        press(&mut draft, KeyCode::Enter);
        assert_eq!(
            press(&mut draft, KeyCode::Tab),
            Action::Steer(vec!["and tests".to_string()])
        );
        assert_eq!(draft.queued, ["/model opus"]);
        let lines = Draft {
            editor: {
                let mut editor = Composer::default();
                editor.set("/perm");
                editor
            },
            ..Draft::default()
        }
        .lines(Duration::ZERO, 60);
        assert!(
            lines
                .last()
                .is_some_and(|line| bare(line).ends_with("/permissions"))
        );
    }

    #[test]
    fn shift_tab_switches_permissions_while_working() {
        let mut draft = Draft::default();
        typed(&mut draft, "half");
        assert_eq!(press(&mut draft, KeyCode::BackTab), Action::Cycle);
        assert_eq!(draft.editor.text, "half");
    }

    #[test]
    fn blank_enter_queues_nothing() {
        let mut draft = Draft::default();
        press(&mut draft, KeyCode::Char(' '));
        press(&mut draft, KeyCode::Enter);
        assert!(draft.queued.is_empty());
    }

    #[test]
    fn up_pulls_the_last_queued_message_back() {
        let mut draft = Draft {
            queued: vec!["one".to_string(), "two".to_string()],
            ..Draft::default()
        };
        press(&mut draft, KeyCode::Up);
        assert_eq!(draft.editor.text, "two");
        assert_eq!(draft.queued, ["one"]);
    }

    #[test]
    fn escape_and_ctrl_c_stop() {
        let mut draft = Draft::default();
        assert_eq!(press(&mut draft, KeyCode::Esc), Action::Stop);
        assert_eq!(
            draft.press(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Action::Stop
        );
    }

    #[test]
    fn shows_status_queue_and_what_is_typed() {
        let mut draft = Draft {
            queued: vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
            ],
            ..Draft::default()
        };
        draft.editor.set("next");
        let lines = draft.lines(Duration::from_secs(12), 46);
        assert_eq!(lines.len(), 7);
        assert!(bare(&lines[0]).starts_with("• working 12s · esc to stop"));
        assert!(lines[1].is_empty());
        assert_eq!(bare(&lines[2]), "┋ a");
        assert!(bare(&lines[5]).contains("1 more"));
        assert!(visible(&lines[6]).eq(&visible("┃ next ")));
        assert!(lines[6].contains("next"));
    }

    #[test]
    fn wraps_long_drafts_and_keeps_the_caret_row_visible() {
        let mut draft = Draft::default();
        draft.editor.set(&format!("{}end", "word ".repeat(40)));
        let lines = draft.lines(Duration::ZERO, 30);
        assert_eq!(lines.len(), 2 + TALLEST);
        assert!(lines.iter().all(|line| visible(line) < 30));
        assert!(lines.last().is_some_and(|line| line.contains("end")));
        draft.editor.set("one\ntwo");
        assert_eq!(draft.lines(Duration::ZERO, 30).len(), 4);
    }
}

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crossterm::cursor::MoveUp;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, ClearType};
use sombre_core::Event as Item;
use sombre_harness::config::Mode;

use crate::view::{bold, clear, diff, dim, ember, fit, relative, stepped, width};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Yes,
    Always,
    No,
}

impl From<Choice> for sombre_harness::Answer {
    fn from(choice: Choice) -> Self {
        match choice {
            Choice::Yes => Self::Yes,
            Choice::Always => Self::Always,
            Choice::No => Self::No,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Press {
    Select(usize),
    Pick(Choice),
    Cycle,
    Ignore,
}

pub fn choices(always: bool) -> Vec<Choice> {
    if always {
        vec![Choice::Yes, Choice::Always, Choice::No]
    } else {
        vec![Choice::Yes, Choice::No]
    }
}

pub fn press(code: KeyCode, modifiers: KeyModifiers, selected: usize, options: &[Choice]) -> Press {
    let count = options.len().max(1);
    match code {
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Press::Pick(Choice::No),
        KeyCode::BackTab => Press::Cycle,
        KeyCode::Up | KeyCode::Char('k') => Press::Select((selected + count - 1) % count),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => Press::Select((selected + 1) % count),
        KeyCode::Enter => options
            .get(selected)
            .map_or(Press::Ignore, |choice| Press::Pick(*choice)),
        KeyCode::Char('y' | 'Y') => Press::Pick(Choice::Yes),
        KeyCode::Char('a' | 'A') if options.contains(&Choice::Always) => {
            Press::Pick(Choice::Always)
        }
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Press::Pick(Choice::No),
        KeyCode::Char(digit @ '1'..='9') => digit
            .to_digit(10)
            .and_then(|number| usize::try_from(number).ok())
            .and_then(|number| options.get(number - 1))
            .map_or(Press::Ignore, |choice| Press::Pick(*choice)),
        _ => Press::Ignore,
    }
}

pub fn title(tool: &str) -> String {
    match tool {
        "Bash" => "run this command?".to_string(),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "make this edit?".to_string(),
        "WebFetch" | "WebSearch" => "go on the web?".to_string(),
        other => format!("use {other}?"),
    }
}

pub fn squeeze(text: &str, room: usize) -> String {
    let count = crate::view::cells(text);
    if count <= room {
        return text.to_string();
    }
    let (Some(start), Some(end)) = (text.find('`'), text.rfind('`')) else {
        return fit(text, room);
    };
    if end <= start {
        return fit(text, room);
    }
    let inner = &text[start + 1..end];
    let keep = room.saturating_sub(count - crate::view::cells(inner));
    if keep < 4 {
        return fit(text, room);
    }
    format!(
        "{}`{}`{}",
        &text[..start],
        fit(inner, keep),
        &text[end + 1..]
    )
}

pub fn first(text: &str) -> String {
    let mut lines = text.lines();
    let head = lines.next().unwrap_or_default().to_string();
    if lines.next().is_some() {
        format!("{head} …")
    } else {
        head
    }
}

pub fn granted(mode: &str) {
    crate::view::line(&format!("    {} allowed by {mode}", ember("✔")));
    stepped();
}

pub fn ask(
    agent: &str,
    item: &Item,
    cwd: &Path,
    mut mode: Mode,
    cycle: &mut dyn FnMut() -> Mode,
) -> Choice {
    let Item::Approval {
        tool,
        detail,
        reason,
        change,
        always,
        ..
    } = item
    else {
        return Choice::No;
    };
    clear();
    let room = width().saturating_sub(4).max(20);
    let detail = first(&relative(detail, cwd));
    let shown = if tool == "Bash" {
        format!("$ {detail}")
    } else {
        detail
    };
    let mut header = vec![
        String::new(),
        bold(&fit(&title(tool), room)),
        fit(&shown, room),
    ];
    if !reason.is_empty() {
        header.push(dim(&fit(&first(reason), room)));
    }
    if !change.is_empty() {
        header.push(String::new());
        header.extend(diff(change, 8));
    }
    header.push(String::new());
    let options = choices(always.is_some());
    let labels: Vec<String> = options
        .iter()
        .map(|choice| {
            let (label, key) = match choice {
                Choice::Yes => ("yes".to_string(), "(y)"),
                Choice::Always => (
                    format!("yes, and {}", always.as_deref().unwrap_or_default()),
                    "(a)",
                ),
                Choice::No => (format!("no, and tell {agent} what to do instead"), "(esc)"),
            };
            squeeze(
                &label.replace('\n', " "),
                room.saturating_sub(key.len() + 7),
            )
        })
        .collect();
    let choice = loop {
        for line in &header {
            if line.is_empty() {
                println!();
            } else {
                println!("  {line}");
            }
        }
        if let Some(choice) = select(&options, &labels, header.len(), mode) {
            break choice;
        }
        mode = cycle();
        if sombre_engine::covers(mode, item) {
            granted(mode.name());
            return Choice::Yes;
        }
    };
    let outcome = match choice {
        Choice::Yes => format!("{} allowed", ember("✔")),
        Choice::Always => format!("{} allowed for this chat", ember("✔")),
        Choice::No => format!("{} denied", bold("✕")),
    };
    crate::view::line(&format!("    {outcome}"));
    stepped();
    choice
}

fn select(options: &[Choice], labels: &[String], header: usize, mode: Mode) -> Option<Choice> {
    if terminal::enable_raw_mode().is_err() {
        return Some(Choice::No);
    }
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let _ = event::read();
    }
    while crate::relay::answer().is_some() {}
    let block = labels.len() + 2;
    let mut selected = 0;
    draw(options, labels, selected, mode);
    let choice = loop {
        if let Some(yes) = crate::relay::answer() {
            break Some(if yes { Choice::Yes } else { Choice::No });
        }
        if sombre_harness::process::hung() {
            break Some(Choice::No);
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => break Some(Choice::No),
        }
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                match press(key.code, key.modifiers, selected, options) {
                    Press::Select(index) => {
                        selected = index;
                        erase(block);
                        draw(options, labels, selected, mode);
                    }
                    Press::Pick(choice) => break Some(choice),
                    Press::Cycle => break None,
                    Press::Ignore => {}
                }
            }
            Err(_) => break Some(Choice::No),
            _ => {}
        }
    };
    erase(header + block);
    let _ = terminal::disable_raw_mode();
    choice
}

fn draw(options: &[Choice], labels: &[String], selected: usize, mode: Mode) {
    let mut out = io::stdout();
    for (index, (choice, label)) in options.iter().zip(labels).enumerate() {
        let key = match choice {
            Choice::Yes => "(y)",
            Choice::Always => "(a)",
            Choice::No => "(esc)",
        };
        let line = format!("{}. {label}", index + 1);
        if index == selected {
            let _ = write!(out, "  {} {}  {}\r\n", ember("›"), bold(&line), dim(key));
        } else {
            let _ = write!(out, "    {line}  {}\r\n", dim(key));
        }
    }
    let _ = write!(
        out,
        "\r\n  {}\r\n",
        dim(&fit(
            &format!(
                "↑↓ to move, enter to confirm, esc to say no. permissions: {}, shift tab switches",
                mode.name()
            ),
            width().saturating_sub(4).max(20)
        ))
    );
    let _ = out.flush();
}

pub fn erase(lines: usize) {
    let rows = u16::try_from(lines).unwrap_or(u16::MAX);
    let _ = execute!(
        io::stdout(),
        MoveUp(rows),
        terminal::Clear(ClearType::FromCursorDown)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Choice; 3] = [Choice::Yes, Choice::Always, Choice::No];

    fn key(code: KeyCode, selected: usize) -> Press {
        press(code, KeyModifiers::NONE, selected, &ALL)
    }

    #[test]
    fn offers_always_only_when_the_agent_suggests_a_rule() {
        assert_eq!(choices(true), ALL.to_vec());
        assert_eq!(choices(false), vec![Choice::Yes, Choice::No]);
    }

    #[test]
    fn arrows_wrap_around_the_options() {
        assert_eq!(key(KeyCode::Down, 2), Press::Select(0));
        assert_eq!(key(KeyCode::Up, 0), Press::Select(2));
        assert_eq!(key(KeyCode::Char('j'), 0), Press::Select(1));
    }

    #[test]
    fn enter_confirms_the_selected_option() {
        assert_eq!(key(KeyCode::Enter, 1), Press::Pick(Choice::Always));
    }

    #[test]
    fn shortcuts_and_numbers_pick_directly() {
        assert_eq!(key(KeyCode::Char('y'), 2), Press::Pick(Choice::Yes));
        assert_eq!(key(KeyCode::Char('a'), 0), Press::Pick(Choice::Always));
        assert_eq!(key(KeyCode::Esc, 0), Press::Pick(Choice::No));
        assert_eq!(key(KeyCode::Char('3'), 0), Press::Pick(Choice::No));
        assert_eq!(key(KeyCode::Char('9'), 0), Press::Ignore);
    }

    #[test]
    fn always_is_ignored_when_not_offered() {
        let two = choices(false);
        assert_eq!(
            press(KeyCode::Char('a'), KeyModifiers::NONE, 0, &two),
            Press::Ignore
        );
        assert_eq!(
            press(KeyCode::Char('2'), KeyModifiers::NONE, 0, &two),
            Press::Pick(Choice::No)
        );
    }

    #[test]
    fn shift_tab_switches_permissions_instead_of_moving() {
        assert_eq!(key(KeyCode::BackTab, 1), Press::Cycle);
        assert_eq!(key(KeyCode::Up, 1), Press::Select(0));
    }

    #[test]
    fn ctrl_c_says_no() {
        assert_eq!(
            press(KeyCode::Char('c'), KeyModifiers::CONTROL, 0, &ALL),
            Press::Pick(Choice::No)
        );
    }

    #[test]
    fn squeeze_shortens_the_command_and_keeps_the_rest() {
        let label = "yes, and don't ask again for `curl -s --max-time 15 \"wttr.in/San+Francisco?format=%l\"` in this chat";
        let short = squeeze(label, 60);
        assert_eq!(short.chars().count(), 60);
        assert!(short.starts_with("yes, and don't ask again for `curl -s"));
        assert!(short.ends_with("…` in this chat"));
    }

    #[test]
    fn squeeze_leaves_short_labels_alone() {
        assert_eq!(squeeze("yes", 60), "yes");
        assert_eq!(
            squeeze("allow edits for the rest of this chat", 20)
                .chars()
                .count(),
            20
        );
    }

    #[test]
    fn first_keeps_one_line() {
        assert_eq!(first("echo one\necho two"), "echo one …");
        assert_eq!(first("ls"), "ls");
    }

    #[test]
    fn titles_read_as_a_question() {
        assert_eq!(title("Bash"), "run this command?");
        assert_eq!(title("Write"), "make this edit?");
        assert_eq!(title("mcp__docs__search"), "use mcp__docs__search?");
    }
}

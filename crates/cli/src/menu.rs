use std::fmt::Display;
use std::io::{self, Write};

use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::{execute, terminal};

use crate::dock::{paint, rows, visible};
use crate::view::{bold, dim, ember, fit, line, width};

const ROWS: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Moved,
    Picked(usize),
    Cancelled,
    Nothing,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct State {
    pub selected: usize,
    pub offset: usize,
    pub query: String,
}

pub fn shown(labels: &[String], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    labels
        .iter()
        .enumerate()
        .filter(|(_, label)| label.to_lowercase().contains(&query))
        .map(|(index, _)| index)
        .collect()
}

impl State {
    pub fn press(
        &mut self,
        code: KeyCode,
        control: bool,
        filter: bool,
        labels: &[String],
        rows: usize,
    ) -> Step {
        let count = shown(labels, &self.query).len();
        match code {
            KeyCode::Esc => return Step::Cancelled,
            KeyCode::Char('c') if control => return Step::Cancelled,
            KeyCode::Enter | KeyCode::Tab => {
                return shown(labels, &self.query)
                    .get(self.selected)
                    .map_or(Step::Nothing, |index| Step::Picked(*index));
            }
            KeyCode::Up => {
                self.selected = self
                    .selected
                    .checked_sub(1)
                    .unwrap_or(count.saturating_sub(1))
            }
            KeyCode::Down => {
                self.selected = if self.selected + 1 >= count {
                    0
                } else {
                    self.selected + 1
                }
            }
            KeyCode::Char('k') if !filter => {
                return self.press(KeyCode::Up, false, filter, labels, rows);
            }
            KeyCode::Char('j') if !filter => {
                return self.press(KeyCode::Down, false, filter, labels, rows);
            }
            KeyCode::Backspace if filter => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char(character) if filter && !control => {
                self.query.push(character);
                self.selected = 0;
            }
            _ => return Step::Nothing,
        }
        let count = shown(labels, &self.query).len();
        self.selected = self.selected.min(count.saturating_sub(1));
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + rows {
            self.offset = self.selected + 1 - rows;
        }
        Step::Moved
    }
}

pub fn lines(
    title: &str,
    labels: &[String],
    hints: &[String],
    state: &State,
    filter: bool,
    rows: usize,
    room: usize,
) -> Vec<String> {
    let mut lines = vec![format!("  {}", bold(&fit(title, room)))];
    let picked = shown(labels, &state.query);
    if picked.is_empty() {
        lines.push(format!("    {}", dim("nothing matches")));
    }
    for (place, index) in picked.iter().enumerate().skip(state.offset).take(rows) {
        let label = fit(&labels[*index], room.saturating_sub(4));
        if place != state.selected {
            lines.push(format!("  {} {label}", dim("○")));
            continue;
        }
        lines.push(format!("  {} {}", ember("●"), bold(&label)));
        if let Some(hint) = hints.get(*index).filter(|hint| !hint.is_empty()) {
            lines.push(format!("    {}", dim(&fit(hint, room.saturating_sub(4)))));
        }
    }
    let more = picked.len().saturating_sub(state.offset + rows);
    let keys = if filter {
        format!(
            "type to filter{}  ↑↓  enter  esc",
            if state.query.is_empty() {
                String::new()
            } else {
                format!(": {}", state.query)
            }
        )
    } else {
        "↑↓  enter  esc".to_string()
    };
    let keys = if more > 0 {
        format!("{keys}  {more} more")
    } else {
        keys
    };
    lines.push(format!("  {}", dim(&fit(&keys, room))));
    lines
}

pub struct Select<T> {
    title: String,
    values: Vec<T>,
    labels: Vec<String>,
    hints: Vec<String>,
    initial: Option<T>,
    rows: usize,
    filter: bool,
    quiet: bool,
}

pub fn select<T: Clone + PartialEq>(title: impl Display) -> Select<T> {
    Select {
        filter: false,
        hints: Vec::new(),
        initial: None,
        labels: Vec::new(),
        quiet: false,
        rows: ROWS,
        title: title.to_string(),
        values: Vec::new(),
    }
}

fn summary(title: &str, answer: &str, columns: usize) -> Vec<String> {
    let room = columns.saturating_sub(3);
    let mut rows: Vec<String> = crate::view::wrap(title, room)
        .iter()
        .map(|row| format!("  {}", dim(row)))
        .collect();
    let fits = rows
        .last()
        .is_some_and(|last| visible(last) + 2 + crate::view::cells(answer) <= room + 2);
    match rows.last_mut() {
        Some(last) if fits => {
            last.push_str("  ");
            last.push_str(answer);
        }
        _ => rows.push(format!("  {answer}")),
    }
    rows
}

fn show(frame: &[String], widths: &[usize]) {
    if crate::pane::pinned() {
        crate::pane::over(frame);
        return;
    }
    print!(
        "{}",
        paint(
            frame,
            rows(widths, width()).saturating_sub(1),
            !widths.is_empty(),
            true
        )
    );
    let _ = io::stdout().flush();
}

fn erase(widths: &[usize]) {
    if crate::pane::pinned() {
        crate::pane::back();
        return;
    }
    let up = rows(widths, width()).saturating_sub(1);
    print!(
        "\r{}\u{1b}[J\u{1b}[?25h",
        if up > 0 {
            format!("\u{1b}[{up}A")
        } else {
            String::new()
        }
    );
    let _ = io::stdout().flush();
}

impl<T: Clone + PartialEq> Select<T> {
    pub fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
        self.values.push(value);
        self.labels.push(label.to_string());
        self.hints.push(hint.to_string());
        self
    }

    pub fn initial_value(mut self, value: T) -> Self {
        self.initial = Some(value);
        self
    }

    pub fn max_rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    pub fn filter_mode(mut self) -> Self {
        self.filter = true;
        self
    }

    pub fn interact(self) -> io::Result<T> {
        let mut state = State::default();
        if let Some(start) = self
            .initial
            .as_ref()
            .and_then(|initial| self.values.iter().position(|value| value == initial))
        {
            state.selected = start;
            state.offset = start.saturating_sub(self.rows - 1);
        }
        terminal::enable_raw_mode()?;
        let mut widths: Vec<usize> = Vec::new();
        let picked = loop {
            let room = width().saturating_sub(4).max(10);
            let frame = lines(
                &self.title,
                &self.labels,
                &self.hints,
                &state,
                self.filter,
                self.rows,
                room,
            );
            show(&frame, &widths);
            widths = frame.iter().map(|line| visible(line)).collect();
            let step = loop {
                match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        let control = key.modifiers.contains(KeyModifiers::CONTROL);
                        let step =
                            state.press(key.code, control, self.filter, &self.labels, self.rows);
                        if step != Step::Nothing {
                            break step;
                        }
                    }
                    Ok(Event::Resize(..)) => break Step::Moved,
                    Err(error) => {
                        erase(&widths);
                        let _ = terminal::disable_raw_mode();
                        return Err(error);
                    }
                    _ => {}
                }
            };
            match step {
                Step::Picked(index) => break Some(index),
                Step::Cancelled => break None,
                Step::Moved | Step::Nothing => {}
            }
        };
        erase(&widths);
        terminal::disable_raw_mode()?;
        match picked {
            Some(index) => {
                if !self.quiet {
                    for row in summary(&self.title, &self.labels[index], width()) {
                        line(&row);
                    }
                }
                Ok(self.values[index].clone())
            }
            None => Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled")),
        }
    }
}

pub struct Confirm {
    question: String,
    initial: bool,
}

pub fn confirm(question: impl Display) -> Confirm {
    Confirm {
        initial: true,
        question: question.to_string(),
    }
}

impl Confirm {
    pub fn initial_value(mut self, initial: bool) -> Self {
        self.initial = initial;
        self
    }

    pub fn interact(self) -> io::Result<bool> {
        select(self.question)
            .item(true, "yes", "")
            .item(false, "no", "")
            .initial_value(self.initial)
            .interact()
    }
}

type Check = Box<dyn Fn(&String) -> Result<(), &'static str>>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub error: Option<&'static str>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Edit {
    Changed,
    Submit,
    Cancel,
    Nothing,
}

impl Field {
    pub fn press(&mut self, code: KeyCode, control: bool) -> Edit {
        match code {
            KeyCode::Esc => Edit::Cancel,
            KeyCode::Char('c') if control => Edit::Cancel,
            KeyCode::Char('u') if control => {
                self.text.clear();
                Edit::Changed
            }
            KeyCode::Enter => Edit::Submit,
            KeyCode::Backspace => {
                self.text.pop();
                self.error = None;
                Edit::Changed
            }
            KeyCode::Char(character) if !control => {
                self.text.push(character);
                self.error = None;
                Edit::Changed
            }
            _ => Edit::Nothing,
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.text
            .extend(text.chars().filter(|character| !character.is_control()));
        self.error = None;
    }
}

fn masked(text: &str, mask: Option<char>) -> String {
    mask.map_or_else(
        || text.to_string(),
        |mask| mask.to_string().repeat(text.chars().count()),
    )
}

pub fn field(
    title: &str,
    placeholder: &str,
    field: &Field,
    mask: Option<char>,
    room: usize,
) -> Vec<String> {
    let shown = if field.text.is_empty() {
        dim(placeholder)
    } else {
        let text = masked(&field.text, mask);
        let keep = room.saturating_sub(6);
        if visible(&text) > keep {
            let tail: String = text
                .chars()
                .rev()
                .take(keep.saturating_sub(1))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("…{tail}")
        } else {
            text
        }
    };
    let mut lines = vec![
        format!("  {}", bold(&fit(title, room))),
        format!("  {} {shown}\u{1b}[7m \u{1b}[0m", ember("›")),
    ];
    if let Some(error) = field.error {
        lines.push(format!(
            "    {} {}",
            bold("✕"),
            fit(error, room.saturating_sub(6))
        ));
    }
    lines.push(format!("  {}", dim("enter  esc")));
    lines
}

pub struct Input {
    title: String,
    placeholder: String,
    initial: String,
    mask: Option<char>,
    empty: bool,
    check: Option<Check>,
}

pub fn input(title: impl Display) -> Input {
    Input {
        check: None,
        empty: false,
        initial: String::new(),
        mask: None,
        placeholder: String::new(),
        title: title.to_string(),
    }
}

pub fn password(title: impl Display) -> Input {
    input(title).mask('•')
}

impl Input {
    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = placeholder.to_string();
        self
    }

    pub fn default_input(mut self, initial: &str) -> Self {
        self.initial = initial.to_string();
        self
    }

    pub fn mask(mut self, mask: char) -> Self {
        self.mask = Some(mask);
        self
    }

    pub fn allow_empty(mut self) -> Self {
        self.empty = true;
        self
    }

    pub fn validate(
        mut self,
        check: impl Fn(&String) -> Result<(), &'static str> + 'static,
    ) -> Self {
        self.check = Some(Box::new(check));
        self
    }

    pub fn interact(self) -> io::Result<String> {
        let mut state = Field {
            text: self.initial.clone(),
            error: None,
        };
        terminal::enable_raw_mode()?;
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        let mut widths: Vec<usize> = Vec::new();
        let done = loop {
            let room = width().saturating_sub(4).max(10);
            let frame = field(&self.title, &self.placeholder, &state, self.mask, room);
            show(&frame, &widths);
            widths = frame.iter().map(|line| visible(line)).collect();
            let edit = match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    state.press(key.code, key.modifiers.contains(KeyModifiers::CONTROL))
                }
                Ok(Event::Paste(text)) => {
                    state.paste(&text);
                    Edit::Changed
                }
                Ok(_) => Edit::Nothing,
                Err(_) => Edit::Cancel,
            };
            match edit {
                Edit::Cancel => break None,
                Edit::Submit => {
                    let text = state.text.trim().to_string();
                    let problem = if text.is_empty() && !self.empty {
                        Some("type something, or press esc to go back")
                    } else {
                        self.check.as_ref().and_then(|check| check(&text).err())
                    };
                    match problem {
                        Some(problem) => state.error = Some(problem),
                        None => break Some(text),
                    }
                }
                Edit::Changed | Edit::Nothing => {}
            }
        };
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        erase(&widths);
        terminal::disable_raw_mode()?;
        let text = done.ok_or_else(|| io::Error::new(io::ErrorKind::Interrupted, "cancelled"))?;
        if !text.is_empty() {
            line(&format!(
                "  {}  {}",
                dim(&self.title),
                masked(&text, self.mask)
            ));
        }
        Ok(text)
    }
}

pub struct Multi<T> {
    title: String,
    values: Vec<T>,
    labels: Vec<String>,
    hints: Vec<String>,
    required: bool,
}

pub fn multiselect<T: Clone>(title: impl Display) -> Multi<T> {
    Multi {
        hints: Vec::new(),
        labels: Vec::new(),
        required: true,
        title: title.to_string(),
        values: Vec::new(),
    }
}

pub fn ticks(
    title: &str,
    labels: &[String],
    hints: &[String],
    chosen: &[bool],
    selected: usize,
    room: usize,
) -> Vec<String> {
    let mut lines = vec![format!("  {}", bold(&fit(title, room)))];
    for (index, label) in labels.iter().enumerate() {
        let mark = if chosen.get(index).copied().unwrap_or(false) {
            ember("■")
        } else {
            dim("□")
        };
        let label = fit(label, room.saturating_sub(4));
        if index != selected {
            lines.push(format!("  {mark} {label}"));
            continue;
        }
        lines.push(format!("  {mark} {}", bold(&label)));
        if let Some(hint) = hints.get(index).filter(|hint| !hint.is_empty()) {
            lines.push(format!("    {}", dim(&fit(hint, room.saturating_sub(4)))));
        }
    }
    lines.push(format!("  {}", dim("↑↓  space picks  enter  esc")));
    lines
}

impl<T: Clone> Multi<T> {
    pub fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
        self.values.push(value);
        self.labels.push(label.to_string());
        self.hints.push(hint.to_string());
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn interact(self) -> io::Result<Vec<T>> {
        let mut chosen = vec![false; self.values.len()];
        let mut selected = 0;
        terminal::enable_raw_mode()?;
        let mut widths: Vec<usize> = Vec::new();
        let done = loop {
            let room = width().saturating_sub(4).max(10);
            let frame = ticks(
                &self.title,
                &self.labels,
                &self.hints,
                &chosen,
                selected,
                room,
            );
            show(&frame, &widths);
            widths = frame.iter().map(|line| visible(line)).collect();
            let key = match event::read() {
                Ok(Event::Key(key)) => key,
                Ok(_) => continue,
                Err(_) => break false,
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let count = self.values.len().max(1);
            match key.code {
                KeyCode::Esc => break false,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break false,
                KeyCode::Up => selected = (selected + count - 1) % count,
                KeyCode::Down => selected = (selected + 1) % count,
                KeyCode::Char(' ') => {
                    if let Some(tick) = chosen.get_mut(selected) {
                        *tick = !*tick;
                    }
                }
                KeyCode::Enter if !self.required || chosen.contains(&true) => break true,
                _ => {}
            }
        };
        erase(&widths);
        terminal::disable_raw_mode()?;
        if !done {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let picked: Vec<T> = self
            .values
            .iter()
            .zip(&chosen)
            .filter(|(_, tick)| **tick)
            .map(|(value, _)| value.clone())
            .collect();
        let names: Vec<&str> = self
            .labels
            .iter()
            .zip(&chosen)
            .filter(|(_, tick)| **tick)
            .map(|(label, _)| label.as_str())
            .collect();
        if !names.is_empty() {
            for row in summary(&self.title, &names.join(", "), width()) {
                line(&row);
            }
        }
        Ok(picked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_wrap_long_questions_and_keep_the_answer_whole() {
        let rows = summary(
            "yolo lets the agent change any file and run any command on your computer without asking. turn it on?",
            "no",
            60,
        );
        assert!(rows.len() >= 2);
        assert!(rows.iter().all(|row| visible(row) < 60));
        assert!(rows.iter().all(|row| row.starts_with("  ")));
        assert!(rows.last().is_some_and(|row| row.ends_with("  no")));
        assert_eq!(summary("model", "opus", 80).len(), 1);
    }

    fn labels(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn hints_sit_right_under_the_selected_row() {
        let names = labels(&["anywhere", "home network"]);
        let hints = labels(&["over tailscale", "only on this wi-fi"]);
        let state = State::default();
        let drawn = lines("where?", &names, &hints, &state, false, 8, 60);
        assert!(drawn[1].contains("anywhere"));
        assert!(drawn[2].contains("over tailscale"));
        assert!(drawn[3].contains("home network"));
        assert!(!drawn.iter().any(|line| line.contains("wi-fi")));
        let ticked = ticks("pick", &names, &hints, &[false, true], 1, 60);
        assert!(ticked[2].contains("home network"));
        assert!(ticked[3].contains("wi-fi"));
    }

    #[test]
    fn edits_text_and_takes_pastes_without_newlines() {
        let mut state = Field::default();
        for character in "sk-1".chars() {
            state.press(KeyCode::Char(character), false);
        }
        state.press(KeyCode::Backspace, false);
        state.paste("23\n45");
        assert_eq!(state.text, "sk-2345");
        assert_eq!(state.press(KeyCode::Enter, false), Edit::Submit);
        assert_eq!(state.press(KeyCode::Esc, false), Edit::Cancel);
        assert_eq!(state.press(KeyCode::Char('u'), true), Edit::Changed);
        assert!(state.text.is_empty());
    }

    #[test]
    fn masks_keys_and_shows_errors_under_the_field() {
        let state = Field {
            error: Some("start with https://"),
            text: "secret".to_string(),
        };
        let shown = field("api key", "", &state, Some('•'), 40);
        assert!(shown[1].contains("••••••"));
        assert!(!shown.iter().any(|line| line.contains("secret")));
        assert!(shown[2].contains("start with https://"));
    }

    #[test]
    fn ticks_show_what_is_picked() {
        let names = labels(&["github", "linear"]);
        let shown = ticks(
            "add servers?",
            &names,
            &labels(&["", ""]),
            &[true, false],
            1,
            40,
        );
        assert!(shown[1].contains("■") && shown[2].contains("□"));
    }

    #[test]
    fn moves_wraps_and_scrolls() {
        let names = labels(&["a", "b", "c", "d"]);
        let mut state = State::default();
        assert_eq!(
            state.press(KeyCode::Up, false, false, &names, 2),
            Step::Moved
        );
        assert_eq!((state.selected, state.offset), (3, 2));
        state.press(KeyCode::Down, false, false, &names, 2);
        assert_eq!((state.selected, state.offset), (0, 0));
        state.press(KeyCode::Char('j'), false, false, &names, 2);
        assert_eq!(
            state.press(KeyCode::Enter, false, false, &names, 2),
            Step::Picked(1)
        );
        assert_eq!(
            state.press(KeyCode::Esc, false, false, &names, 2),
            Step::Cancelled
        );
    }

    #[test]
    fn filters_as_you_type() {
        let names = labels(&["gpt-oss-120b", "claude-opus", "qwen-coder"]);
        let mut state = State::default();
        for character in "co".chars() {
            state.press(KeyCode::Char(character), false, true, &names, 8);
        }
        assert_eq!(shown(&names, &state.query), [2]);
        assert_eq!(
            state.press(KeyCode::Enter, false, true, &names, 8),
            Step::Picked(2)
        );
        state.press(KeyCode::Backspace, false, true, &names, 8);
        assert_eq!(shown(&names, &state.query), [1, 2]);
    }

    #[test]
    fn shows_only_the_hint_of_the_selected_row_and_fits_the_width() {
        let names = labels(&["on", "off"]);
        let hints = labels(&[
            "a very long explanation that would never fit on a narrow screen",
            "",
        ]);
        let shown = lines("settings", &names, &hints, &State::default(), false, 8, 20);
        assert_eq!(shown.len(), 5);
        assert!(shown.iter().all(|line| visible(line) <= 24));
    }
}

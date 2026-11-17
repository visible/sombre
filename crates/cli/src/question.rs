use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use sombre_core::Event as Item;
use sombre_core::event::Question;

use crate::menu::{Edit, Field};
use crate::view::{bold, clear, dim, ember, fit, stepped, width, wrap};

const OTHER: &str = "something else";

#[derive(Debug, Default)]
pub struct Card {
    pub selected: usize,
    pub ticked: Vec<bool>,
    pub typing: Option<Field>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Redraw,
    Answer(String),
    Skip,
    Ignore,
}

enum Asked {
    Answer(String),
    Skip,
    Phone(Option<Vec<String>>),
}

impl Card {
    fn new(question: &Question) -> Self {
        Self {
            ticked: vec![false; question.choices.len()],
            ..Self::default()
        }
    }

    fn chosen(&self, question: &Question, typed: Option<&str>) -> String {
        question
            .choices
            .iter()
            .zip(&self.ticked)
            .filter(|(_, ticked)| **ticked)
            .map(|(choice, _)| choice.label.as_str())
            .chain(typed.filter(|text| !text.is_empty()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn press(card: &mut Card, question: &Question, code: KeyCode, modifiers: KeyModifiers) -> Step {
    let control = modifiers.contains(KeyModifiers::CONTROL);
    if let Some(field) = card.typing.as_mut() {
        return match field.press(code, control) {
            Edit::Submit if field.text.trim().is_empty() => {
                field.error = Some("type an answer, or press esc to go back");
                Step::Redraw
            }
            Edit::Submit => {
                let typed = field.text.trim().to_string();
                Step::Answer(card.chosen(question, Some(&typed)))
            }
            Edit::Cancel => {
                card.typing = None;
                Step::Redraw
            }
            Edit::Changed => Step::Redraw,
            Edit::Nothing => Step::Ignore,
        };
    }
    let rows = question.choices.len() + 1;
    let other = question.choices.len();
    match code {
        KeyCode::Esc => Step::Skip,
        KeyCode::Char('c') if control => Step::Skip,
        KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
            card.selected = (card.selected + rows - 1) % rows;
            Step::Redraw
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
            card.selected = (card.selected + 1) % rows;
            Step::Redraw
        }
        KeyCode::Char(digit @ '1'..='9') => {
            let Some(index) = digit
                .to_digit(10)
                .and_then(|number| usize::try_from(number).ok())
                .map(|number| number - 1)
                .filter(|index| *index < rows)
            else {
                return Step::Ignore;
            };
            card.selected = index;
            pick(card, question, other, true)
        }
        KeyCode::Char(' ') if question.multiple => pick(card, question, other, true),
        KeyCode::Enter => pick(card, question, other, false),
        _ => Step::Ignore,
    }
}

fn pick(card: &mut Card, question: &Question, other: usize, tick: bool) -> Step {
    if card.selected == other {
        card.typing = Some(Field {
            error: None,
            text: String::new(),
        });
        return Step::Redraw;
    }
    if !question.multiple {
        return Step::Answer(question.choices[card.selected].label.clone());
    }
    let none = !card.ticked.contains(&true);
    if let Some(ticked) = card.ticked.get_mut(card.selected) {
        if tick {
            *ticked = !*ticked;
            return Step::Redraw;
        }
        *ticked |= none;
    }
    Step::Answer(card.chosen(question, None))
}

pub fn lines(
    agent: &str,
    question: &Question,
    card: &Card,
    place: (usize, usize),
    room: usize,
) -> Vec<String> {
    let mut context = Vec::new();
    if !question.header.is_empty() {
        context.push(question.header.clone());
    }
    if place.1 > 1 {
        context.push(format!("{} of {}", place.0 + 1, place.1));
    }
    let title = format!("{agent} asks");
    let mut lines = vec![String::new()];
    lines.push(if context.is_empty() {
        bold(&fit(&title, room))
    } else {
        format!(
            "{}  {}",
            bold(&title),
            dim(&fit(
                &context.join(" · "),
                room.saturating_sub(crate::view::cells(&title) + 2)
            ))
        )
    });
    lines.extend(wrap(&question.text, room));
    lines.push(String::new());
    let other = question.choices.len();
    for index in 0..=other {
        let number = format!("{}. ", index + 1);
        let mark = match card.ticked.get(index) {
            Some(true) if question.multiple => "[x] ",
            Some(false) if question.multiple => "[ ] ",
            _ => "",
        };
        let label = question
            .choices
            .get(index)
            .map_or(OTHER, |choice| choice.label.as_str());
        let space = room.saturating_sub(4 + number.len() + mark.len());
        let selected = index == card.selected;
        match (&card.typing, selected && index == other) {
            (Some(field), true) => {
                let tail: String = field
                    .text
                    .chars()
                    .rev()
                    .take(space.saturating_sub(1))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                lines.push(format!("{} {number}{tail}\u{1b}[7m \u{1b}[0m", ember("›")));
                if let Some(error) = field.error {
                    lines.push(format!("     {}", dim(&fit(error, room.saturating_sub(5)))));
                }
            }
            (_, _) if selected => {
                lines.push(format!(
                    "{} {}",
                    ember("›"),
                    bold(&format!("{number}{mark}{}", fit(label, space)))
                ));
                if let Some(about) = question
                    .choices
                    .get(index)
                    .map(|choice| choice.about.as_str())
                    .filter(|about| !about.is_empty())
                {
                    lines.push(format!("     {}", dim(&fit(about, room.saturating_sub(5)))));
                }
            }
            _ => lines.push(format!("  {number}{mark}{}", fit(label, space))),
        }
    }
    lines.push(String::new());
    let keys = match (&card.typing, question.multiple) {
        (Some(_), _) => "type your answer, enter to send, esc to go back",
        (None, true) => "↑↓ to move, space to tick, enter to send, esc to skip",
        (None, false) => "↑↓ to move, enter to pick, esc to skip",
    };
    lines.push(dim(&fit(keys, room)));
    lines
}

fn paint(lines: &[String]) -> usize {
    let mut out = io::stdout();
    for line in lines {
        if line.is_empty() {
            let _ = write!(out, "\r\n");
        } else {
            let _ = write!(out, "  {line}\r\n");
        }
    }
    let _ = out.flush();
    lines.len()
}

fn ask(agent: &str, id: &str, question: &Question, place: (usize, usize)) -> Asked {
    let mut card = Card::new(question);
    let room = || width().saturating_sub(4).max(20);
    let mut shown = paint(&lines(agent, question, &card, place, room()));
    let asked = loop {
        if let Some(reply) = crate::relay::replied(id) {
            break Asked::Phone(reply);
        }
        if sombre_harness::process::hung() {
            break Asked::Skip;
        }
        match event::poll(Duration::from_millis(100)) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => break Asked::Skip,
        }
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                match press(&mut card, question, key.code, key.modifiers) {
                    Step::Redraw => {
                        crate::approve::erase(shown);
                        shown = paint(&lines(agent, question, &card, place, room()));
                    }
                    Step::Answer(text) => break Asked::Answer(text),
                    Step::Skip => break Asked::Skip,
                    Step::Ignore => {}
                }
            }
            Err(_) => break Asked::Skip,
            _ => {}
        }
    };
    crate::approve::erase(shown);
    asked
}

pub fn answer(agent: &str, item: &Item) -> Option<Vec<String>> {
    let Item::Questions { id, questions, .. } = item else {
        return None;
    };
    clear();
    if terminal::enable_raw_mode().is_err() {
        return None;
    }
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let _ = event::read();
    }
    let mut answers = Vec::new();
    let mut outcome = None;
    for (index, question) in questions.iter().enumerate() {
        match ask(agent, id, question, (index, questions.len())) {
            Asked::Answer(text) => answers.push(text),
            Asked::Skip => {
                outcome = Some(None);
                break;
            }
            Asked::Phone(reply) => {
                outcome = Some(reply);
                break;
            }
        }
    }
    let _ = terminal::disable_raw_mode();
    let result = outcome.unwrap_or(Some(answers));
    let room = width().saturating_sub(6).max(20);
    match &result {
        Some(given) => {
            for (question, given) in questions.iter().zip(given) {
                crate::view::line(&format!(
                    "    {} {}",
                    dim("?"),
                    dim(&fit(&question.text, room))
                ));
                crate::view::line(&format!("    {} {}", ember("✔"), fit(given, room)));
            }
        }
        None => crate::view::line(&format!("    {} skipped", bold("✕"))),
    }
    stepped();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sombre_core::event::Choice;

    fn question(multiple: bool) -> Question {
        Question {
            choices: ["Ember", "Ink"]
                .iter()
                .map(|label| Choice {
                    about: format!("about {label}"),
                    label: (*label).to_string(),
                })
                .collect(),
            header: "Color".to_string(),
            multiple,
            text: "Which color should the button be?".to_string(),
        }
    }

    fn key(card: &mut Card, question: &Question, code: KeyCode) -> Step {
        press(card, question, code, KeyModifiers::NONE)
    }

    #[test]
    fn a_single_choice_answers_with_its_label() {
        let asked = question(false);
        let mut card = Card::new(&asked);
        assert_eq!(key(&mut card, &asked, KeyCode::Down), Step::Redraw);
        assert_eq!(
            key(&mut card, &asked, KeyCode::Enter),
            Step::Answer("Ink".to_string())
        );
        assert_eq!(
            key(&mut card, &asked, KeyCode::Char('1')),
            Step::Answer("Ember".to_string())
        );
        assert_eq!(key(&mut card, &asked, KeyCode::Esc), Step::Skip);
    }

    #[test]
    fn something_else_takes_a_typed_answer() {
        let asked = question(false);
        let mut card = Card::new(&asked);
        assert_eq!(key(&mut card, &asked, KeyCode::Char('3')), Step::Redraw);
        assert!(card.typing.is_some());
        assert_eq!(key(&mut card, &asked, KeyCode::Enter), Step::Redraw);
        assert!(
            card.typing
                .as_ref()
                .is_some_and(|field| field.error.is_some())
        );
        for letter in "teal".chars() {
            key(&mut card, &asked, KeyCode::Char(letter));
        }
        assert_eq!(
            key(&mut card, &asked, KeyCode::Enter),
            Step::Answer("teal".to_string())
        );
        assert_eq!(key(&mut card, &asked, KeyCode::Esc), Step::Redraw);
        assert!(card.typing.is_none());
    }

    #[test]
    fn multiple_choices_tick_and_join() {
        let asked = question(true);
        let mut card = Card::new(&asked);
        assert_eq!(key(&mut card, &asked, KeyCode::Char(' ')), Step::Redraw);
        assert_eq!(key(&mut card, &asked, KeyCode::Char('2')), Step::Redraw);
        assert_eq!(card.ticked, vec![true, true]);
        assert_eq!(
            key(&mut card, &asked, KeyCode::Enter),
            Step::Answer("Ember, Ink".to_string())
        );
        let mut fresh = Card::new(&asked);
        key(&mut fresh, &asked, KeyCode::Down);
        assert_eq!(
            key(&mut fresh, &asked, KeyCode::Enter),
            Step::Answer("Ink".to_string())
        );
    }

    #[test]
    fn the_card_fits_narrow_terminals() {
        let asked = question(true);
        let card = Card::new(&asked);
        for room in [41, 76] {
            let drawn = lines("claude", &asked, &card, (0, 2), room);
            assert!(
                drawn
                    .iter()
                    .all(|line| crate::dock::visible(line) <= room + 2)
            );
            assert!(drawn.iter().any(|line| line.contains("about Ember")));
            assert!(!drawn.iter().any(|line| line.contains("about Ink")));
        }
    }
}

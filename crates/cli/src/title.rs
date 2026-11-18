use std::io::{self, IsTerminal};
use std::sync::Mutex;

use crossterm::execute;
use crossterm::terminal::SetTitle;

static SHOWN: Mutex<Option<String>> = Mutex::new(None);

pub fn set(text: &str) {
    let Some(clean) = cleaned(text) else {
        return;
    };
    if !io::stdout().is_terminal() {
        return;
    }
    let Ok(mut shown) = SHOWN.lock() else {
        return;
    };
    if shown.as_deref() == Some(clean.as_str()) {
        return;
    }
    let _ = execute!(io::stdout(), SetTitle(&clean));
    *shown = Some(clean);
}

pub fn reset() {
    if SHOWN
        .lock()
        .ok()
        .and_then(|mut shown| shown.take())
        .is_some()
    {
        let _ = execute!(io::stdout(), SetTitle(""));
    }
}

fn cleaned(text: &str) -> Option<String> {
    let visible: String = sombre_core::scrub::scrub(text)
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                ' '
            } else {
                character
            }
        })
        .filter(|character| !hidden(*character))
        .collect();
    sombre_core::title(&visible)
}

fn hidden(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_keep_the_words_and_drop_anything_that_could_spoof_or_break_the_terminal() {
        assert_eq!(
            cleaned("fix the\nlogin  bug").as_deref(),
            Some("fix the login bug")
        );
        assert_eq!(
            cleaned("pay \u{202E}lio\u{1b}]0;evil\u{7} now").as_deref(),
            Some("pay lio now")
        );
        assert_eq!(cleaned("\u{200B}\u{7}"), None);
        let long = "word ".repeat(40);
        assert!(cleaned(&long).is_some_and(|title| title.chars().count() <= 80));
    }
}

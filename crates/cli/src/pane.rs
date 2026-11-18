use std::env;
use std::io::{self, IsTerminal, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use crossterm::{cursor, terminal};
use unicode_width::UnicodeWidthChar;

use crate::view;

const WHEEL: isize = 3;
const ENTER: &str = "\u{1b}[?1049h\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1006h\u{1b}[H\u{1b}[2J";
const EXIT: &str = "\u{1b}[?1006l\u{1b}[?1002l\u{1b}[?1000l\u{1b}[r\u{1b}[?1049l";

type Spot = (u16, u16);

struct State {
    on: bool,
    full: bool,
    inside: bool,
    height: u16,
    screen: u16,
    away: bool,
    base: Vec<String>,
    offset: usize,
    unseen: usize,
    held: Option<Spot>,
    reach: Option<Spot>,
    shown: Vec<String>,
    copied: Option<Instant>,
}

static STATE: Mutex<State> = Mutex::new(State {
    on: false,
    full: false,
    inside: false,
    height: 0,
    screen: 0,
    away: false,
    base: Vec::new(),
    offset: 0,
    unseen: 0,
    held: None,
    reach: None,
    shown: Vec::new(),
    copied: None,
});

pub struct Pin;

impl Drop for Pin {
    fn drop(&mut self) {
        let full = STATE.lock().is_ok_and(|state| state.on && state.full);
        release();
        if full {
            print!("{}", printed(&view::transcript()));
            let _ = io::stdout().flush();
        }
        if let Ok(mut state) = STATE.lock() {
            state.on = false;
        }
    }
}

fn printed(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn able(full: bool) -> bool {
    cfg!(unix)
        && io::stdout().is_terminal()
        && io::stdin().is_terminal()
        && env::var("TERM").is_ok_and(|term| term != "dumb")
        && (full || env::var("TERM_PROGRAM").map_or(true, |program| program != "vscode"))
        && env::var_os("WT_SESSION").is_none()
        && env::var_os("ZELLIJ").is_none()
}

pub fn start(full: bool) -> Option<Pin> {
    if !able(full) {
        return None;
    }
    let mut state = STATE.lock().ok()?;
    state.on = true;
    state.full = full;
    if full {
        print!("{ENTER}");
        let _ = io::stdout().flush();
        state.inside = true;
    }
    Some(Pin)
}

pub fn pinned() -> bool {
    STATE.lock().is_ok_and(|state| state.on)
}

pub fn inside() -> bool {
    STATE.lock().is_ok_and(|state| state.on && state.inside)
}

pub fn following() -> bool {
    STATE
        .lock()
        .map_or(true, |state| !state.inside || state.offset == 0)
}

pub fn grew(rows: usize) {
    if let Ok(mut state) = STATE.lock()
        && state.inside
        && state.offset > 0
    {
        state.offset += rows;
        state.unseen += rows;
    }
}

pub fn behind() -> Option<String> {
    let state = STATE.lock().ok()?;
    if !state.inside {
        return None;
    }
    if state
        .copied
        .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
    {
        return Some(format!("{} {}", view::ember("■"), view::dim("copied")));
    }
    if state.offset == 0 {
        return None;
    }
    Some(match state.unseen {
        0 => format!(
            "{} {}",
            view::ember("↑"),
            view::dim("scrolled up · ctrl+end to return")
        ),
        count => format!(
            "{} {}",
            view::ember("↓"),
            view::dim(&format!(
                "{count} new {} · ctrl+end",
                if count == 1 { "line" } else { "lines" }
            ))
        ),
    })
}

fn region(bottom: u16, screen: u16) -> String {
    if bottom >= screen {
        "\u{1b}[r".to_string()
    } else {
        format!("\u{1b}[1;{bottom}r")
    }
}

fn at(row: u16, column: u16) -> String {
    format!("\u{1b}[{};{}H", row + 1, column + 1)
}

fn measure(count: usize, screen: u16) -> u16 {
    let keep = usize::from(screen.saturating_sub(2)).max(1);
    u16::try_from(count.min(keep)).unwrap_or(0)
}

fn order(held: Spot, reach: Spot) -> (Spot, Spot) {
    if held <= reach {
        (held, reach)
    } else {
        (reach, held)
    }
}

fn span(selection: Option<(Spot, Spot)>, row: u16) -> Option<(usize, usize)> {
    let ((top, left), (end, right)) = selection?;
    if row < top || row > end {
        return None;
    }
    let from = if row == top { usize::from(left) } else { 0 };
    let to = if row == end {
        usize::from(right) + 1
    } else {
        usize::MAX
    };
    Some((from, to))
}

fn cut(line: &str, from: usize, to: usize) -> (String, String, String) {
    let mut parts = (String::new(), String::new(), String::new());
    let mut column = 0;
    for character in crate::dock::bare(line).chars() {
        let part = if column < from {
            &mut parts.0
        } else if column < to {
            &mut parts.1
        } else {
            &mut parts.2
        };
        part.push(character);
        column += character.width().unwrap_or(0);
    }
    parts
}

fn sheet(rows: &[String], bottom: u16, columns: u16, selection: Option<(Spot, Spot)>) -> String {
    let room = usize::from(columns).saturating_sub(1);
    let mut out = String::new();
    for row in 0..bottom {
        out.push_str(&at(row, 0));
        if let Some(line) = rows.get(usize::from(row)) {
            match span(selection, row) {
                Some((from, to)) => {
                    let (before, inside, after) = cut(line, from, to);
                    out.push_str(&format!("{before}\u{1b}[7m{inside}\u{1b}[27m{after}"));
                }
                None => out.push_str(&view::clip(line, room)),
            }
        }
        out.push_str("\u{1b}[K");
    }
    out
}

pub fn picked(rows: &[String], held: Spot, reach: Spot) -> String {
    let selection = Some(order(held, reach));
    let ((top, _), (end, _)) = order(held, reach);
    (top..=end)
        .filter_map(|row| {
            let line = rows.get(usize::from(row))?;
            let (from, to) = span(selection, row)?;
            Some(cut(line, from, to).1.trim_end().to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compose(
    state: &mut State,
    lines: &[String],
    cursor: Option<(usize, usize)>,
    (columns, screen): (u16, u16),
    window: Option<&[String]>,
    locate: &mut dyn FnMut(&str) -> Option<(u16, u16)>,
) -> String {
    if screen != state.screen {
        state.height = 0;
    }
    let height = measure(lines.len(), screen);
    let shown = &lines[lines.len() - usize::from(height)..];
    let bottom = screen.saturating_sub(height);
    let mut out = String::from("\u{1b}[?2026h\u{1b}[?25l");
    if state.away {
        out.push_str("\u{1b}8");
        state.away = false;
    }
    if let Some(rows) = window {
        out.push_str(&region(bottom, screen));
        out.push_str(&sheet(rows, bottom, columns, None));
        let row = if state.offset == 0 {
            u16::try_from(rows.len()).unwrap_or(0)
        } else {
            bottom
        };
        out.push_str(&at(row.min(bottom.saturating_sub(1)), 0));
        state.shown = rows.to_vec();
        state.held = None;
        state.reach = None;
    } else if height > state.height {
        let last = screen.saturating_sub(state.height).saturating_sub(1);
        let (column, mut row) = locate(&std::mem::take(&mut out)).unwrap_or((0, last));
        if row >= bottom {
            let lift = row + 1 - bottom;
            out.push_str(&at(last, 0));
            out.push_str(&"\n".repeat(usize::from(lift)));
            row = bottom.saturating_sub(1);
        }
        out.push_str(&region(bottom, screen));
        out.push_str(&at(row, column));
    } else if height < state.height {
        out.push_str("\u{1b}7");
        out.push_str(&region(bottom, screen));
        for spare in screen.saturating_sub(state.height)..bottom {
            out.push_str(&at(spare, 0));
            out.push_str("\u{1b}[K");
        }
        out.push_str("\u{1b}8");
    }
    out.push_str("\u{1b}7");
    let room = usize::from(columns).saturating_sub(1);
    for (index, line) in (bottom..).zip(shown) {
        out.push_str(&at(index, 0));
        out.push_str(&view::clip(line, room));
        out.push_str("\u{1b}[K");
    }
    match cursor {
        Some((row, column)) => {
            let row = u16::try_from(row).unwrap_or(0);
            let column = u16::try_from(column).unwrap_or(0);
            out.push_str(&at(bottom + row.min(height.saturating_sub(1)), column));
            out.push_str("\u{1b}[?25h");
            state.away = true;
        }
        None => out.push_str("\u{1b}8"),
    }
    out.push_str("\u{1b}[?2026l");
    state.height = height;
    state.screen = screen;
    out
}

fn size() -> (u16, u16) {
    terminal::size().unwrap_or((80, 24))
}

fn paint(lines: &[String], cursor: Option<(usize, usize)>, base: bool) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if !state.on {
        return;
    }
    if base {
        state.base = lines.to_vec();
    }
    let (columns, screen) = size();
    let mut prefix = String::new();
    if state.full && !state.inside {
        prefix.push_str(ENTER);
        state.inside = true;
        state.height = 0;
        state.screen = 0;
        state.away = false;
    }
    let height = measure(lines.len(), screen);
    let window = (state.inside && (height != state.height || screen != state.screen)).then(|| {
        let bottom = screen.saturating_sub(height);
        view::window(
            state.offset,
            usize::from(bottom.saturating_sub(1)),
            usize::from(columns),
        )
    });
    let mut out = io::stdout();
    let mut locate = |prefix: &str| {
        let _ = write!(out, "{prefix}");
        let _ = out.flush();
        cursor::position().ok()
    };
    let _ = write!(io::stdout(), "{prefix}");
    let frame = compose(
        &mut state,
        lines,
        cursor,
        (columns, screen),
        window.as_deref(),
        &mut locate,
    );
    let _ = write!(out, "{frame}");
    let _ = out.flush();
}

pub fn draw(lines: &[String], cursor: Option<(usize, usize)>) {
    paint(lines, cursor, true);
}

pub fn over(lines: &[String]) {
    paint(lines, None, false);
}

pub fn back() {
    let base = STATE
        .lock()
        .map(|state| state.base.clone())
        .unwrap_or_default();
    paint(&base, None, false);
}

fn redraw(state: &mut State) {
    let (columns, screen) = size();
    let bottom = screen.saturating_sub(state.height);
    let rows = view::window(
        state.offset,
        usize::from(bottom.saturating_sub(1)),
        usize::from(columns),
    );
    let row = if state.offset == 0 {
        u16::try_from(rows.len()).unwrap_or(0)
    } else {
        bottom
    };
    let out = format!(
        "\u{1b}[?2026h\u{1b}[?25l{}{}\u{1b}[?2026l",
        sheet(&rows, bottom, columns, None),
        at(row.min(bottom.saturating_sub(1)), 0)
    );
    state.shown = rows;
    state.held = None;
    state.reach = None;
    state.away = false;
    print!("{out}");
    let _ = io::stdout().flush();
}

fn moved(state: &mut State, offset: usize) -> bool {
    let (columns, screen) = size();
    let room = usize::from(screen.saturating_sub(state.height).saturating_sub(1));
    let reached = view::rows(offset.saturating_add(room), usize::from(columns));
    let offset = offset.min(reached.saturating_sub(room));
    if offset == state.offset && state.held.is_none() {
        return false;
    }
    state.offset = offset;
    if offset == 0 {
        state.unseen = 0;
    }
    redraw(state);
    true
}

pub fn scroll(delta: isize) -> bool {
    let Ok(mut state) = STATE.lock() else {
        return false;
    };
    if !state.inside {
        return false;
    }
    let offset = state.offset.saturating_add_signed(delta);
    moved(&mut state, offset)
}

pub fn page(up: bool) -> bool {
    let half = isize::try_from(size().1.saturating_sub(4) / 2)
        .unwrap_or(1)
        .max(1);
    scroll(if up { half } else { -half })
}

pub fn edge(top: bool) -> bool {
    let Ok(mut state) = STATE.lock() else {
        return false;
    };
    if !state.inside {
        return false;
    }
    moved(&mut state, if top { usize::MAX } else { 0 })
}

pub fn mouse(event: MouseEvent) -> bool {
    match event.kind {
        MouseEventKind::ScrollUp => scroll(WHEEL),
        MouseEventKind::ScrollDown => scroll(-WHEEL),
        MouseEventKind::Down(MouseButton::Left)
        | MouseEventKind::Drag(MouseButton::Left)
        | MouseEventKind::Up(MouseButton::Left) => select(event),
        _ => false,
    }
}

fn highlight(state: &State) {
    let (columns, screen) = size();
    let bottom = screen.saturating_sub(state.height);
    let selection = state
        .held
        .zip(state.reach)
        .map(|(held, reach)| order(held, reach));
    let body = sheet(&state.shown, bottom, columns, selection);
    let out = if state.away {
        format!("\u{1b}[?2026h{body}\u{1b}[?2026l")
    } else {
        format!("\u{1b}[?2026h\u{1b}7{body}\u{1b}8\u{1b}[?2026l")
    };
    print!("{out}");
    let _ = io::stdout().flush();
}

fn select(event: MouseEvent) -> bool {
    let Ok(mut state) = STATE.lock() else {
        return false;
    };
    if !state.inside {
        return false;
    }
    let bottom = size().1.saturating_sub(state.height);
    let spot = (event.row.min(bottom.saturating_sub(1)), event.column);
    match event.kind {
        MouseEventKind::Down(_) if event.row >= bottom => {
            if state.held.take().is_some() {
                state.reach = None;
                highlight(&state);
                return true;
            }
            false
        }
        MouseEventKind::Down(_) => {
            redraw(&mut state);
            state.held = Some(spot);
            state.reach = Some(spot);
            true
        }
        MouseEventKind::Drag(_) if state.held.is_some() => {
            state.reach = Some(spot);
            highlight(&state);
            true
        }
        MouseEventKind::Up(_) => match state.held.zip(state.reach) {
            Some((held, reach)) if held != reach => {
                let text = picked(&state.shown, held, reach);
                if !text.is_empty() && crate::clipboard::copy(&text) {
                    state.copied = Some(Instant::now());
                }
                true
            }
            _ => {
                state.held = None;
                state.reach = None;
                false
            }
        },
        _ => false,
    }
}

pub fn switch(full: bool) {
    let was = {
        let Ok(state) = STATE.lock() else {
            return;
        };
        if !state.on || state.full == full {
            return;
        }
        state.full
    };
    release();
    if was {
        print!("{}", printed(&view::transcript()));
        let _ = io::stdout().flush();
    }
    if let Ok(mut state) = STATE.lock() {
        state.full = full;
    }
}

pub fn leave() {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if state.away {
        state.away = false;
        print!("\u{1b}8\u{1b}[?25l");
        let _ = io::stdout().flush();
    }
}

pub fn reset() -> &'static str {
    if let Ok(mut state) = STATE.lock()
        && state.on
    {
        state.height = 0;
        state.away = false;
        state.offset = 0;
        state.unseen = 0;
        state.held = None;
        state.reach = None;
        return "\u{1b}[r";
    }
    ""
}

pub fn release() {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if !state.on {
        return;
    }
    let mut out = String::from("\u{1b}[?2026h");
    if state.inside {
        out.push_str(EXIT);
        state.inside = false;
        state.offset = 0;
        state.unseen = 0;
    } else {
        if state.away {
            out.push_str("\u{1b}8");
        }
        if state.height > 0 {
            out.push_str("\u{1b}7");
            for row in state.screen.saturating_sub(state.height)..state.screen {
                out.push_str(&at(row, 0));
                out.push_str("\u{1b}[K");
            }
            out.push_str("\u{1b}[r\u{1b}8");
        }
    }
    out.push_str("\u{1b}[?25h\u{1b}[?2026l");
    state.height = 0;
    state.away = false;
    print!("{out}");
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> State {
        State {
            on: true,
            full: false,
            inside: false,
            height: 0,
            screen: 24,
            away: false,
            base: Vec::new(),
            offset: 0,
            unseen: 0,
            held: None,
            reach: None,
            shown: Vec::new(),
            copied: None,
        }
    }

    fn lines(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("row {index}")).collect()
    }

    fn pinned(state: &mut State, count: usize, row: u16) -> String {
        compose(state, &lines(count), None, (80, 24), None, &mut |_| {
            Some((0, row))
        })
    }

    #[test]
    fn pins_the_pane_to_the_bottom_rows_and_returns_to_the_output() {
        let mut state = fresh();
        let frame = pinned(&mut state, 3, 5);
        assert!(frame.contains("\u{1b}[1;21r"));
        assert!(frame.contains("\u{1b}[22;1Hrow 0\u{1b}[K"));
        assert!(frame.contains("\u{1b}[24;1Hrow 2\u{1b}[K"));
        assert!(frame.ends_with("\u{1b}8\u{1b}[?2026l"));
        assert!(!frame.contains('\n'));
        assert_eq!(state.height, 3);
        assert!(!state.away);
    }

    #[test]
    fn lifts_the_history_when_the_pane_would_cover_it() {
        let mut state = fresh();
        let frame = pinned(&mut state, 3, 23);
        assert!(frame.contains("\u{1b}[24;1H\n\n\n\u{1b}[1;21r\u{1b}[21;1H"));
        let again = pinned(&mut state, 5, 20);
        assert!(again.contains("\u{1b}[21;1H\n\n\u{1b}[1;19r\u{1b}[19;1H"));
    }

    #[test]
    fn shrinking_clears_the_freed_rows_without_asking_where_the_cursor_is() {
        let mut state = fresh();
        pinned(&mut state, 5, 0);
        let frame = compose(&mut state, &lines(2), None, (80, 24), None, &mut |_| {
            panic!("no need to ask")
        });
        assert!(frame.contains("\u{1b}7\u{1b}[1;22r\u{1b}[20;1H\u{1b}[K\u{1b}[21;1H\u{1b}[K"));
        assert!(frame.contains("\u{1b}[23;1Hrow 0"));
        assert_eq!(state.height, 2);
    }

    #[test]
    fn a_cursor_stays_in_the_pane_until_output_takes_it_back() {
        let mut state = fresh();
        let frame = compose(
            &mut state,
            &lines(3),
            Some((1, 4)),
            (80, 24),
            None,
            &mut |_| Some((0, 0)),
        );
        assert!(frame.ends_with("\u{1b}[23;5H\u{1b}[?25h\u{1b}[?2026l"));
        assert!(state.away);
        let next = pinned(&mut state, 3, 0);
        assert!(next.starts_with("\u{1b}[?2026h\u{1b}[?25l\u{1b}8"));
    }

    #[test]
    fn never_takes_the_whole_screen() {
        let mut state = fresh();
        compose(&mut state, &lines(40), None, (80, 10), None, &mut |_| {
            Some((0, 0))
        });
        assert_eq!(state.height, 8);
    }

    #[test]
    fn full_screen_redraws_the_history_above_the_pane_without_asking() {
        let mut state = fresh();
        state.inside = true;
        let history = lines(2);
        let frame = compose(
            &mut state,
            &lines(3),
            None,
            (80, 24),
            Some(&history),
            &mut |_| panic!("no need to ask"),
        );
        assert!(frame.contains("\u{1b}[1;21r\u{1b}[1;1Hrow 0\u{1b}[K\u{1b}[2;1Hrow 1\u{1b}[K"));
        assert!(frame.contains("\u{1b}[21;1H\u{1b}[K\u{1b}[3;1H\u{1b}7"));
        assert_eq!(state.shown, history);
    }

    #[test]
    fn selections_copy_plain_text_across_rows() {
        let rows = vec![
            format!("  {} hello world", view::ember("■")),
            "  second line here".to_string(),
        ];
        assert_eq!(picked(&rows, (0, 4), (0, 8)), "hello");
        assert_eq!(picked(&rows, (1, 8), (0, 10)), "world\n  second");
        let (before, inside, after) = cut(&rows[0], 4, 9);
        assert_eq!((before.as_str(), inside.as_str()), ("  ■ ", "hello"));
        assert_eq!(after, " world");
    }
}

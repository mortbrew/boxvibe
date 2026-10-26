//! A line of text being typed, with a cursor: the email and password prompts
//! and the rename box all edit through this, so they all behave alike.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

/// The text and where the cursor is (a count of characters, not bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    text: String,
    cursor: usize,
}

impl TextInput {
    /// Starts with `text`, the cursor at its end.
    pub fn new(text: &str) -> Self {
        Self { text: text.to_owned(), cursor: text.chars().count() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// The text, leaving this empty.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }

    fn byte(&self, chars: usize) -> usize {
        self.text.char_indices().nth(chars).map_or(self.text.len(), |(i, _)| i)
    }

    /// Types `c` at the cursor, unless the text is already `max` characters long.
    pub fn insert(&mut self, c: char, max: usize) {
        if self.len() < max {
            let at = self.byte(self.cursor);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }

    /// Ctrl+w: the word before the cursor.
    fn delete_word(&mut self) {
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) == Some(' ') {
            self.backspace();
        }
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) != Some(' ') {
            self.backspace();
        }
    }

    /// Handles an editing key; false when the key is not one (the caller
    /// then treats it: Enter, Esc …).
    pub fn key(&mut self, key: KeyEvent, max: usize) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.len(),
            KeyCode::Char('b') if ctrl => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('f') if ctrl => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Char('d') if ctrl => self.delete(),
            KeyCode::Char('u') if ctrl => self.clear(),
            KeyCode::Char('w') if ctrl => self.delete_word(),
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => self.insert(c, max),
            _ => return false,
        }
        true
    }

    /// What to draw in `cells` columns (`mask` draws a • per character), and
    /// the column of the cursor in it. The visible part scrolls to keep the
    /// cursor in view, with room for it at the end. `width_of` is the width
    /// to fit with (the worst case on any terminal); the cursor column is
    /// where the terminal really draws the text up to it.
    pub fn window(&self, cells: usize, mask: bool, width_of: impl Fn(char) -> usize) -> (String, usize) {
        let chars: Vec<char> = self.text.chars().map(|c| if mask { '•' } else { c }).collect();
        let cells = cells.max(1);
        let span = |from: usize, to: usize| chars[from..to].iter().map(|c| width_of(*c)).sum::<usize>();
        let mut start = 0;
        while start < self.cursor && span(start, self.cursor) + 1 > cells {
            start += 1;
        }
        let mut end = start;
        let mut used = 0;
        while end < chars.len() && used + width_of(chars[end]) <= cells {
            used += width_of(chars[end]);
            end += 1;
        }
        let column = chars[start..self.cursor].iter().map(|c| c.width().unwrap_or(0)).sum();
        (chars[start..end].iter().collect(), column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(input: &mut TextInput, code: KeyCode, modifiers: KeyModifiers) {
        assert!(input.key(KeyEvent::new(code, modifiers), 100), "{code:?}");
    }

    fn type_in(input: &mut TextInput, text: &str) {
        for c in text.chars() {
            press(input, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    #[test]
    fn typing_goes_in_at_the_cursor() {
        let mut t = TextInput::new("helo");
        press(&mut t, KeyCode::Left, KeyModifiers::NONE);
        type_in(&mut t, "l");
        assert_eq!((t.text(), t.cursor()), ("hello", 4));
        press(&mut t, KeyCode::Home, KeyModifiers::NONE);
        type_in(&mut t, ">");
        assert_eq!((t.text(), t.cursor()), (">hello", 1));
        press(&mut t, KeyCode::End, KeyModifiers::NONE);
        type_in(&mut t, "!");
        assert_eq!(t.text(), ">hello!");
    }

    #[test]
    fn arrows_stop_at_the_ends() {
        let mut t = TextInput::new("ab");
        for _ in 0..5 {
            press(&mut t, KeyCode::Right, KeyModifiers::NONE);
        }
        assert_eq!(t.cursor(), 2);
        for _ in 0..5 {
            press(&mut t, KeyCode::Left, KeyModifiers::NONE);
        }
        assert_eq!(t.cursor(), 0);
    }

    #[test]
    fn backspace_and_delete_work_at_the_cursor() {
        let mut t = TextInput::new("abcd");
        press(&mut t, KeyCode::Left, KeyModifiers::NONE);
        press(&mut t, KeyCode::Left, KeyModifiers::NONE);
        press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!((t.text(), t.cursor()), ("acd", 1));
        press(&mut t, KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!((t.text(), t.cursor()), ("ad", 1));
        press(&mut t, KeyCode::Home, KeyModifiers::NONE);
        press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(t.text(), "ad", "nothing before the cursor");
        press(&mut t, KeyCode::End, KeyModifiers::NONE);
        press(&mut t, KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(t.text(), "ad", "nothing after the cursor");
    }

    #[test]
    fn ctrl_keys_move_and_clear() {
        let mut t = TextInput::new("one two");
        press(&mut t, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(t.cursor(), 0);
        press(&mut t, KeyCode::Char('e'), KeyModifiers::CONTROL);
        assert_eq!(t.cursor(), 7);
        press(&mut t, KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(t.text(), "one ");
        press(&mut t, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!((t.text(), t.cursor()), ("", 0));
    }

    #[test]
    fn multibyte_characters_count_as_one() {
        let mut t = TextInput::new("añb");
        press(&mut t, KeyCode::Left, KeyModifiers::NONE);
        press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(t.text(), "ab");
        press(&mut t, KeyCode::Home, KeyModifiers::NONE);
        type_in(&mut t, "名");
        assert_eq!((t.text(), t.cursor()), ("名ab", 1));
    }

    #[test]
    fn the_length_limit_stops_typing_only() {
        let mut t = TextInput::new("abc");
        assert!(t.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE), 3));
        assert_eq!(t.text(), "abc");
        press(&mut t, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(t.text(), "ab");
    }

    #[test]
    fn enter_and_esc_are_left_to_the_caller() {
        let mut t = TextInput::new("x");
        assert!(!t.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 9));
        assert!(!t.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 9));
    }

    #[test]
    fn the_window_keeps_the_cursor_in_view() {
        let one = |_: char| 1;
        let t = TextInput::new("abcdefghij");
        assert_eq!(t.window(20, false, one), ("abcdefghij".into(), 10));
        // Cursor at the end of a long line: the tail shows, the cursor after it.
        assert_eq!(t.window(5, false, one), ("ghij".into(), 4));
        let mut moved = t.clone();
        for _ in 0..10 {
            moved.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), 99);
        }
        assert_eq!(moved.window(5, false, one), ("abcde".into(), 0));
        assert_eq!(TextInput::new("abc").window(8, true, one), ("•••".into(), 3));
    }
}

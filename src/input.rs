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
mod tests;

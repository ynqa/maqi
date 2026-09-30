use unicode_segmentation::UnicodeSegmentation;

use crate::{Component, Content, Line, Size, Span, TextPosition, component::display_text};

/// Editable input, with a Unicode scalar offset for maqi's completion ranges.
/// Movement and deletion operate on extended grapheme clusters.
#[derive(Clone, Debug)]
pub struct Editor {
    text: String,
    cursor: usize,
    pub prompt: String,
    pub continuation_prompt: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            prompt: "maqi> ".into(),
            continuation_prompt: "...> ".into(),
        }
    }
}

impl Editor {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn position(&self) -> usize {
        self.cursor
    }

    fn byte(&self, character: usize) -> usize {
        self.text
            .char_indices()
            .nth(character)
            .map_or(self.text.len(), |(byte, _)| byte)
    }

    pub fn replace(&mut self, text: &str) {
        self.text = text.to_owned();
        self.move_to_tail();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let end = self.byte(self.cursor) + text.len();
        self.text.insert_str(self.byte(self.cursor), &text);
        // Inserting a combining character or ZWJ can join the following cluster.
        let next = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|&i| i >= end)
            .unwrap_or(self.text.len());
        self.cursor = self.text[..next].chars().count();
    }

    pub fn insert(&mut self, ch: char) {
        self.insert_text(&ch.to_string());
    }
    pub fn insert_newline(&mut self) {
        self.insert('\n');
    }
    pub fn move_to_tail(&mut self) {
        self.cursor = self.text.chars().count();
    }

    pub fn move_to(&mut self, character: usize) {
        let byte = self.byte(character);
        let boundary = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|&i| i <= byte)
            .last()
            .unwrap_or(0);
        self.cursor = if byte == self.text.len() {
            self.text.chars().count()
        } else {
            self.text[..boundary].chars().count()
        };
    }

    pub fn backward(&mut self) {
        let byte = self.byte(self.cursor);
        if let Some((start, _)) = self.text[..byte].grapheme_indices(true).next_back() {
            self.cursor = self.text[..start].chars().count();
        }
    }

    pub fn forward(&mut self) {
        if let Some(grapheme) = self.text[self.byte(self.cursor)..].graphemes(true).next() {
            self.cursor += grapheme.chars().count();
        }
    }

    pub fn erase(&mut self) {
        let end = self.byte(self.cursor);
        self.backward();
        let start = self.byte(self.cursor);
        self.text.replace_range(start..end, "");
        self.move_to(self.cursor);
    }

    pub fn erase_forward(&mut self) {
        let start = self.byte(self.cursor);
        self.forward();
        let end = self.byte(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor = self.text[..start].chars().count();
        self.move_to(self.cursor);
    }

    fn line_head(&self) -> usize {
        self.text[..self.byte(self.cursor)]
            .rfind('\n')
            .map_or(0, |i| i + 1)
    }

    pub fn move_to_line_head(&mut self) {
        self.cursor = self.text[..self.line_head()].chars().count();
    }

    pub fn move_to_line_tail(&mut self) {
        let byte = self.byte(self.cursor);
        let end = self.text[byte..]
            .find('\n')
            .map_or(self.text.len(), |i| byte + i);
        self.cursor = self.text[..end].chars().count();
    }

    pub fn move_up(&mut self) -> bool {
        let head = self.line_head();
        if head == 0 {
            return false;
        }
        let column = self.text[head..self.byte(self.cursor)]
            .graphemes(true)
            .count();
        let previous = self.text[..head - 1].rfind('\n').map_or(0, |i| i + 1);
        self.cursor = self.text[..previous].chars().count()
            + self.text[previous..head - 1]
                .graphemes(true)
                .take(column)
                .map(|g| g.chars().count())
                .sum::<usize>();
        true
    }

    pub fn move_down(&mut self) -> bool {
        let head = self.line_head();
        let Some(end) = self.text[self.byte(self.cursor)..].find('\n') else {
            return false;
        };
        let next = self.byte(self.cursor) + end + 1;
        let column = self.text[head..self.byte(self.cursor)]
            .graphemes(true)
            .count();
        self.cursor = self.text[..next].chars().count()
            + self.text[next..]
                .split('\n')
                .next()
                .unwrap_or("")
                .graphemes(true)
                .take(column)
                .map(|g| g.chars().count())
                .sum::<usize>();
        true
    }
}

impl Component for Editor {
    fn render(&self, _size: Size) -> Content {
        let prefix = &self.text[..self.byte(self.cursor)];
        let line = prefix.bytes().filter(|&ch| ch == b'\n').count();
        let prompt = if line == 0 {
            &self.prompt
        } else {
            &self.continuation_prompt
        };
        Content {
            lines: self
                .text
                .split('\n')
                .enumerate()
                .map(|(i, text)| Line {
                    spans: vec![
                        Span::plain(display_text(if i == 0 {
                            &self.prompt
                        } else {
                            &self.continuation_prompt
                        })),
                        Span::plain(display_text(text)),
                    ],
                })
                .collect(),
            cursor: Some(TextPosition {
                line,
                character: display_text(prompt).chars().count()
                    + display_text(prefix.rsplit('\n').next().unwrap_or(""))
                        .chars()
                        .count(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_graphemes_and_reports_character_offsets() {
        let mut editor = Editor::default();
        editor.insert_text("a界e\u{301}👩‍💻z");
        editor.backward();
        editor.erase();
        assert_eq!(editor.text(), "a界e\u{301}z");
        assert_eq!(editor.position(), 4);
        editor.backward();
        assert_eq!(editor.position(), 2);
        editor.erase_forward();
        assert_eq!(editor.text(), "a界z");
    }

    #[test]
    fn normalizes_paste_without_executing_controls() {
        let mut editor = Editor::default();
        editor.insert_text("a\r\nb\rc\x1b[2J");
        assert_eq!(editor.text(), "a\nb\nc\x1b[2J");
        let content = editor.render(Size {
            columns: 2,
            rows: 1,
        });
        assert_eq!(
            content.lines.iter().map(Line::text).collect::<Vec<_>>(),
            ["maqi> a", "...> b", "...> c^[[2J"]
        );
    }
}

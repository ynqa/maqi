use crossterm::style::ContentStyle;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub columns: u16,
    pub rows: u16,
}

/// A character offset within a logical line, before terminal wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPosition {
    pub line: usize,
    pub character: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: ContentStyle,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: ContentStyle::default(),
        }
    }

    pub fn styled(text: impl Into<String>, style: ContentStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![Span::plain(text)],
        }
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.spans
            .iter()
            .map(|span| UnicodeWidthStr::width(display_text(&span.text).as_str()))
            .sum()
    }

    /// Clip a supporting component's line, keeping graphemes and styles intact.
    /// Editor lines deliberately do not use this operation.
    pub fn truncate(&mut self, columns: usize) {
        if self.width() <= columns {
            return;
        }
        let mut remaining = columns.saturating_sub(1);
        let mut spans = Vec::new();
        'outer: for span in &self.spans {
            let mut text = String::new();
            for grapheme in display_text(&span.text).graphemes(true) {
                let width = UnicodeWidthStr::width(grapheme);
                if width > remaining {
                    spans.push(Span::styled(text, span.style));
                    break 'outer;
                }
                text.push_str(grapheme);
                remaining -= width;
            }
            spans.push(Span::styled(text, span.style));
        }
        if columns > 0 {
            spans.push(Span::plain("…"));
        } else {
            spans.clear();
        }
        self.spans = spans;
    }
}

#[derive(Clone, Debug, Default)]
pub struct Content {
    pub lines: Vec<Line>,
    /// Only the focused component should supply a cursor.
    pub cursor: Option<TextPosition>,
}

/// Implement this for completion, status, diagnostics, or other inline output.
/// Components are rendered in the order passed to `Renderer::render`.
/// Lines contain text, not escape sequences; use span styles for decoration.
pub trait Component {
    fn render(&self, size: Size) -> Content;
}

/// Keep pasted controls and component data from becoming terminal commands.
pub(crate) fn display_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_control() {
            output.push('^');
            output.push(if ch == '\x7f' {
                '?'
            } else {
                (ch as u8 + 64) as char
            });
        } else if ch.is_control() {
            output.push('\u{fffd}');
        } else {
            output.push(ch);
        }
    }
    output
}

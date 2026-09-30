use crate::{Component, Size, component::display_text};
use crossterm::style::ContentStyle;
use std::io;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Glyph {
    pub text: String,
    pub style: ContentStyle,
}

#[derive(Default)]
pub(crate) struct Frame {
    pub glyphs: Vec<Glyph>,
    pub cursor: usize,
    pub focused_end: Option<usize>,
}

impl Frame {
    pub fn new(components: &[&dyn Component], size: Size) -> io::Result<Self> {
        let mut frame = Self::default();
        let mut has_line = false;
        let mut cursor_set = false;
        for component in components {
            let content = component.render(size);
            if content.cursor.is_some() && cursor_set {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "only one component may own the terminal cursor",
                ));
            }
            for (row, line) in content.lines.iter().enumerate() {
                if has_line {
                    frame.glyphs.push(Glyph {
                        text: "\n".into(),
                        style: ContentStyle::default(),
                    });
                }
                has_line = true;
                let mut character = 0;
                for span in &line.spans {
                    for grapheme in span.text.graphemes(true) {
                        if content
                            .cursor
                            .is_some_and(|c| c.line == row && c.character == character)
                        {
                            frame.cursor = frame.glyphs.len();
                            cursor_set = true;
                        }
                        let text = display_text(grapheme);
                        // Control representations (e.g. ^[) consist of multiple cells.
                        for grapheme in text.graphemes(true) {
                            frame.glyphs.push(Glyph {
                                text: grapheme.to_owned(),
                                style: span.style,
                            });
                        }
                        character += grapheme.chars().count();
                    }
                }
                if content
                    .cursor
                    .is_some_and(|c| c.line == row && c.character == character)
                {
                    frame.cursor = frame.glyphs.len();
                    cursor_set = true;
                }
            }
            if content.cursor.is_some() && !cursor_set {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "component cursor must be on a grapheme boundary in its content",
                ));
            }
            if content.cursor.is_some() {
                frame.focused_end = Some(frame.glyphs.len());
            }
        }
        // Materialize the next cell even when the input fills the rightmost
        // column. This ordinary blank gives the native cursor its own cell.
        if cursor_set && frame.cursor == frame.glyphs.len() {
            frame.glyphs.push(Glyph {
                text: " ".into(),
                style: ContentStyle::default(),
            });
        }
        Ok(frame)
    }

    /// Measure logical input independently of the visible editing viewport.
    pub fn positions(&self, columns: u16) -> Vec<Position> {
        let width = usize::from(columns.max(1));
        let mut positions = Vec::with_capacity(self.glyphs.len() + 1);
        let mut pos = Position::default();
        for glyph in &self.glyphs {
            if glyph.text == "\n" {
                positions.push(pos);
                pos.row += 1;
                pos.column = 0;
            } else {
                let cells = UnicodeWidthStr::width(glyph.text.as_str()).min(width);
                if cells > 0 && pos.column + cells > width {
                    pos.row += 1;
                    pos.column = 0;
                }
                positions.push(pos);
                pos.column += cells;
            }
        }
        positions.push(pos);
        positions
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Position {
    pub row: usize,
    pub column: usize,
}

/// Physical rows owned by the editor. Unsubmitted content never needs to scroll
/// just to display a later part of the input; only the viewport changes.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Layout {
    pub rows: Vec<Vec<Glyph>>,
    pub cursor: Position,
}

impl Layout {
    pub fn new(frame: &Frame, size: Size) -> Self {
        let positions = frame.positions(size.columns);
        let focus_end = frame.focused_end.unwrap_or(frame.glyphs.len());
        let mut input = vec![Vec::new()];
        for (index, glyph) in frame.glyphs[..focus_end].iter().enumerate() {
            let pos = positions[index];
            input.resize_with(input.len().max(pos.row + 1), Vec::new);
            if glyph.text != "\n" {
                let mut glyph = glyph.clone();
                if UnicodeWidthStr::width(glyph.text.as_str()) > usize::from(size.columns) {
                    glyph.text = "�".into();
                }
                input[pos.row].push(glyph);
            }
        }
        let mut cursor = positions[frame.cursor];
        if cursor.column >= usize::from(size.columns) {
            cursor.row += 1;
            cursor.column = 0;
        }
        input.resize_with(input.len().max(cursor.row + 1), Vec::new);
        // Supporting rows are clipped to the remaining height. They are never
        // allowed to push the input cursor out of the visible frame.
        let mut support = Vec::<Vec<Glyph>>::new();
        for glyph in &frame.glyphs[focus_end..] {
            if glyph.text == "\n" {
                support.push(Vec::new());
            } else if let Some(row) = support.last_mut() {
                let width: usize = row
                    .iter()
                    .map(|g| UnicodeWidthStr::width(g.text.as_str()))
                    .sum();
                if width + UnicodeWidthStr::width(glyph.text.as_str()) <= usize::from(size.columns)
                {
                    row.push(glyph.clone());
                }
            }
        }
        support.truncate(usize::from(size.rows.saturating_sub(1)));
        let available = usize::from(size.rows) - support.len();
        let first = cursor.row.saturating_sub(available - 1);
        let end = input.len().min(first + available);
        let mut rows = input[first..end].to_vec();
        cursor.row -= first;
        rows.extend(support);
        Self { rows, cursor }
    }

    /// Reflow the rows that were actually painted, rather than the complete
    /// logical input (which may have been outside the editing viewport).
    pub fn reflowed_cursor(&self, columns: u16) -> Position {
        let width = usize::from(columns.max(1));
        let preceding: usize = self.rows[..self.cursor.row]
            .iter()
            .map(|row| {
                let cells: usize = row
                    .iter()
                    .map(|g| UnicodeWidthStr::width(g.text.as_str()))
                    .sum();
                cells.max(1).div_ceil(width)
            })
            .sum();
        Position {
            row: preceding + self.cursor.column / width,
            column: self.cursor.column % width,
        }
    }

    pub fn reflowed_height(&self, columns: u16) -> usize {
        self.rows
            .iter()
            .map(|row| {
                let cells: usize = row
                    .iter()
                    .map(|g| UnicodeWidthStr::width(g.text.as_str()))
                    .sum();
                cells.max(1).div_ceil(usize::from(columns.max(1)))
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Content, Editor, Line};

    struct Menu;
    impl Component for Menu {
        fn render(&self, _: Size) -> Content {
            Content {
                lines: vec![Line::plain("> first"), Line::plain("second")],
                cursor: None,
            }
        }
    }

    #[test]
    fn viewport_keeps_cursor_and_menu_visible_at_one_column() {
        let mut editor = Editor::default();
        editor.insert_text("kubectl");
        for position in [0, 3, 7] {
            editor.move_to(position);
            let size = Size {
                columns: 1,
                rows: 4,
            };
            let frame = Frame::new(&[&editor, &Menu], size).unwrap();
            let layout = Layout::new(&frame, size);
            assert_eq!(layout.rows.len(), 4);
            assert!(layout.cursor.row < 2);
            assert_eq!(layout.cursor.column, 0);
            assert!(layout.rows.iter().all(|row| {
                row.iter()
                    .map(|g| UnicodeWidthStr::width(g.text.as_str()))
                    .sum::<usize>()
                    <= 1
            }));
            assert_eq!(editor.text(), "kubectl");
        }
    }

    #[test]
    fn full_margin_reserves_a_cell_for_the_native_cursor() {
        let mut editor = Editor::default();
        editor.insert_text("abcdef");
        let size = Size {
            columns: 12,
            rows: 3,
        };
        let frame = Frame::new(&[&editor], size).unwrap();
        let layout = Layout::new(&frame, size);
        assert_eq!(layout.cursor, Position { row: 1, column: 0 });
        assert_eq!(layout.rows.len(), 2);
    }

    #[test]
    fn wide_grapheme_has_a_visible_fallback_in_a_single_column() {
        let mut editor = Editor::default();
        editor.insert_text("界");
        let size = Size {
            columns: 1,
            rows: 10,
        };
        let frame = Frame::new(&[&editor], size).unwrap();
        let layout = Layout::new(&frame, size);
        assert!(layout.rows.iter().flatten().any(|g| g.text == "�"));
        assert_eq!(editor.text(), "界");
    }

    #[test]
    fn leading_combining_character_keeps_the_cursor_valid() {
        let mut editor = Editor::default();
        editor.insert_text("\u{301}a");
        editor.move_to(0);
        let size = Size {
            columns: 20,
            rows: 3,
        };
        let frame = Frame::new(&[&editor], size).unwrap();
        assert_eq!(Layout::new(&frame, size).cursor.column, 6);
    }

    #[test]
    fn multiple_cursor_owners_are_rejected() {
        let editor = Editor::default();
        let error = Frame::new(
            &[&editor, &editor],
            Size {
                columns: 20,
                rows: 3,
            },
        )
        .err()
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}

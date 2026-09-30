use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crossterm::{
    cursor, execute, queue,
    style::{ContentStyle, Print, PrintStyledContent},
    terminal::{self, Clear, ClearType},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{Component, Size, component::display_text};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Glyph {
    text: String,
    style: ContentStyle,
}

#[derive(Default)]
struct Frame {
    glyphs: Vec<Glyph>,
    cursor: usize,
}

impl Frame {
    fn new(components: &[&dyn Component], size: Size) -> io::Result<Self> {
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

    /// Measure output for cursor addressing only. No terminal-height clipping,
    /// editor viewport, or replacement for the terminal's scrollback is kept.
    fn positions(&self, columns: u16) -> Vec<Position> {
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

#[derive(Clone, Copy, Default)]
struct Position {
    row: usize,
    column: usize,
}

/// Inline renderer on the normal terminal screen.
///
/// Unchanged prefixes stay in place, including text already scrolled away by the
/// terminal. Changed suffixes are printed normally with CRLF at logical newlines.
/// A resize clears the old frame at its reflowed position, then reprints the
/// complete component output once the terminal dimensions have settled.
/// Output and cursor restoration are written together, without querying the
/// terminal mid-frame. While active, the renderer must own terminal output.
pub struct Renderer {
    previous: Frame,
    size: Option<Size>,
    origin: i64,
    cursor_row: u16,
    resize_pending: bool,
}

impl Renderer {
    pub fn new() -> io::Result<Self> {
        if cursor::position()?.0 != 0 {
            execute!(io::stdout(), Print("\r\n"))?;
        }
        Ok(Self {
            previous: Frame::default(),
            size: None,
            origin: 0,
            cursor_row: 0,
            resize_pending: false,
        })
    }

    /// Notify the renderer about a resize event, including a burst which ends
    /// at the original size. Reading the final size alone cannot detect that.
    pub fn resize(&mut self) {
        self.resize_pending = true;
    }

    pub fn render(&mut self, components: &[&dyn Component]) -> io::Result<()> {
        let mut size = terminal_size()?;
        let mut resized = self.resize_pending || self.size.is_some_and(|previous| previous != size);
        let actual_row = loop {
            // Normal edits and menu navigation use the position retained from
            // the last frame. Only initial placement and reflow need a report.
            if !resized && self.size.is_some() {
                break self.cursor_row;
            }
            if resized {
                size = settled_size(size)?;
            }
            let (_, row) = cursor::position()?;
            let observed_size = terminal_size()?;
            if observed_size == size {
                break row;
            }
            size = observed_size;
            resized = true;
        };
        let next = Frame::new(components, size)?;
        let old_positions = self.previous.positions(size.columns);
        let new_positions = next.positions(size.columns);
        let reflowed_origin =
            i64::from(actual_row) - old_positions[self.previous.cursor].row as i64;
        let mut origin = if resized {
            // Retain the drawing position, but make room before printing so a
            // smaller terminal does not scroll another copy into history.
            let last_origin = i64::from(size.rows - 1) - new_positions.last().unwrap().row as i64;
            self.origin.clamp(0, last_origin.max(0))
        } else if self.size.is_some() {
            self.origin + i64::from(actual_row) - i64::from(self.cursor_row)
        } else {
            i64::from(actual_row)
        };
        // Terminal reflow can scroll the beginning away even if the input now
        // fits. Reprint it from the top instead of retaining a stale origin.
        let rebase = origin < 0 && new_positions.last().unwrap().row < usize::from(size.rows);
        let changed = resized || rebase || self.previous.glyphs != next.glyphs;
        let mut output = Vec::new();

        if changed {
            queue!(output, cursor::Hide)?;
            let common = self
                .previous
                .glyphs
                .iter()
                .zip(&next.glyphs)
                .take_while(|(a, b)| a == b)
                .count();
            // Reprint one preceding glyph to establish autowrap at boundaries.
            let mut start = if resized || rebase {
                0
            } else {
                common.saturating_sub(1)
            };
            // A newline after a full row has its logical position one column
            // past the margin. CUP cannot address that pending-wrap position:
            // clamping it would erase the last cell without repainting it.
            // Include the last printable glyph to establish the margin again.
            while start > 0 && old_positions[start].column >= usize::from(size.columns) {
                start -= 1;
            }
            let mut from = old_positions[start];
            if origin + (from.row as i64) < 0 {
                // The changed prefix has left the screen. Re-emit the full
                // input from the top; do not invent a clipped editor viewport.
                start = 0;
                from = Position::default();
                origin = 0;
            }
            if resized {
                // Reflow may have moved the OLD frame above the stored origin.
                // Erase it there before drawing at the retained position. The
                // erase origin and the new drawing origin are not interchangeable.
                queue!(
                    output,
                    cursor::MoveTo(0, screen_row(reflowed_origin.min(origin), 0, size.rows)),
                    Clear(ClearType::FromCursorDown),
                )?;
            }
            queue!(
                output,
                cursor::MoveTo(
                    from.column.min(usize::from(size.columns - 1)) as u16,
                    screen_row(origin, from.row, size.rows)
                ),
                Clear(ClearType::FromCursorDown)
            )?;
            for glyph in &next.glyphs[start..] {
                if glyph.text == "\n" {
                    queue!(output, Print("\r\n"))?;
                } else if glyph.style == ContentStyle::default() {
                    queue!(output, Print(&glyph.text))?;
                } else {
                    queue!(output, PrintStyledContent(glyph.style.apply(&glyph.text)))?;
                }
            }
            // Normal wrapping/CRLF scrolls only when the last output row passes
            // the bottom margin. Account for that locally, so returning to the
            // input does not require a round trip with the cursor on the menu.
            // A full final column is still on its row (deferred autowrap).
            origin =
                origin.min(i64::from(size.rows - 1) - new_positions.last().unwrap().row as i64);
        }

        let caret = new_positions[next.cursor];
        queue!(
            output,
            cursor::MoveTo(
                caret.column.min(usize::from(size.columns - 1)) as u16,
                screen_row(origin, caret.row, size.rows),
            )
        )?;
        if changed {
            queue!(output, cursor::Show)?;
        }
        // Include the native cursor's final position and visibility in the same
        // output buffer as the text. Never flush or wait for input mid-frame.
        let mut stdout = io::stdout().lock();
        if let Err(error) = stdout.write_all(&output).and_then(|()| stdout.flush()) {
            let _ = execute!(stdout, cursor::Show);
            self.size = None;
            return Err(error);
        }
        self.resize_pending = false;
        self.previous = next;
        self.size = Some(size);
        self.origin = origin;
        self.cursor_row = screen_row(origin, caret.row, size.rows);
        Ok(())
    }

    /// Call after rendering the editor at its end, without auxiliary components.
    pub fn finish(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Print("\r\n"))?;
        self.previous = Frame::default();
        self.size = None;
        Ok(())
    }
}

fn terminal_size() -> io::Result<Size> {
    let (columns, rows) = terminal::size()?;
    Ok(Size {
        columns: columns.max(1),
        rows: rows.max(1),
    })
}

fn settled_size(mut size: Size) -> io::Result<Size> {
    // While dragging a window edge, another reflow can occur between measuring
    // the cursor and writing the frame. Keep the existing output during the
    // burst and repaint once the geometry has been quiet for a short interval.
    let mut since = Instant::now();
    while since.elapsed() < Duration::from_millis(200) {
        std::thread::sleep(Duration::from_millis(5));
        let next = terminal_size()?;
        if next != size {
            size = next;
            since = Instant::now();
        }
    }
    Ok(size)
}

fn screen_row(origin: i64, row: usize, rows: u16) -> u16 {
    (origin + row as i64).clamp(0, i64::from(rows.saturating_sub(1))) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    #[test]
    fn a_leading_combining_character_does_not_invalidate_the_editor_cursor() {
        let mut editor = Editor::default();
        editor.insert_text("\u{301}a");
        editor.move_to(0);
        let frame = Frame::new(
            &[&editor],
            Size {
                columns: 20,
                rows: 3,
            },
        )
        .unwrap();
        let positions = frame.positions(20);
        assert_eq!(positions[frame.cursor].column, 6);
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

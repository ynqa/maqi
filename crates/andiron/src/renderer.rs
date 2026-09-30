use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crossterm::{
    cursor, execute, queue,
    style::{ContentStyle, Print, PrintStyledContent},
    terminal::{
        self, BeginSynchronizedUpdate, Clear, ClearType, DisableLineWrap, EnableLineWrap,
        EndSynchronizedUpdate,
    },
};

use crate::{
    Component, Size,
    layout::{Frame, Layout},
};

/// Ownership of an inline region on the normal terminal screen. Resizing
/// invalidates its coordinates, not its contents: the old frame remains visible
/// until an entire replacement, including the input cursor, is ready.
enum Anchor {
    Unmeasured,
    Verified(u16),
    Resized(u16),
    Checking(u16),
}

impl Anchor {
    fn row(&self) -> u16 {
        match *self {
            Self::Unmeasured => 0,
            Self::Verified(row) | Self::Resized(row) | Self::Checking(row) => row,
        }
    }
}

/// An inline painter with a bounded editing viewport. Committed input uses
/// normal terminal scrolling; uncommitted input and supporting components are
/// repainted together, without a clearing-only frame or resize debounce.
pub struct Renderer {
    anchor: Anchor,
    size: Option<Size>,
    previous: Layout,
    frame: Frame,
    pty_size: Option<Size>,
    refresh_until: Option<Instant>,
}

impl Renderer {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            anchor: Anchor::Unmeasured,
            size: None,
            previous: Layout::default(),
            frame: Frame::default(),
            pty_size: None,
            refresh_until: None,
        })
    }

    pub fn resize(&mut self) {
        self.anchor = Anchor::Resized(self.anchor.row());
        self.refresh_until = Some(Instant::now() + Duration::from_millis(250));
    }

    pub fn resize_polling(&self) -> bool {
        self.refresh_until.is_some()
    }

    pub fn refresh(&mut self, components: &[&dyn Component]) -> io::Result<()> {
        if self
            .refresh_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.refresh_until = None;
        }
        if matches!(self.anchor, Anchor::Verified(_)) {
            self.anchor = Anchor::Checking(self.anchor.row());
        }
        self.render(components)
    }

    pub fn render(&mut self, components: &[&dyn Component]) -> io::Result<()> {
        let pty_size = terminal_size()?;
        let mut size = self.size.unwrap_or(pty_size);
        if self.pty_size.is_some_and(|old| old != pty_size) {
            self.resize();
        }
        let mut origin = self.anchor.row();
        let mut geometry_changed = matches!(self.anchor, Anchor::Unmeasured | Anchor::Resized(_));
        if !matches!(self.anchor, Anchor::Verified(_)) {
            let (actual_size, column, row) = match measure_geometry() {
                Ok(report) => report,
                Err(error) if error.kind() == io::ErrorKind::TimedOut && self.size.is_some() => {
                    // Keep the last complete frame visible if the frontend is
                    // busy. Retry from the input loop, without discarding keys.
                    self.frame = Frame::new(components, size)?;
                    self.resize();
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            if self.size.is_some_and(|old| old != actual_size) {
                self.refresh_until = Some(Instant::now() + Duration::from_millis(250));
            }
            size = actual_size;
            if self.previous.rows.is_empty() {
                origin = row;
                if column != 0 {
                    execute!(io::stdout(), Print("\r\n"))?;
                    origin = (row + 1).min(size.rows - 1);
                }
            } else {
                let caret = self.previous.reflowed_cursor(size.columns);
                let distance = if usize::from(column) == caret.column {
                    caret.row
                } else {
                    // The terminal clamped a cursor whose row left the screen.
                    // In that case the report bounds the old frame's bottom,
                    // rather than identifying the logical editing position.
                    self.previous
                        .reflowed_height(size.columns)
                        .saturating_sub(1)
                };
                origin = row
                    .saturating_sub(distance.min(usize::from(u16::MAX)) as u16)
                    .min(origin);
            }
        }
        // Leave horizontal headroom while the frontend is moving. The terminal
        // may resize again between its geometry reply and processing this frame.
        // Restore the full width once the resize stream has become quiet.
        let drawing_size = Size {
            columns: if self.resize_polling() {
                size.columns.saturating_sub(8).max(1)
            } else {
                size.columns
            },
            rows: size.rows,
        };
        let frame = Frame::new(components, drawing_size)?;
        let layout = Layout::new(&frame, drawing_size);
        geometry_changed |= self.size != Some(size) || origin != self.anchor.row();
        if !geometry_changed && layout == self.previous {
            self.frame = frame;
            self.anchor = Anchor::Verified(origin);
            self.pty_size = Some(pty_size);
            return Ok(());
        }
        let mut output = Vec::new();
        queue!(
            output,
            BeginSynchronizedUpdate,
            cursor::Hide,
            DisableLineWrap
        )?;
        // Clear the old region before scrolling for space. Only committed
        // output above it may enter scrollback, never obsolete menu rows.
        if geometry_changed {
            clear_region(&mut output, origin, size.rows)?;
        }
        let extra =
            (usize::from(origin) + layout.rows.len()).saturating_sub(usize::from(size.rows));
        if extra != 0 {
            if !geometry_changed {
                clear_region(&mut output, origin, size.rows)?;
            }
            geometry_changed = true;
            queue!(output, cursor::MoveTo(0, size.rows - 1))?;
            for _ in 0..extra {
                queue!(output, Print("\r\n"))?;
            }
            origin = origin.saturating_sub(extra as u16);
        }
        if !geometry_changed {
            for index in layout.rows.len()..self.previous.rows.len() {
                queue!(
                    output,
                    cursor::MoveTo(0, origin + index as u16),
                    Clear(ClearType::CurrentLine)
                )?;
            }
        }
        for (index, row) in layout.rows.iter().enumerate() {
            if !geometry_changed && self.previous.rows.get(index) == Some(row) {
                continue;
            }
            queue!(output, cursor::MoveTo(0, origin + index as u16))?;
            if !geometry_changed {
                queue!(output, Clear(ClearType::CurrentLine))?;
            }
            paint_row(&mut output, row)?;
        }
        queue!(
            output,
            cursor::MoveTo(
                layout.cursor.column as u16,
                origin + layout.cursor.row as u16
            ),
            EnableLineWrap,
            cursor::Show,
            EndSynchronizedUpdate
        )?;
        write_frame(&output)?;
        self.anchor = Anchor::Verified(origin);
        self.size = Some(size);
        self.pty_size = Some(pty_size);
        self.previous = layout;
        self.frame = frame;
        Ok(())
    }

    /// Commit the complete logical input once, including parts outside the
    /// editing viewport, and leave it in the terminal's native scrollback.
    pub fn finish(&mut self) -> io::Result<()> {
        let size = terminal_size()?;
        let origin = self.anchor.row().min(size.rows - 1);
        let mut output = Vec::new();
        queue!(
            output,
            BeginSynchronizedUpdate,
            cursor::Hide,
            EnableLineWrap
        )?;
        clear_region(&mut output, origin, size.rows)?;
        queue!(output, cursor::MoveTo(0, origin))?;
        let end = self.frame.focused_end.unwrap_or(self.frame.glyphs.len());
        for glyph in &self.frame.glyphs[..end] {
            if glyph.text == "\n" {
                queue!(output, Print("\r\n"))?;
            } else if glyph.style == ContentStyle::default() {
                queue!(output, Print(&glyph.text))?;
            } else {
                queue!(output, PrintStyledContent(glyph.style.apply(&glyph.text)))?;
            }
        }
        queue!(output, Print("\r\n"), cursor::Show, EndSynchronizedUpdate)?;
        write_frame(&output)?;
        self.anchor = Anchor::Unmeasured;
        self.size = None;
        self.previous = Layout::default();
        self.frame = Frame::default();
        Ok(())
    }
}

fn clear_region(output: &mut Vec<u8>, origin: u16, rows: u16) -> io::Result<()> {
    // Per-row erase avoids terminal-specific clear-screen-to-scrollback
    // behavior and clears wrapping metadata along with the old cell contents.
    for row in origin..rows {
        queue!(
            output,
            cursor::MoveTo(0, row),
            Clear(ClearType::CurrentLine)
        )?;
    }
    Ok(())
}

fn write_frame(output: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let result = stdout.write_all(output).and_then(|()| stdout.flush());
    if result.is_err() {
        let _ = execute!(stdout, EnableLineWrap, cursor::Show, EndSynchronizedUpdate);
    }
    result
}

fn terminal_size() -> io::Result<Size> {
    let (columns, rows) = terminal::size()?;
    Ok(Size {
        columns: columns.max(1),
        rows: rows.max(1),
    })
}

fn measure_geometry() -> io::Result<(Size, u16, u16)> {
    crate::event::geometry()
}

fn paint_row(output: &mut Vec<u8>, row: &[crate::layout::Glyph]) -> io::Result<()> {
    let mut start = 0;
    while start < row.len() {
        let style = row[start].style;
        let mut end = start + 1;
        while end < row.len() && row[end].style == style {
            end += 1;
        }
        let text: String = row[start..end]
            .iter()
            .map(|glyph| glyph.text.as_str())
            .collect();
        if style == ContentStyle::default() {
            queue!(output, Print(text))?;
        } else {
            queue!(output, PrintStyledContent(style.apply(text)))?;
        }
        start = end;
    }
    Ok(())
}

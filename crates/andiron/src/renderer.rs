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
    layout::{Frame, Layout, Position},
};

/// Ownership of an inline region on the normal terminal screen. Resizing
/// invalidates its coordinates, not its contents: the old frame remains visible
/// until an entire replacement, including the input cursor, is ready.
enum Anchor {
    Unmeasured,
    Verified(u16),
    Resized(u16),
}

impl Anchor {
    fn row(&self) -> u16 {
        match *self {
            Self::Unmeasured => 0,
            Self::Verified(row) | Self::Resized(row) => row,
        }
    }
}

/// An inline painter with a bounded editing viewport. Committed input uses
/// normal terminal scrolling; uncommitted input and supporting components are
/// repainted together. On iTerm, resizing retains the existing terminal frame
/// until it can be replaced without leaving an offscreen copy.
pub struct Renderer {
    anchor: Anchor,
    size: Option<Size>,
    previous: Layout,
    frame: Frame,
    pty_size: Option<Size>,
    retain_reflow: bool,
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
            retain_reflow: false,
            refresh_until: None,
        })
    }

    fn resize_settle(&self) -> Duration {
        // iTerm can batch OS resize notifications at roughly 200 ms intervals.
        // Keep the existing frame until that notification stream becomes quiet.
        Duration::from_millis(if self.retain_reflow { 250 } else { 50 })
    }

    pub fn resize(&mut self) {
        self.anchor = Anchor::Resized(self.anchor.row());
        self.refresh_until = Some(Instant::now() + self.resize_settle());
    }

    /// A resize is settling, or the previous frame is partly outside the screen.
    /// Continue refreshing. Buffer input when `input_deferred()` is also true.
    pub fn resize_polling(&self) -> bool {
        self.refresh_until.is_some()
    }

    /// Retain keys while a resize is settling; on iTerm, also while the old
    /// frame extends into scrollback and cannot yet be replaced.
    pub fn input_deferred(&self) -> bool {
        self.resize_polling()
    }

    pub fn refresh(&mut self, components: &[&dyn Component]) -> io::Result<()> {
        self.render(components)
    }

    pub fn render(&mut self, components: &[&dyn Component]) -> io::Result<()> {
        self.render_in(components, &mut NativeTerminal)
    }

    fn render_in(
        &mut self,
        components: &[&dyn Component],
        terminal: &mut impl Terminal,
    ) -> io::Result<()> {
        if self
            .refresh_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.refresh_until = None;
        }
        let pty_size = terminal.size()?;
        let mut size = self.size.unwrap_or(pty_size);
        if self.pty_size.is_some_and(|old| old != pty_size) {
            self.resize();
        }
        if self.resize_polling() {
            // Drain the resize/key event stream before requesting a cursor
            // report. A query during a SIGWINCH burst can time out in crossterm.
            self.frame = Frame::new(components, pty_size)?;
            self.pty_size = Some(pty_size);
            return Ok(());
        }
        if self.retain_reflow && !self.previous.rows.is_empty() {
            let old = self.size.unwrap();
            let tail = usize::from(old.rows)
                .saturating_sub(usize::from(self.anchor.row()) + self.previous.rows.len());
            let required = self.previous.reflowed_height(pty_size.columns) + tail;
            if required > usize::from(pty_size.rows) {
                self.frame = Frame::new(components, pty_size)?;
                self.pty_size = Some(pty_size);
                self.anchor = Anchor::Resized(self.anchor.row());
                self.refresh_until = Some(Instant::now() + self.resize_settle());
                return Ok(());
            }
        }
        let mut origin = self.anchor.row();
        let mut position = self.previous.cursor;
        let mut geometry_changed = matches!(self.anchor, Anchor::Unmeasured | Anchor::Resized(_));
        if !matches!(self.anchor, Anchor::Verified(_)) {
            let (actual_size, column, row) = match terminal.geometry() {
                Ok(report) => report,
                Err(error)
                    if self.size.is_some()
                        && matches!(
                            error.kind(),
                            io::ErrorKind::TimedOut | io::ErrorKind::Other
                        ) =>
                {
                    // crossterm 0.29 uses Other for a cursor-report timeout.
                    // Keep the complete frame and drain events before retrying.
                    self.frame = Frame::new(components, size)?;
                    self.resize();
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            self.retain_reflow = terminal.retains_reflowed_frame()?;
            if actual_size != pty_size {
                // A resize arrived during the cursor query. Keep the frame and
                // let the input loop drain the new resize events before retrying.
                self.frame = Frame::new(components, actual_size)?;
                self.pty_size = Some(actual_size);
                self.resize();
                return Ok(());
            }
            size = actual_size;
            if self.previous.rows.is_empty() {
                origin = row;
                if column != 0 {
                    terminal.write(b"\r\n")?;
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
            position = Position {
                row: usize::from(row.saturating_sub(origin)),
                column: usize::from(column),
            };
        }
        // The old frame stays visible throughout the resize stream. Once the
        // geometry has settled, repaint at the full width; arbitrary headroom
        // cannot protect against a resize to a single column.
        let drawing_size = size;
        let frame = Frame::new(components, drawing_size)?;
        let layout = Layout::new(&frame, drawing_size);
        geometry_changed |= self.size != Some(size) || origin != self.anchor.row();
        // A round trip can restore the exact old frame without any output.
        // Do not clear and reprint it merely because resize events occurred.
        if layout == self.previous
            && self.size == Some(size)
            && origin == self.anchor.row()
            && position == self.previous.cursor
        {
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
        // Like zsh's moveto(), travel relative to the editing region. A CUP
        // based on a CPR can already be stale when the terminal executes it.
        // Reserve the replacement's rows before painting any text: scrolling
        // here can move committed output into history, but not the old menu.
        let rebuild = geometry_changed || layout.rows.len() > self.previous.rows.len();
        if rebuild {
            move_to(&mut output, &mut position, Position::default())?;
            queue!(output, Clear(ClearType::FromCursorDown))?;
            let last = layout.rows.len().saturating_sub(1);
            for _ in 0..last {
                queue!(output, Print("\r\n"))?;
            }
            position.row = last;
            origin = origin.min(size.rows.saturating_sub(layout.rows.len() as u16));
        } else {
            for index in layout.rows.len()..self.previous.rows.len() {
                move_to(
                    &mut output,
                    &mut position,
                    Position {
                        row: index,
                        column: 0,
                    },
                )?;
                queue!(output, Clear(ClearType::CurrentLine))?;
            }
        }
        // Paint back toward the input, ending near the native input cursor.
        // Supporting rows must never become the cursor's resting position.
        for (index, row) in layout.rows.iter().enumerate().rev() {
            if !rebuild && self.previous.rows.get(index) == Some(row) {
                continue;
            }
            move_to(
                &mut output,
                &mut position,
                Position {
                    row: index,
                    column: 0,
                },
            )?;
            if !rebuild {
                queue!(output, Clear(ClearType::CurrentLine))?;
            }
            paint_row(&mut output, row)?;
            queue!(output, Print("\r"))?;
            position.column = 0;
        }
        move_to(&mut output, &mut position, layout.cursor)?;
        queue!(output, EnableLineWrap, cursor::Show, EndSynchronizedUpdate)?;
        terminal.write(&output)?;
        self.anchor = Anchor::Verified(origin);
        self.size = Some(size);
        self.pty_size = Some(pty_size);
        self.previous = layout;
        self.frame = frame;
        Ok(())
    }

    /// Commit the complete logical input once, including parts outside the
    /// editing viewport, and leave it in the terminal's native scrollback.
    /// Returns `WouldBlock` while a resize is pending; refresh and retry rather
    /// than printing another copy over an inaccessible old frame.
    pub fn finish(&mut self) -> io::Result<()> {
        if self.input_deferred() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the previous frame must return to the visible screen before committing",
            ));
        }
        let mut output = Vec::new();
        queue!(
            output,
            BeginSynchronizedUpdate,
            cursor::Hide,
            EnableLineWrap
        )?;
        let mut position = self.previous.cursor;
        move_to(&mut output, &mut position, Position::default())?;
        queue!(output, Clear(ClearType::FromCursorDown))?;
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

// Cursor movement is relative to the region, never to a saved screen row.
fn move_to(output: &mut Vec<u8>, current: &mut Position, target: Position) -> io::Result<()> {
    queue!(output, Print("\r"))?;
    if current.row > target.row {
        queue!(output, cursor::MoveUp((current.row - target.row) as u16))?;
    } else if target.row > current.row {
        queue!(output, cursor::MoveDown((target.row - current.row) as u16))?;
    }
    if target.column != 0 {
        queue!(output, cursor::MoveRight(target.column as u16))?;
    }
    *current = target;
    Ok(())
}

trait Terminal {
    fn size(&mut self) -> io::Result<Size>;
    fn geometry(&mut self) -> io::Result<(Size, u16, u16)>;
    fn write(&mut self, output: &[u8]) -> io::Result<()>;
    fn retains_reflowed_frame(&mut self) -> io::Result<bool> {
        Ok(false)
    }
}

struct NativeTerminal;
impl Terminal for NativeTerminal {
    fn retains_reflowed_frame(&mut self) -> io::Result<bool> {
        // Use the terminal environment instead of sending identity queries that
        // crossterm does not expose. A multiplexer owns its own reflow behavior.
        Ok(std::env::var("TERM_PROGRAM").as_deref() == Ok("iTerm.app")
            && std::env::var_os("TMUX").is_none()
            && std::env::var_os("STY").is_none())
    }
    fn size(&mut self) -> io::Result<Size> {
        terminal_size()
    }
    fn geometry(&mut self) -> io::Result<(Size, u16, u16)> {
        // read/poll and position share crossterm's input reader on the same
        // thread; terminal replies cannot compete with a second TTY reader.
        let (column, row) = cursor::position()?;
        Ok((terminal_size()?, column, row))
    }
    fn write(&mut self, output: &[u8]) -> io::Result<()> {
        write_frame(output)
    }
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

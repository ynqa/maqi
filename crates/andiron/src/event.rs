//! A single input owner for user events and terminal geometry reports.
//! Terminal replies are never interpreted as editable input.
pub use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::{geometry, reset, retains_reflowed_frame};
#[cfg(unix)]
pub use unix::{poll, read};

#[cfg(not(unix))]
pub use crossterm::event::{poll, read};
#[cfg(not(unix))]
pub(crate) fn geometry() -> std::io::Result<(crate::Size, u16, u16)> {
    let (columns, rows) = crossterm::terminal::size()?;
    let (column, row) = crossterm::cursor::position()?;
    Ok((
        crate::Size {
            columns: columns.max(1),
            rows: rows.max(1),
        },
        column,
        row,
    ))
}

#[cfg(not(unix))]
pub(crate) fn reset() {}

#[cfg(not(unix))]
pub(crate) fn retains_reflowed_frame() -> std::io::Result<bool> {
    Ok(false)
}

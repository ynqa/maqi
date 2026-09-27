use std::io;

use promkit::core::crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
};

/// Restore paste mode on normal exit, errors, and panic unwinding.
pub struct BracketedPaste;

impl BracketedPaste {
    pub fn enable() -> io::Result<Self> {
        let mode = Self;
        execute!(io::stdout(), EnableBracketedPaste)?;
        Ok(mode)
    }
}

impl Drop for BracketedPaste {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste);
    }
}

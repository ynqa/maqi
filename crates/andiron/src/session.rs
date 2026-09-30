use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    style::ResetColor,
    terminal,
};
use std::io;

/// Restores terminal modes on return, I/O errors, and panic unwinding.
pub struct TerminalSession {
    restore_raw: bool,
}

impl TerminalSession {
    pub fn new() -> io::Result<Self> {
        let restore_raw = !terminal::is_raw_mode_enabled()?;
        terminal::enable_raw_mode()?;
        let session = Self { restore_raw };
        execute!(io::stdout(), Show, EnableBracketedPaste)?;
        Ok(session)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            terminal::EnableLineWrap,
            ResetColor,
            Show,
            terminal::EndSynchronizedUpdate,
            DisableBracketedPaste
        );
        if self.restore_raw {
            let _ = terminal::disable_raw_mode();
        }
    }
}

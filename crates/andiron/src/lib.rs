//! maqi's line editor and composable, inline terminal output.
//!
//! Components produce logical lines; the renderer lays out a bounded editing
//! viewport using the native input cursor. Committed input and command output
//! remain in the terminal's normal scrollback.

mod component;
mod editor;
pub mod event;
mod layout;
mod renderer;
mod session;

pub use component::{Component, Content, Line, Size, Span, TextPosition};
pub use editor::Editor;
pub use renderer::Renderer;
pub use session::TerminalSession;

pub use crossterm::style;

//! maqi's line editor and composable, inline terminal output.
//!
//! Components produce logical lines. The terminal handles wrapping and scrolling;
//! andiron does not crop the editor into a viewport or paint a replacement cursor.

mod component;
mod editor;
mod renderer;
mod session;

pub use component::{Component, Content, Line, Size, Span, TextPosition};
pub use editor::Editor;
pub use renderer::Renderer;
pub use session::TerminalSession;

pub use crossterm::{event, style};

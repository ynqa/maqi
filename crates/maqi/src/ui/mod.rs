//! Shared UI components own interaction state and implement promkit's `Widget`.
//! Domain logic supplies their data; `Readline` owns key bindings, lifecycle,
//! and application-specific effects such as replacing editor input.
//! Terminal layout and drawing remain the renderer's responsibility.

mod completion;

pub use completion::CompletionComponent;

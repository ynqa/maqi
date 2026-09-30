# andiron

The inline line editor beneath maqi. An andiron supports firewood; this crate
supports maqi's input.

`Editor` owns the input and grapheme-aware editing. maqi owns history, completion
sources, key bindings, and the decision to submit or continue a command.
`TerminalSession` enables raw input and bracketed paste and restores them on drop.

The renderer uses the terminal's native, visible cursor. It prints text across
the full terminal width, with CRLF between logical lines, and leaves wrapping
and scrolling to the terminal. It retains component output and measures character
positions to address the cursor; it does not maintain an editor viewport or a
screen-sized grid. An unchanged prefix is left in place, and a changed suffix is
cleared and printed again. Call `Renderer::resize()` when receiving a resize
event, including when a burst returns to the original dimensions. The renderer
waits for 200 ms of stable dimensions, reads the terminal cursor position to locate
and clear the old reflowed output, then reprints the complete component output.
Clearing the old location separately from the new drawing location prevents
abandoned prompts and completion rows from accumulating during width changes.

## Components

Implement `Component` to return logical `Line`s containing plain or styled
`Span`s. Pass components to `Renderer::render` in display order. Only the focused
component supplies a `TextPosition`; completion selection does not take the
terminal cursor away from the editor. Components may choose their own presentation
limits, such as maqi's five-row completion list.

```rust
use andiron::{Component, Content, Editor, Line, Size};

struct Status;

impl Component for Status {
    fn render(&self, _: Size) -> Content {
        Content {
            lines: vec![Line::plain("Ready")],
            cursor: None,
        }
    }
}

let editor = Editor::default();
let status = Status;
let components: [&dyn Component; 2] = [&editor, &status];
// With a TerminalSession and Renderer: renderer.render(&components)?;
```

## Terminal regression tests

```sh
cargo test -p andiron
cargo test -p maqi
```

Every `.th` file under `tests/scenarios/` is discovered automatically. A regression
can be captured by adding a file with the terminal size, input/resize actions,
and expected screen after each step. The test runner launches its own small
andiron fixture in a PTY. In that fixture, Enter inserts a newline, Tab toggles
two example supporting components, Escape dismisses them, and Ctrl-D exits.
For one scenario, use `cargo test -p andiron --test terminal_scenarios -- resize_roundtrip`.

Scenarios cover width/height changes, the right margin, wide and combining
characters, output taller than the screen, terminal scrollback, and attaching and
removing components. A PTY test additionally checks the actual cursor coordinates
and terminal-mode cleanup. maqi's integration scenarios exercise completion,
continuation, history, submission, and pasting through the same renderer. The
`completion_width_resize_without_input.th` regression starts at the bottom of the
screen, opens completion once, then shrinks and expands repeatedly without typing;
it also checks scrollback and editing after accepting a completion.

These checks use termharness's Alacritty terminal model. This first implementation
requires cursor-position reports and normal terminal autowrap. It does not
reconstruct scrollback after a resize or expose off-screen input through an editor
viewport: a cursor target above the screen is bounded to the top row, and changing
an off-screen prefix reprints the full input. Editing arbitrary off-screen
positions and preserving all scrollback across repeated resizing are not yet
guaranteed. Terminal-specific reflow behavior needs additional validation.

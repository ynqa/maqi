# andiron

The inline line editor beneath maqi. An andiron supports firewood; this crate
supports maqi's input.

## Display contract

andiron uses the normal terminal screen. Submitted input and command output stay
in the terminal's native scrollback; the terminal continues to own scrolling and
history. It does not enter an alternate screen or maintain a replacement history.
This shell-like behavior is the specification shared with reedline. The layout,
input routing, and painting implementation are independent.

`Editor` owns grapheme-aware input. maqi owns completion sources, history and key
bindings. Components return logical lines and at most one input cursor. The
renderer turns those lines into physical rows, keeping the editing cursor inside
a bounded viewport when the input is taller than the screen. Supporting rows are
clipped to the available width and height. Hidden input remains editable; on
submission, `Renderer::finish` writes the complete logical input to the normal
terminal exactly once. Completion rows are removed before submission.

## Painting and resize handling

The renderer retains the previous physical rows. Normal edits repaint only changed
rows. Cursor hiding, clearing, painting and restoring the input cursor are sent
in one synchronized-output transaction and one flush. Completion selection does
not become the terminal cursor. No cursor-position query occurs inside a paint
transaction, and resizing does not deliberately leave a cleared frame on screen
while waiting for the drag to finish.

A resize invalidates the tracked coordinates. The renderer measures the frontend's
size and native cursor before replacing the frame, accounting for reflow of the
rows it actually painted. On Unix, one input reader separates geometry replies
from keys and bracketed paste. This avoids multiple consumers competing for the
terminal's input stream. After a frontend answers a size query, the reader probes
its dimensions while waiting for input: some terminals update their visible grid
before delivering the corresponding PTY size notification. Unsupported size
queries fall back to the PTY dimensions. Cursor-position reports are required.

During a resize, the layout reserves eight horizontal cells to reduce the chance
of another width change wrapping a freshly painted row. Full width is restored
after 250 ms without another observed size change. This is headroom, **not a bound
on how quickly a terminal can resize**. A timed-out report retains the current
frame and is retried, rather than clearing the display or dropping pending keys.

Call `Renderer::resize()` for resize events. While `resize_polling()` is true,
poll input with a short timeout (maqi uses 16 ms), calling `refresh()` on timeout.
Use `andiron::event::{read, poll}` together on the same thread as the renderer.
The Unix reader supports UTF-8 text, ordinary editing/function keys and modifiers,
and bracketed paste; mouse and enhanced keyboard protocols are not enabled.
`TerminalSession` restores raw mode, wrapping, cursor visibility and paste mode
when dropped. Other output must be written between editing sessions, since
unrelated writes invalidate the renderer's coordinates.

## Regression tests and current limits

```sh
cargo test -p andiron
cargo test -p maqi
```

andiron automatically discovers `.th` files in `tests/scenarios/`. Its PTY fixture
uses Enter for a logical newline, Tab to toggle supporting components, Escape to
dismiss them, and Ctrl-D to finish. `Arg "--submit"` makes Enter commit the input
and start a new prompt, for native-scrollback tests.

Scenarios cover Unicode and the right margin, attaching/removing components,
resizing, editing a bounded viewport, and committing the complete input to native
scrollback. maqi scenarios also cover completion navigation, history, continuation,
and pasting. The extreme-width scenarios resize from 80 columns to one and back
in single-column steps without input between resizes. The history variant checks
that previously submitted output survives. The overflow variant permits a whole
frame's vertical translation, but rejects duplicate rows, stray text and gaps
inside the frame. A byte-stream test checks synchronized painting and the native
input cursor, which final screen snapshots alone cannot observe.

The scenarios use termharness's Alacritty model. They do not reproduce every
frontend's resize timing. In particular, rapid repeated width changes in iTerm
can still move part of the active frame into native scrollback before andiron
repaints it. Standard cursor addressing cannot selectively erase that off-screen
content without erasing history. The redesign is still under validation for this
case; passing the model tests is not a claim that the iTerm duplication is fixed.

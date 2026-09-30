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
rows. Cursor movement is relative to the editing region, following the approach
used by zsh's `moveto`, rather than absolute screen coordinates saved from a cursor
report. Rebuilds clear the visible old region and reserve space before painting;
rows are painted from the bottom back toward the input. A resize between a geometry
reply and output must not turn a stale screen row into a second prompt. Cursor hiding, clearing, painting and restoring the input cursor are sent
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

iTerm reflows the whole normal screen when its width changes, including rows
below the input. A sufficiently narrow width can move the active prompt into
native history before any application output is processed. Repainting another
prompt then leaves an inaccessible duplicate. The renderer identifies iTerm with
XTVERSION, rather than inherited environment variables, and retains its existing
frame throughout the resize stream. After 50 ms without a size change, it replaces
the frame only if the previous rows and trailing blank rows fit on the screen.
Otherwise, it waits for sufficient width to return. A round trip that restores the
same layout and cursor needs no paint output at all.

While `input_deferred()` is true, the caller must queue input in order and continue
refreshing. maqi does this for editing, completion, submission, and input intended
for the next prompt. At an extremely narrow fixed width, editing remains deferred
until the previous frame fits again. `finish()` returns `WouldBlock` if a resize
intervenes before submission; refresh and retry before processing later input.
No history is erased or scrolled by application commands to conceal old frames.

Other terminals use the relative painter without this retention policy: their
history-to-screen reflow behavior differs. The iTerm strategy is not a claim of
identical extreme-resize behavior in every emulator. A timed-out geometry report
retains the current frame and is retried.

Call `Renderer::resize()` for resize events. While `resize_polling()` is true,
poll input with a short timeout (maqi uses 16 ms) and call `refresh()` regularly.
Buffer non-resize events when `input_deferred()` is true and drain them in order
once it becomes false.
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

`tests/resize_races/*.th` adds deterministic races using termharness's parser and
screen with an in-process renderer fixture. Unlike the PTY scenarios, its `Resize`
action takes effect after the geometry reply, either before output or between
painting the menu and input (`Env RESIZE_AT "after-menu"`). These scenarios scroll
to the oldest history and back. The runner additionally checks every retained row:
all 30 committed history lines must remain in order, with only one input/menu.
The report/output scenario fails on the previous absolute-coordinate renderer.

The ordinary scenarios use termharness's Alacritty model. They cannot validate
iTerm's reflow behavior. [The optional iTerm engine tests](tests/iterm/README.md)
parse `.th` files with termharness and execute their actions against iTerm's parser
and screen, reading both visible rows and the complete retained history. They
cover single-column and coarse width streams, keeping a frame at one column,
ordered queued input, and resuming submission. Every resize-only round trip must
preserve all retained rows, including interior gaps. Old history and the boundary
with the active frame are also checked with `Scroll` snapshots.

These are engine tests, not validation of mouse dragging or visual flicker in the
iTerm GUI. The supplied zsh comparison is isolated and is not evidence that the
user's normal zsh + kubectl session exhibits the same fault.

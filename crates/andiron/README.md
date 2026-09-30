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

Terminal I/O uses crossterm on every platform: `event::read`/`poll` for input and
resize events, `terminal::size` for dimensions, and `cursor::position` for cursor
reports. There is no separate Unix TTY reader, escape-sequence parser, or signal
handler. Cursor reports and key events use crossterm's shared input reader.
The `use-dev-tty` feature selects crossterm's poll-based Unix backend to handle
simultaneous input and resize readiness; Windows retains its native backend.

A resize invalidates the tracked coordinates. The renderer keeps the current
frame while resize notifications settle (50 ms, or 250 ms for iTerm's batched OS
notifications), then measures the native cursor before replacing the frame.
Refresh ticks check the OS dimensions without sending cursor queries during the
resize stream. Once the frame is verified, unchanged refreshes need no report.

The renderer identifies iTerm through `TERM_PROGRAM=iTerm.app`, excluding tmux
and screen sessions (`TMUX`/`STY`). iTerm reflows the whole normal screen when its
width changes, including rows below the input. A sufficiently narrow width can
move the active prompt into native history before application output is processed.
The renderer retains that frame until its previous rows and trailing blank rows
fit on the screen again. A round trip that restores the same layout and cursor
needs no paint output at all. This detection depends on the terminal environment;
custom terminal launchers must preserve it correctly.

While `input_deferred()` is true, the caller must queue input in order and continue
refreshing. maqi does this for editing, completion, submission, and input intended
for the next prompt. At an extremely narrow fixed width, editing remains deferred
until the previous frame fits again. `finish()` returns `WouldBlock` if a resize
intervenes before submission; refresh and retry before processing later input.
No history is erased or scrolled by application commands to conceal old frames.

Other terminals also wait for resize notifications to settle, but do not hold
an offscreen frame indefinitely: their history-to-screen reflow behavior differs. The iTerm strategy is not a claim of
identical extreme-resize behavior in every emulator. A timed-out geometry report
retains the current frame and is retried.

Call `Renderer::resize()` for resize events. While `resize_polling()` is true,
poll input with a short timeout (maqi uses 16 ms) and call `refresh()` regularly.
Buffer non-resize events when `input_deferred()` is true and drain them in order
once it becomes false.
Use `andiron::event::{read, poll}` together on the same thread as the renderer.
`andiron::event` re-exports crossterm's event API. Do not combine this synchronous
input loop with `EventStream`. Bracketed paste is enabled; mouse and enhanced
keyboard protocols are not enabled.
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

Until release, add behavioral display regressions to the ordinary PTY `.th`
scenarios in each crate's `tests/scenarios/` directory. These run the complete
input/render loop, including resize bursts and scrollback checks. The timing of
resizes relative to cursor reports and paint bytes is controlled by the PTY and
scheduler; these scenarios do not force a resize at a particular internal
instruction or assert exact query counts during settling.

The ordinary scenarios use termharness's Alacritty model. They cannot validate
iTerm's reflow behavior. All iTerm-specific tests live in the separate `maqi.docs`
repository: `.th` scenarios in `tools/iterm/scenarios/` and their optional runner
in `tools/iterm/` (see `tools/iterm/README.md`). The runner parses those files
with termharness and executes them against iTerm's parser and screen, reading
both visible rows and the complete retained history. The scenarios cover
single-column and coarse width streams, keeping a frame at one column,
ordered queued input, and resuming submission. Every resize-only round trip must
preserve all retained rows, including interior gaps. Old history and the boundary
with the active frame are also checked with `Scroll` snapshots.

These are engine tests, not validation of mouse dragging or visual flicker in the
iTerm GUI. Add regression cases as `.th` files in that directory; no exporter,
Python driver, or Xcode bridge belongs to the andiron crate. The normal
`cargo test` commands above do not execute this optional iTerm suite.

# iTerm engine resize regressions

This optional macOS suite runs real maqi processes in PTYs and feeds their output
into iTerm's `VT100Screen` through an XCTest bridge. It does not operate an iTerm
window. The bridge reads visible cells and **all retained history rows**. A passing
XCTest only means the bridge ran; the driver determines whether assertions pass.

## Run `.th` scenarios

Use an isolated, already buildable iTerm source checkout, its dependencies, and
Xcode. The runner temporarily adds one test to
`ModernTests/iTermLineAttributeTests.swift` and restores the file in `finally`.
It does not modify the terminal engine. Avoid concurrent builds or edits of that
test file.

```sh
cargo build -p maqi
python3 crates/andiron/tests/iterm/run.py \
  --iterm /path/to/isolated/iTerm2 \
  --build-dir /path/to/DerivedData \
  --packages /path/to/Packages \
  --maqi target/debug/maqi \
  --output /tmp/new-empty-probe-directory \
  --scenario crates/andiron/tests/iterm/scenarios/*.th
```

`--output` must be empty so stale RPC responses cannot make a run pass. Requests,
responses, PTY output traces, snapshots, and engine/driver logs are retained. This
suite needs macOS/Xcode and is separate from ordinary `cargo test`.

The Rust helper uses **termharness's parser**, then exports its AST as JSON. The
Python backend executes `Input`, `Paste`, `Resize`, waits, and `Scroll` and checks
the `.th` snapshots. This fixture supports maqi commands. It starts each scenario
with a fresh simulated screen containing 30 committed history lines, `H00` through
`H29`, then places the cursor as declared in the scenario. `Scroll` inspects the
corresponding viewport of the engine's retained rows; it does not send scrolling
commands to the application or automate the GUI.

Each resize action changes the frontend before notifying the PTY, with 4 ms of
output processing before the next action (plus RPC overhead). After every
resize-only step that returns to the original dimensions, **all retained rows**
must match their pre-step contents. Only empty rows after the final content are
ignored. Duplicates, lost history, stray fragments, and interior gaps fail even
when the live snapshot looks correct. `ITERM_EXPECT_HISTORY` optionally asserts
the retained history length after scenarios that deliberately change input.

Scenarios cover:

- Four 80 → 1 → 80 round trips, in single-column increments.
- Three coarse round trips through 64, 48, 32, 16, 8, and 1 columns.
- A 350 ms pause at one column, then queued editing, cursor movement, submission,
  and input for the next prompt, followed by restoration to 80 columns.
- Oldest history and the boundary between history and the active input.

## Additional timing probes and zsh comparison

Without `--scenario`, the driver uses four timing patterns: a settled 80 → 1 → 80
jump, coarse bursts, single-column streams, and a stream with PTY size notices
delayed by up to 200 ms. `--cycles`, `--rows`, `--cursor-row` (1-based), and `--modes`
control these patterns. Use `--shell maqi` for maqi alone.

The default zsh comparison is a synthetic five-candidate menu. To use real kubectl
completion with isolated Zim-style completion settings:

```sh
# Add these options to the same run.py command (without --scenario):
--shell zsh --zsh-config crates/andiron/tests/iterm/zsh-kubectl.zshrc \
  --zsh-input $'kubectl \t'
```

This does not load the user's full startup configuration. Failures in synthetic
resize timings do not establish that the user's normal zsh + kubectl interaction
has the same problem. zsh is a reference for relative cursor movement, not a golden
terminal model. See `moveto` and the `resetneeded` / `winchanged` branch in
[zsh's refresh implementation](https://github.com/zsh-users/zsh/blob/26ba0e39b9ccfebb4ad6aa288e2e7a2e6f48b915/Src/Zle/zle_refresh.c).

## Evidence and limits

The engine source used on 2026-09-30 was iTerm commit
[`9b6084d879ee780e51f16144277ac2459e668b0d`](https://github.com/gnachman/iTerm2/tree/9b6084d879ee780e51f16144277ac2459e668b0d).
Its engine sources were compared with the source archive; none were modified.
Before the retained-frame fix, the coarse-width `.th` failed on its first round
trip: the live screen was correct, but history contained repeated prompts and
completion fragments. This is the failure the full-history assertion detects.
After the fix, all three `.th` scenarios passed. The four additional timing
patterns also preserved every retained row with a 12-row screen, input starting
at row 1, and three round trips per pattern.

The renderer now retains iTerm's reflowed frame instead of repainting another
copy while the original is in history. It defers input until sufficient width
returns, without erasing history or switching to an alternate screen. Other
emulators have different reflow rules, so this policy is selected using iTerm's
[terminal identity response](https://iterm2.com/documentation-escape-codes.html).

These tests exercise the source engine, not mouse dragging, GUI flicker, profile
settings, or every interleaving in an installed iTerm release. The ordinary
Alacritty-based termharness tests remain a separate compatibility check.

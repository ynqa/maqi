use std::time::{Duration, Instant};

use portable_pty::CommandBuilder;
use termharness::{screen::Screen, session::Session};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const HIDE: &[u8] = b"\x1b[?25l";
const SHOW: &[u8] = b"\x1b[?25h";
const QUERY: &[u8] = b"\x1b[6n";

// Snapshots alone miss both an invisible cursor waiting for a terminal reply
// and a visible cursor visiting the menu. Check the raw stream as well.
struct Replay {
    screen: Screen,
    consumed: usize,
}

impl Replay {
    fn expect_frame(
        &mut self,
        session: &Session,
        input: &str,
        selected: Option<&str>,
        caret: (usize, usize),
        may_query: bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let cursor_move = format!("\x1b[{};{}H", caret.0 + 1, caret.1 + 1);
        let output = loop {
            let output = session.output();
            let frame = &output[self.consumed..];
            if !may_query {
                assert!(
                    !frame.windows(QUERY.len()).any(|s| s == QUERY),
                    "ordinary input/menu navigation must not wait for a cursor report"
                );
            }
            let snapshot = session.screen_snapshot();
            if !frame.is_empty()
                && (frame.ends_with(SHOW)
                    || (!frame.windows(HIDE.len()).any(|s| s == HIDE)
                        && frame.ends_with(cursor_move.as_bytes())))
                && snapshot[caret.0].trim_end() == input
                && selected
                    .is_none_or(|selection| snapshot.iter().any(|line| line.starts_with(selection)))
            {
                break output;
            }
            assert!(Instant::now() < deadline, "incomplete frame: {snapshot:?}");
            std::thread::sleep(Duration::from_millis(10));
        };

        let old_row = self.screen.cursor_position().0;
        let mut visible = true;
        let mut painting = false;
        for index in self.consumed..output.len() {
            let prefix = &output[..=index];
            self.screen.process(&output[index..=index]);
            if prefix.ends_with(HIDE) {
                assert!(!painting, "nested redraw");
                painting = true;
                visible = false;
            } else if prefix.ends_with(SHOW) {
                painting = false;
                visible = true;
            } else if prefix.ends_with(QUERY) {
                assert!(!painting, "cursor report requested mid-frame");
            }
            if visible {
                let row = self.screen.cursor_position().0;
                assert!(
                    row == old_row || row == caret.0,
                    "native cursor visible on auxiliary row {row} at byte {index}"
                );
            }
        }
        assert!(visible && !painting, "cursor not restored after redraw");
        assert_eq!(self.screen.cursor_position(), caret);
        assert_eq!(self.screen.snapshot(), session.screen_snapshot());
        self.consumed = output.len();
    }
}

#[test]
fn completion_navigation_and_resize_restore_cursor_without_mid_frame_queries() -> Result {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_maqi"));
    command.env("PATH", "/nonexistent");
    // Opening the five-row menu forces a scroll, so this also checks the
    // renderer's position tracking when it cannot query the end position.
    let mut session = Session::spawn(command, 6, 80, 0, 5)?;
    let mut replay = Replay {
        screen: Screen::new_with_cursor(6, 80, 0, 5),
        consumed: 0,
    };
    replay.expect_frame(&session, "maqi>", None, (5, 6), true);
    session.write_input(b"kubectl \t")?;
    replay.expect_frame(&session, "maqi> kubectl", Some("> alpha"), (0, 14), false);

    for selection in [
        "annotate",
        "api-resources",
        "api-versions",
        "apply",
        "attach",
    ] {
        session.write_input(b"\x1b[B")?;
        replay.expect_frame(
            &session,
            "maqi> kubectl",
            Some(&format!("> {selection}")),
            (0, 14),
            false,
        );
    }
    // No input between these resizes; position queries may precede a repaint,
    // but must never interrupt it while the native cursor is hidden.
    for columns in [40, 80, 32, 80] {
        session.resize(6, columns)?;
        replay.screen.resize(6, columns);
        replay.expect_frame(&session, "maqi> kubectl", Some("> attach"), (0, 14), true);
    }
    session.write_input(b"\x1b[A")?;
    replay.expect_frame(&session, "maqi> kubectl", Some("> apply"), (0, 14), false);
    session.write_input(b"\rX")?;
    replay.expect_frame(&session, "maqi> kubectl apply X", None, (0, 21), false);
    session.write_input(b"\x03\x04")?;
    session.wait()?;
    Ok(())
}

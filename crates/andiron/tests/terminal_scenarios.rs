//! This executable doubles as a tiny PTY fixture, with no libtest output inside
//! the terminal. Every .th file in scenarios/ is discovered automatically.
use std::{
    collections::VecDeque,
    error::Error,
    path::PathBuf,
    time::{Duration, Instant},
};

use andiron::{
    Component, Content, Editor, Line, Renderer, Size, TerminalSession,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};
use portable_pty::CommandBuilder;
use termharness::{scenario, screen::Screen, session::Session};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

struct Status;
impl Component for Status {
    fn render(&self, _: Size) -> Content {
        Content {
            lines: vec![Line::plain("status: ready")],
            cursor: None,
        }
    }
}

struct Notice;
impl Component for Notice {
    fn render(&self, _: Size) -> Content {
        Content {
            lines: vec![Line::plain("notice: editable")],
            cursor: None,
        }
    }
}

fn fixture() -> Result {
    {
        let _session = TerminalSession::new()?;
        let mut editor = Editor::default();
        let mut renderer = Renderer::new()?;
        let mut extras = false;
        let mut pending = VecDeque::new();
        let submit = std::env::args().any(|arg| arg == "--submit");
        loop {
            let mut components: Vec<&dyn Component> = vec![&editor];
            if extras {
                components.extend([&Status as &dyn Component, &Notice]);
            }
            renderer.render(&components)?;
            let next = loop {
                if renderer.input_deferred() {
                    if event::poll(Duration::from_millis(16))? {
                        let next = event::read()?;
                        if matches!(next, Event::Resize(..)) {
                            renderer.resize();
                        } else {
                            pending.push_back(next);
                        }
                    }
                    renderer.refresh(&components)?;
                } else if renderer.resize_polling() && !event::poll(Duration::from_millis(16))? {
                    renderer.refresh(&components)?;
                } else {
                    break match pending.pop_front() {
                        Some(next) => next,
                        None => event::read()?,
                    };
                }
            };
            match next {
                Event::Resize(..) => renderer.resize(),
                Event::Paste(text) => editor.insert_text(&text),
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    match (key.modifiers, key.code) {
                        (KeyModifiers::CONTROL, KeyCode::Char('c' | 'd')) => {
                            editor.move_to_tail();
                            renderer.render(&[&editor])?;
                            renderer.finish()?;
                            break;
                        }
                        (_, KeyCode::Left) => editor.backward(),
                        (_, KeyCode::Right) => editor.forward(),
                        (_, KeyCode::Up) => {
                            editor.move_up();
                        }
                        (_, KeyCode::Down) => {
                            editor.move_down();
                        }
                        (_, KeyCode::Backspace) => editor.erase(),
                        (_, KeyCode::Delete) => editor.erase_forward(),
                        (_, KeyCode::Home) => editor.move_to_line_head(),
                        (_, KeyCode::End) => editor.move_to_line_tail(),
                        (_, KeyCode::Enter) if submit => {
                            renderer.finish()?;
                            editor = Editor::default();
                            extras = false;
                        }
                        (_, KeyCode::Enter) => editor.insert_newline(),
                        (_, KeyCode::Tab) => extras = !extras,
                        (_, KeyCode::Esc) => extras = false,
                        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                            editor.insert(ch)
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    assert!(!crossterm::terminal::is_raw_mode_enabled()?);
    println!("<restored>");
    Ok(())
}

fn wait_for(session: &Session, mut check: impl FnMut(&[u8]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = session.output();
        if check(&output) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "terminal did not reach expected state: {:?}",
            session.screen_snapshot()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn expect_cursor(session: &Session, row: usize, column: usize, line: &str) {
    wait_for(session, |output| {
        let mut screen = Screen::new(5, 20);
        screen.process(output);
        screen.cursor_position() == (row, column) && screen.snapshot()[row].trim_end() == line
    });
}

fn native_cursor_and_cleanup() -> Result {
    let mut command = CommandBuilder::new(std::env::current_exe()?);
    command.arg("--fixture");
    let mut session = Session::spawn(command, 5, 20, 0, 0)?;
    expect_cursor(&session, 0, 6, "maqi>");
    session.write_input(b"abc")?;
    expect_cursor(&session, 0, 9, "maqi> abc");
    session.write_input(b"\t")?;
    wait_for(&session, |_| {
        session.screen_snapshot()[2].starts_with("notice: editable")
    });
    expect_cursor(&session, 0, 9, "maqi> abc");
    session.write_input(b"\x1b[D")?;
    expect_cursor(&session, 0, 8, "maqi> abc");
    session.write_input(b"X")?;
    expect_cursor(&session, 0, 9, "maqi> abXc");
    session.write_input(b"\x04")?;
    wait_for(&session, |output| {
        output
            .windows(b"<restored>".len())
            .any(|s| s == b"<restored>")
    });
    let output = session.output();
    assert!(
        output.windows(6).rposition(|s| s == b"\x1b[?25h")
            > output.windows(6).rposition(|s| s == b"\x1b[?25l"),
        "native cursor must be visible after cleanup"
    );
    assert!(
        output.windows(8).any(|s| s == b"\x1b[?2004l"),
        "bracketed paste must be restored"
    );
    session.wait()?;
    Ok(())
}

fn main() -> Result {
    if std::env::args().nth(1).as_deref() == Some("--fixture") {
        return fixture();
    }
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios");
    let mut paths = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().is_some_and(|ext| ext == "th"));
    paths.sort();
    assert!(!paths.is_empty());
    let filter = std::env::args().skip(1).find(|arg| !arg.starts_with('-'));
    for path in paths {
        if filter
            .as_ref()
            .is_some_and(|filter| !path.file_stem().unwrap().to_string_lossy().contains(filter))
        {
            continue;
        }
        let mut ast = scenario::parser::parse(&std::fs::read_to_string(&path)?)?;
        ast.command = std::env::current_exe()?.to_string_lossy().into_owned();
        ast.args.insert(0, "--fixture".into());
        eprintln!("scenario: {}", path.display());
        scenario::run_ast(&ast)?;
    }
    if filter.is_none() {
        native_cursor_and_cleanup()?;
    }
    Ok(())
}

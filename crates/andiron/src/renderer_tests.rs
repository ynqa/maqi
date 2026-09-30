//! Deterministic report/output race tests. The .th actions use termharness's
//! parser and screen, but Resize is deliberately applied after the geometry
//! reply and immediately before the paint bytes, unlike a normal PTY scenario.
use super::*;
use crate::{Content, Editor, Line};
use termharness::{
    scenario::{
        ast::{ActionAst, InputAst, KeyAst, ScrollDirection},
        parser,
    },
    screen::Screen,
};

struct Model {
    screen: Screen,
    size: Size,
    pending: Option<Size>,
    after_menu: bool,
}
impl Terminal for Model {
    fn size(&mut self) -> io::Result<Size> {
        Ok(self.size)
    }
    fn geometry(&mut self) -> io::Result<(Size, u16, u16)> {
        let (row, col) = self.screen.cursor_position();
        let report = (self.size, col as u16, row as u16);
        if !self.after_menu
            && let Some(size) = self.pending.take()
        {
            self.screen.resize(size.rows.into(), size.columns.into());
            self.size = size;
        }
        Ok(report)
    }
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut remainder = bytes;
        if let Some(size) = self.pending.take() {
            if self.after_menu {
                let start = bytes
                    .windows(5)
                    .position(|s| s == b"alpha")
                    .expect("painted menu");
                let end = start + bytes[start..].iter().position(|&b| b == b'\r').unwrap() + 1;
                self.screen.process(&bytes[..end]);
                remainder = &bytes[end..];
            }
            self.screen.resize(size.rows.into(), size.columns.into());
            self.size = size;
        }
        self.screen.process(remainder);
        Ok(())
    }
}
struct Menu(bool);
impl Component for Menu {
    fn render(&self, _: Size) -> Content {
        Content {
            lines: vec![Line::plain(if self.0 {
                "> alpha 012345678901234567890123456789012"
            } else {
                "  alpha 012345678901234567890123456789012"
            })],
            cursor: None,
        }
    }
}

#[test]
fn resize_after_report_preserves_scrollback() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/resize_races");
    let mut paths = std::fs::read_dir(directory)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "th"))
        .collect::<Vec<_>>();
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let ast = parser::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let size = Size {
            rows: ast.terminal.rows as u16,
            columns: ast.terminal.cols as u16,
        };
        let mut model = Model {
            screen: Screen::new(ast.terminal.rows, ast.terminal.cols),
            size,
            pending: None,
            after_menu: ast
                .env
                .iter()
                .any(|(k, v)| k == "RESIZE_AT" && v == "after-menu"),
        };
        // All committed output, including lines outside the live viewport.
        for i in 0..30 {
            model.screen.process(format!("H{i:02}\r\n").as_bytes());
        }
        model.screen.process(b"\r\n\r\n\r\n\r\n");
        model
            .screen
            .process(format!("\x1b[{};{}H", ast.cursor.row, ast.cursor.col).as_bytes());
        let mut renderer = Renderer::new().unwrap();
        let mut editor = Editor::default();
        let mut menu = Menu(false);
        for step in ast.steps {
            for action in step.actions {
                let paint = match action {
                    ActionAst::Input(InputAst::Text(text)) => {
                        editor.insert_text(&text);
                        true
                    }
                    ActionAst::Input(InputAst::Key {
                        key: KeyAst::Tab, ..
                    }) => {
                        menu.0 = !menu.0;
                        true
                    }
                    ActionAst::Resize(size) => {
                        model.pending = Some(Size {
                            rows: size.rows as u16,
                            columns: size.cols as u16,
                        });
                        renderer.resize();
                        true
                    }
                    ActionAst::Scroll { direction, lines } => {
                        match direction {
                            ScrollDirection::Up => model.screen.scroll_up(lines),
                            ScrollDirection::Down => model.screen.scroll_down(lines),
                        }
                        false
                    }
                    other => panic!("unsupported race-fixture action: {other:?}"),
                };
                if paint {
                    renderer.render_in(&[&editor, &menu], &mut model).unwrap();
                    // Observe the new size, then finish settling without real-time sleeps.
                    renderer.render_in(&[&editor, &menu], &mut model).unwrap();
                    renderer.refresh_until = None;
                    renderer.render_in(&[&editor, &menu], &mut model).unwrap();
                }
            }
            assert_eq!(
                model.screen.snapshot(),
                step.expect,
                "{}: {}",
                path.display(),
                step.label
            );
        }
        // Walk every retained row, not just a representative history marker.
        model.screen.scroll_up(u16::MAX);
        let mut rows = model.screen.snapshot();
        let mut last = rows.clone();
        for _ in 0..100 {
            model.screen.scroll_down(1);
            let next = model.screen.snapshot();
            if next == last {
                break;
            }
            rows.push(next.last().unwrap().clone());
            last = next;
        }
        let nonempty = rows
            .iter()
            .map(|row| row.trim())
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>();
        let mut expected = (0..30).map(|i| format!("H{i:02}")).collect::<Vec<_>>();
        expected.extend([
            "maqi> kubectl".into(),
            if menu.0 {
                "> alpha 0123456789012345678901"
            } else {
                "alpha 0123456789012345678901"
            }
            .into(),
        ]);
        assert_eq!(nonempty, expected, "complete retained history");
        assert_eq!(model.screen.cursor_position(), (5, 14));
    }
}

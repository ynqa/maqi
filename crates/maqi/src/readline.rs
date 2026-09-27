use std::io;

use promkit::{
    core::{
        crossterm::{
            cursor,
            event::{Event, KeyCode, KeyEventKind, KeyModifiers},
            execute,
            style::{Color, ContentStyle, Print},
        },
        render::Renderer,
        Widget, WidgetPosition,
    },
    widgets::text_editor,
};

use crate::continuation::needs_continuation;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Submit(String),
    Cancel,
    Exit,
}

pub struct Readline {
    pub editor: text_editor::State,
}

impl Default for Readline {
    fn default() -> Self {
        Self {
            editor: text_editor::State {
                config: text_editor::Config {
                    prefix: "maqi> ".into(),
                    continuation_prefix: "...> ".into(),
                    active_char_style: ContentStyle {
                        background_color: Some(Color::DarkCyan),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }
}

impl Readline {
    pub fn handle_event(&mut self, event: Event) -> Action {
        let Event::Key(key) = event else {
            return Action::Continue;
        };
        if key.kind == KeyEventKind::Release {
            return Action::Continue;
        }

        let editor = &mut self.editor.texteditor;
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Action::Cancel,
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                if editor.text_without_cursor().is_empty() {
                    return Action::Exit;
                }
                editor.erase_forward();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let text = editor.text_without_cursor().to_string();
                if needs_continuation(&text) {
                    editor.insert_newline();
                } else {
                    return Action::Submit(text);
                }
            }
            (KeyModifiers::NONE, KeyCode::Left) => {
                editor.backward();
            }
            (KeyModifiers::NONE, KeyCode::Right) => {
                editor.forward();
            }
            (KeyModifiers::NONE, KeyCode::Up) => {
                editor.move_up();
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                editor.move_down();
            }
            (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                editor.move_to_line_head()
            }
            (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                editor.move_to_line_tail()
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => editor.erase(),
            (KeyModifiers::NONE, KeyCode::Delete) => editor.erase_forward(),
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => editor.erase_all(),
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(ch)) => editor.insert(ch),
            _ => {}
        }

        Action::Continue
    }

    /// Leave the cursor below the entire input, including when it ends on the
    /// bottom row or was submitted while editing an earlier line.
    pub async fn finish(&mut self, renderer: &Renderer<()>) -> anyhow::Result<()> {
        self.editor.texteditor.move_to_tail();
        self.editor.config.active_char_style = ContentStyle::default();
        let content = self.editor.create_graphemes();
        let cursor = content.cursor.expect("the editor always has a cursor");
        renderer.update([((), content)]).render().await?;
        let position = renderer
            .screen_position(WidgetPosition {
                index: (),
                row: cursor.row,
                column: cursor.column,
            })
            .ok_or_else(|| anyhow::anyhow!("input cursor is outside the rendered viewport"))?;
        execute!(io::stdout(), cursor::MoveTo(0, position.row), Print("\r\n"))?;
        Ok(())
    }
}

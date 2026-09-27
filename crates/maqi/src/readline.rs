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
    history: Vec<String>,
    history_position: Option<usize>,
    draft: text_editor::TextEditor,
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
            history: Vec::new(),
            history_position: None,
            draft: text_editor::TextEditor::default(),
        }
    }
}

impl Readline {
    pub fn reset_input(&mut self) {
        self.editor.texteditor = text_editor::TextEditor::default();
        self.history_position = None;
        self.draft = text_editor::TextEditor::default();
    }

    pub fn handle_event(&mut self, event: Event) -> Action {
        let key = match event {
            Event::Paste(text) => {
                // Terminals may use CR, LF, or CRLF for pasted line endings.
                // Insert the entire payload without interpreting it as keys.
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                for ch in text.chars() {
                    self.editor.texteditor.insert(ch);
                }
                return Action::Continue;
            }
            Event::Key(key) => key,
            _ => return Action::Continue,
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
                    if !text.trim().is_empty() {
                        self.history.push(text.clone());
                    }
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
                if !editor.move_up() {
                    self.previous_history();
                }
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                if !editor.move_down() {
                    self.next_history();
                }
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

    fn previous_history(&mut self) {
        let Some(position) = self
            .history_position
            .unwrap_or(self.history.len())
            .checked_sub(1)
        else {
            return;
        };
        if self.history_position.is_none() {
            self.draft = self.editor.texteditor.clone();
        }
        self.history_position = Some(position);
        self.editor.texteditor.replace(&self.history[position]);
    }

    fn next_history(&mut self) {
        let Some(position) = self.history_position else {
            return;
        };
        let next = position + 1;
        if next < self.history.len() {
            self.history_position = Some(next);
            self.editor.texteditor.replace(&self.history[next]);
        } else {
            self.history_position = None;
            self.editor.texteditor = self.draft.clone();
        }
    }

    /// Leave the cursor below the entire input, including when it ends on the
    /// bottom row or was submitted while editing an earlier line.
    pub async fn finish(&mut self, renderer: &Renderer<()>) -> anyhow::Result<()> {
        self.editor.texteditor.move_to_tail();
        let active_char_style = std::mem::take(&mut self.editor.config.active_char_style);
        let content = self.editor.create_graphemes();
        self.editor.config.active_char_style = active_char_style;
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

#[cfg(test)]
mod tests {
    use super::*;
    use promkit::core::crossterm::event::KeyEvent;

    fn key(readline: &mut Readline, code: KeyCode) -> Action {
        readline.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn text(readline: &Readline) -> String {
        readline.editor.texteditor.text_without_cursor().to_string()
    }

    #[test]
    fn empty_history_preserves_input_and_cursor() {
        let mut readline = Readline::default();
        readline.handle_event(Event::Paste("draft".into()));
        key(&mut readline, KeyCode::Left);
        key(&mut readline, KeyCode::Up);
        key(&mut readline, KeyCode::Down);
        key(&mut readline, KeyCode::Char('X'));
        assert_eq!(text(&readline), "drafXt");
    }

    #[test]
    fn blank_and_cancelled_inputs_do_not_enter_history() {
        let mut readline = Readline::default();
        readline.handle_event(Event::Paste("saved".into()));
        assert_eq!(
            key(&mut readline, KeyCode::Enter),
            Action::Submit("saved".into())
        );
        readline.reset_input();
        readline.handle_event(Event::Paste(" \n ".into()));
        key(&mut readline, KeyCode::Enter);
        readline.reset_input();
        readline.handle_event(Event::Paste("cancelled".into()));
        assert_eq!(
            readline.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Action::Cancel,
        );
        readline.reset_input();
        key(&mut readline, KeyCode::Up);
        assert_eq!(text(&readline), "saved");
        key(&mut readline, KeyCode::Up);
        assert_eq!(text(&readline), "saved");
    }

    #[test]
    fn multiline_draft_is_restored_at_its_original_cursor() {
        let mut readline = Readline::default();
        readline.handle_event(Event::Paste("saved".into()));
        key(&mut readline, KeyCode::Enter);
        readline.reset_input();
        readline.handle_event(Event::Paste("first\nlast".into()));
        key(&mut readline, KeyCode::Up);
        let position = readline.editor.texteditor.position();
        key(&mut readline, KeyCode::Up);
        assert_eq!(text(&readline), "saved");
        key(&mut readline, KeyCode::Down);
        assert_eq!(text(&readline), "first\nlast");
        assert_eq!(readline.editor.texteditor.position(), position);
        key(&mut readline, KeyCode::Char('X'));
        assert_eq!(text(&readline), "firsXt\nlast");
    }
}

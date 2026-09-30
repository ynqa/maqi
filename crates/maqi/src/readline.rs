use std::ops::Range;

use andiron::{
    Component, Editor, Renderer,
    event::{Event, KeyCode, KeyEventKind, KeyModifiers},
};

use crate::completion;
use crate::continuation::needs_continuation;
use crate::ui::CompletionComponent;

struct CompletionSession {
    range: Range<usize>,
    candidates: CompletionComponent,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Submit(String),
    Cancel,
    Exit,
}

#[derive(Default)]
pub struct Readline {
    pub editor: Editor,
    history: Vec<String>,
    history_position: Option<usize>,
    draft: Editor,
    completion: Option<CompletionSession>,
}

impl Readline {
    pub fn reset_input(&mut self) {
        self.editor = Editor::default();
        self.history_position = None;
        self.draft = Editor::default();
        self.completion = None;
    }

    pub fn handle_event(&mut self, event: Event) -> Action {
        let key = match event {
            Event::Paste(text) => {
                self.completion = None;
                self.editor.insert_text(&text);
                return Action::Continue;
            }
            Event::Key(key) => key,
            _ => return Action::Continue,
        };
        if key.kind == KeyEventKind::Release {
            return Action::Continue;
        }

        if key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE {
            if let Some(menu) = &mut self.completion {
                menu.candidates.forward();
            } else if let Some(result) =
                completion::complete(self.editor.text(), self.editor.position())
            {
                if result.candidates.len() == 1 {
                    self.apply_completion(result.range, &result.candidates[0].value);
                } else {
                    self.completion = Some(CompletionSession {
                        range: result.range,
                        candidates: CompletionComponent::new(result.candidates),
                    });
                }
            }
            return Action::Continue;
        }
        if let Some(menu) = &mut self.completion {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Down) => {
                    menu.candidates.forward();
                    return Action::Continue;
                }
                (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                    menu.candidates.backward();
                    return Action::Continue;
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    let menu = self.completion.take().unwrap();
                    if let Some(candidate) = menu.candidates.selected() {
                        self.apply_completion(menu.range, &candidate.value);
                    }
                    return Action::Continue;
                }
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    self.completion = None;
                    return Action::Continue;
                }
                _ => self.completion = None,
            }
        }

        let editor = &mut self.editor;
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Action::Cancel,
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                if editor.text().is_empty() {
                    return Action::Exit;
                }
                editor.erase_forward();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let text = editor.text().to_string();
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
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => editor.clear(),
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(ch)) => editor.insert(ch),
            _ => {}
        }

        Action::Continue
    }

    fn apply_completion(&mut self, range: Range<usize>, value: &str) {
        let editor = &mut self.editor;
        let text: Vec<_> = editor.text().chars().collect();
        let mut replacement = value.to_owned();
        if text.get(range.end).is_none_or(|ch| !ch.is_whitespace()) {
            replacement.push(' ');
        }
        let updated: String = text[..range.start]
            .iter()
            .chain(replacement.chars().collect::<Vec<_>>().iter())
            .chain(text[range.end..].iter())
            .collect();
        editor.replace(&updated);
        editor.move_to(range.start + replacement.chars().count());
    }

    pub fn refresh(&self, renderer: &mut Renderer) -> std::io::Result<()> {
        let mut components: Vec<&dyn Component> = vec![&self.editor];
        if let Some(menu) = &self.completion {
            components.push(&menu.candidates);
        }
        renderer.refresh(&components)
    }

    pub fn render(&self, renderer: &mut Renderer) -> std::io::Result<()> {
        let mut components: Vec<&dyn Component> = vec![&self.editor];
        if let Some(menu) = &self.completion {
            components.push(&menu.candidates);
        }
        renderer.render(&components)
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
            self.draft = self.editor.clone();
        }
        self.history_position = Some(position);
        self.editor.replace(&self.history[position]);
    }

    fn next_history(&mut self) {
        let Some(position) = self.history_position else {
            return;
        };
        let next = position + 1;
        if next < self.history.len() {
            self.history_position = Some(next);
            self.editor.replace(&self.history[next]);
        } else {
            self.history_position = None;
            self.editor = self.draft.clone();
        }
    }

    /// Remove supporting components and leave the native cursor below the input.
    pub fn finish(&mut self, renderer: &mut Renderer) -> std::io::Result<()> {
        self.editor.move_to_tail();
        renderer.render(&[&self.editor])?;
        renderer.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andiron::event::KeyEvent;

    fn key(readline: &mut Readline, code: KeyCode) -> Action {
        readline.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn text(readline: &Readline) -> String {
        readline.editor.text().to_string()
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
        let position = readline.editor.position();
        key(&mut readline, KeyCode::Up);
        assert_eq!(text(&readline), "saved");
        key(&mut readline, KeyCode::Down);
        assert_eq!(text(&readline), "first\nlast");
        assert_eq!(readline.editor.position(), position);
        key(&mut readline, KeyCode::Char('X'));
        assert_eq!(text(&readline), "firsXt\nlast");
    }
}

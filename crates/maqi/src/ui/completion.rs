use andiron::{
    Component, Content, Line, Size, Span,
    style::{Color, ContentStyle},
};

use crate::completion::Candidate;

/// Owns candidate selection and rendering independently of the candidate source
/// and the editor's replacement range.
pub struct CompletionComponent {
    candidates: Vec<Candidate>,
    selected: Option<usize>,
}

impl CompletionComponent {
    pub fn new(candidates: Vec<Candidate>) -> Self {
        let selected = (!candidates.is_empty()).then_some(0);
        Self {
            candidates,
            selected,
        }
    }

    pub fn selected(&self) -> Option<&Candidate> {
        self.selected.and_then(|index| self.candidates.get(index))
    }

    pub fn forward(&mut self) {
        if let Some(selected) = self.selected {
            self.selected = Some((selected + 1) % self.candidates.len());
        }
    }

    pub fn backward(&mut self) {
        if let Some(selected) = self.selected {
            self.selected = Some(if selected == 0 {
                self.candidates.len() - 1
            } else {
                selected - 1
            });
        }
    }
}

impl Component for CompletionComponent {
    fn render(&self, size: Size) -> Content {
        let column_width = self
            .candidates
            .iter()
            .map(|candidate| Line::plain(&candidate.value).width())
            .max()
            .unwrap_or(0);
        let help_style = ContentStyle {
            foreground_color: Some(Color::DarkGrey),
            ..Default::default()
        };
        let limit = 5.min(usize::from(size.rows.saturating_sub(1)));
        let start = self
            .selected
            .unwrap_or(0)
            .saturating_sub(limit.saturating_sub(1));
        let lines = self
            .candidates
            .iter()
            .enumerate()
            .skip(start)
            .take(limit)
            .map(|(index, candidate)| {
                let value = Line::plain(&candidate.value);
                let padding = column_width - value.width() + 2;
                let mut line = Line::plain(format!(
                    "{}{}",
                    if Some(index) == self.selected {
                        "> "
                    } else {
                        "  "
                    },
                    candidate.value
                ));
                let help = candidate
                    .help
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !help.is_empty() {
                    line.spans.push(Span::plain(" ".repeat(padding)));
                    line.spans.push(Span::styled(help, help_style));
                }
                line.truncate(usize::from(size.columns));
                line
            })
            .collect();
        // Selection belongs to the menu; the terminal cursor stays in the editor.
        Content {
            lines,
            cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wraps_in_both_directions() {
        let mut component = CompletionComponent::new(
            ["first", "second", "third"]
                .into_iter()
                .map(|value| Candidate {
                    value: value.into(),
                    help: String::new(),
                })
                .collect(),
        );
        assert_eq!(component.selected().unwrap().value, "first");
        component.backward();
        assert_eq!(component.selected().unwrap().value, "third");
        component.forward();
        assert_eq!(component.selected().unwrap().value, "first");
        component.forward();
        assert_eq!(component.selected().unwrap().value, "second");
        component.backward();
        assert_eq!(component.selected().unwrap().value, "first");
    }

    #[test]
    fn empty_candidates_have_no_selection_or_content() {
        let mut component = CompletionComponent::new(vec![]);
        component.forward();
        component.backward();
        assert_eq!(component.selected(), None);
        let content = component.render(Size {
            columns: 80,
            rows: 24,
        });
        assert!(content.lines.is_empty());
        assert_eq!(content.cursor, None);
    }

    #[test]
    fn aligns_help_by_display_width_and_keeps_it_on_one_line() {
        let mut component = CompletionComponent::new(vec![
            Candidate {
                value: "\u{754c}".into(),
                help: " First\n  line\tcontinued ".into(),
            },
            Candidate {
                value: "abc".into(),
                help: "Second line".into(),
            },
            Candidate {
                value: "bare".into(),
                help: " \n ".into(),
            },
        ]);
        component.forward();
        let content = component.render(Size {
            columns: 80,
            rows: 24,
        });
        assert_eq!(
            content
                .lines
                .iter()
                .map(Line::text)
                .collect::<Vec<_>>()
                .join("\n"),
            "  \u{754c}    First line continued\n> abc   Second line\n  bare"
        );
        assert_eq!(content.cursor, None);
        assert_eq!(
            content.lines[0]
                .spans
                .last()
                .unwrap()
                .style
                .foreground_color,
            Some(Color::DarkGrey)
        );
    }
}

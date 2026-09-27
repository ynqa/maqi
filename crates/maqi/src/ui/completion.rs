use promkit::core::{
    ContentPosition, CreatedGraphemes, Widget, WidgetLayout, WidthMode,
    crossterm::style::{Color, ContentStyle},
    grapheme::StyledGraphemes,
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

impl Widget for CompletionComponent {
    fn create_graphemes(&self) -> CreatedGraphemes {
        let values: Vec<_> = self
            .candidates
            .iter()
            .map(|candidate| StyledGraphemes::from(candidate.value.as_str()))
            .collect();
        let column_width = values
            .iter()
            .map(StyledGraphemes::widths)
            .max()
            .unwrap_or(0);
        let help_style = ContentStyle {
            foreground_color: Some(Color::DarkGrey),
            ..Default::default()
        };
        let lines = self.candidates.iter().zip(values).enumerate().map(
            |(index, (candidate, mut value))| {
                let mut line = StyledGraphemes::from(if Some(index) == self.selected {
                    "> "
                } else {
                    "  "
                });
                let padding = column_width - value.widths() + 2;
                line.append(&mut value);
                let help = candidate
                    .help
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !help.is_empty() {
                    line.append(&mut StyledGraphemes::from(" ".repeat(padding)));
                    line.append(&mut StyledGraphemes::from_str(help, help_style));
                }
                line
            },
        );
        CreatedGraphemes {
            graphemes: StyledGraphemes::from_lines(lines),
            layout: WidgetLayout {
                max_height: Some(5),
                width_mode: WidthMode::Truncate,
                ..Default::default()
            },
            cursor: self.selected.map(|row| ContentPosition { row, column: 0 }),
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
        let content = component.create_graphemes();
        assert!(content.graphemes.is_empty());
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
        let content = component.create_graphemes();
        assert_eq!(
            content.graphemes.to_string(),
            "  \u{754c}    First line continued\n> abc   Second line\n  bare"
        );
        assert_eq!(content.cursor, Some(ContentPosition { row: 1, column: 0 }));
        let expected_help = StyledGraphemes::from_str(
            "First line continued",
            ContentStyle {
                foreground_color: Some(Color::DarkGrey),
                ..Default::default()
            },
        );
        let actual_help: StyledGraphemes =
            content.graphemes.iter().skip(7).take(20).cloned().collect();
        assert_eq!(actual_help, expected_help);
    }
}

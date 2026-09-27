use promkit::core::{
    ContentPosition, CreatedGraphemes, Widget, WidgetLayout, WidthMode,
    crossterm::style::{Color, ContentStyle},
    grapheme::StyledGraphemes,
};

use crate::completion::Completion;

pub struct CompletionMenu {
    pub result: Completion,
    pub selected: usize,
}

impl Widget for CompletionMenu {
    fn create_graphemes(&self) -> CreatedGraphemes {
        let values: Vec<_> = self
            .result
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
        let lines = self.result.candidates.iter().zip(values).enumerate().map(
            |(index, (candidate, mut value))| {
                let mut line =
                    StyledGraphemes::from(if index == self.selected { "> " } else { "  " });
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
            cursor: self
                .result
                .candidates
                .get(self.selected)
                .map(|_| ContentPosition {
                    row: self.selected,
                    column: 0,
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::Candidate;

    #[test]
    fn aligns_help_by_display_width_and_keeps_it_on_one_line() {
        let menu = CompletionMenu {
            result: Completion {
                range: 0..0,
                candidates: vec![
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
                ],
            },
            selected: 1,
        };
        let content = menu.create_graphemes();
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

    #[test]
    fn empty_menu_has_no_cursor_or_content() {
        let menu = CompletionMenu {
            result: Completion {
                range: 0..0,
                candidates: vec![],
            },
            selected: 0,
        };
        let content = menu.create_graphemes();
        assert!(content.graphemes.is_empty());
        assert_eq!(content.cursor, None);
    }
}

/// A small input-completeness check, not a shell parser.
/// Keep the original text intact: parsing and execution are separate future steps.
pub fn needs_continuation(input: &str) -> bool {
    let mut quote = None;
    let mut escaped = false;
    let mut trailing_pipe = false;

    for ch in input.chars() {
        if escaped {
            escaped = false;
            if ch != '\n' {
                trailing_pipe = false;
            }
            continue;
        }

        match (quote, ch) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), _) => {}
            (_, '\\') => escaped = true,
            (Some('"'), _) => {}
            (None, '\'' | '"') => {
                quote = Some(ch);
                trailing_pipe = false;
            }
            (None, '|') => trailing_pipe = true,
            (None, ch) if ch.is_whitespace() => {}
            _ => trailing_pipe = false,
        }
    }

    escaped || quote.is_some() || trailing_pipe
}

#[cfg(test)]
mod tests {
    use super::needs_continuation;

    #[test]
    fn continues_unescaped_backslashes_and_pipes() {
        for input in [
            "echo hello \\",
            "echo hello |",
            "echo hello |  ",
            "echo hello |\n",
            "echo hello |\ncat |",
        ] {
            assert!(needs_continuation(input), "{input:?}");
        }
    }

    #[test]
    fn submits_completed_commands_and_literal_operators() {
        for input in [
            "",
            "echo hello",
            "echo hello \\\nworld",
            "echo hello |\ncat",
            r"echo \\",
            r"echo \|",
            "echo '|'",
            "echo \"|\"",
            "echo '\\'",
            "echo hello \\ ",
        ] {
            assert!(!needs_continuation(input), "{input:?}");
        }
    }

    #[test]
    fn continues_open_quotes_until_closed() {
        assert!(needs_continuation("echo 'hello"));
        assert!(needs_continuation("echo \"hello"));
        assert!(!needs_continuation("echo 'hello\nworld'"));
        assert!(!needs_continuation("echo \"hello\nworld\""));
    }
}

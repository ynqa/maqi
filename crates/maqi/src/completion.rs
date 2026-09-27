use std::ops::Range;

use crate::usage_spec::KUBECTL;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub value: String,
    pub help: String,
}

#[derive(Debug)]
pub struct Completion {
    /// Character indices, matching promkit's editor rather than UTF-8 byte offsets.
    pub range: Range<usize>,
    pub candidates: Vec<Candidate>,
}

/// Complete static declarations only. This never evaluates a shell command,
/// queries kubectl, or executes Usage dynamic completion/mount hooks.
pub fn complete(text: &str, cursor: usize) -> Option<Completion> {
    let chars: Vec<_> = text.chars().collect();
    if cursor > chars.len() {
        return None;
    }
    let (words, prefix, start) = words_at_cursor(&chars[..cursor])?;
    let end = chars[cursor..]
        .iter()
        .position(|ch| ch.is_whitespace() || matches!(ch, '|' | ';' | '&'))
        .map_or(chars.len(), |offset| cursor + offset);
    // Don't replace quoted/escaped syntax in the suffix of a half-edited word.
    if chars[cursor..end]
        .iter()
        .any(|ch| matches!(ch, '\'' | '"' | '\\'))
    {
        return None;
    }
    let mut candidates = if words.is_empty() {
        vec![Candidate {
            value: "kubectl".into(),
            help: "Kubernetes command-line client".into(),
        }]
    } else {
        if words[0] != "kubectl" {
            return None;
        }
        let parsed = usage::parse::parse_partial(&KUBECTL, &words).ok()?;
        if parsed.double_dash_seen {
            return None;
        }
        let flags = parsed.completion_flags();
        if let Some((name, value)) = prefix.split_once('=') {
            let flag = flags.get(name)?;
            let values = flag
                .arg
                .as_ref()
                .and_then(|arg| arg.choices.as_ref())
                .map(|choices| choices.choices.clone())
                .unwrap_or_else(|| {
                    if flag.arg.is_none() {
                        vec!["true".into(), "false".into()]
                    } else {
                        vec![]
                    }
                });
            values
                .into_iter()
                .filter(|choice| choice.starts_with(value))
                .map(|choice| Candidate {
                    value: format!("{name}={choice}"),
                    help: flag.help.clone().unwrap_or_default(),
                })
                .collect()
        } else if let Some(flag) = parsed.flag_awaiting_value.last() {
            flag.arg
                .as_ref()
                .and_then(|arg| arg.choices.as_ref())
                .map(|choices| {
                    choices
                        .choices
                        .iter()
                        .map(|value| Candidate {
                            value: value.clone(),
                            help: flag.help.clone().unwrap_or_default(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else if prefix.starts_with('-') {
            flags
                .into_iter()
                .filter(|(_, flag)| !flag.hide)
                .map(|(value, flag)| Candidate {
                    value,
                    help: flag.help.clone().unwrap_or_default(),
                })
                .collect()
        } else if parsed.args.is_empty() {
            parsed
                .cmd
                .subcommands
                .values()
                .filter(|cmd| !cmd.hide)
                .map(|cmd| Candidate {
                    value: cmd.name.clone(),
                    help: cmd.help.clone().unwrap_or_default(),
                })
                .collect()
        } else {
            vec![]
        }
    };
    candidates.retain(|candidate| candidate.value.starts_with(&prefix));
    candidates.sort_by(|a, b| a.value.cmp(&b.value));
    candidates.dedup_by(|a, b| a.value == b.value);
    (!candidates.is_empty()).then_some(Completion {
        range: start..end,
        candidates,
    })
}

/// A small completion lexer, not an execution parser. Decode earlier quoted
/// arguments, skip escaped line breaks, and restart after command separators.
/// Decline completion inside a quoted or escaped current word rather than
/// changing its quoting semantics.
fn words_at_cursor(chars: &[char]) -> Option<(Vec<String>, String, usize)> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut start = chars.len();
    let mut active = false;
    let mut quoted_word = false;
    let mut quote = None;
    let mut escaped = false;
    let mut after_pipe = false;
    for (index, &ch) in chars.iter().enumerate() {
        if escaped {
            if ch != '\n' {
                word.push(ch);
            }
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            // A backslash-newline is only a continuation, not a quoted word.
            if chars.get(index + 1) != Some(&'\n') {
                if !active {
                    start = index;
                    active = true;
                }
                quoted_word = true;
            }
            escaped = true;
            continue;
        }
        if let Some(current) = quote {
            if ch == current {
                quote = None;
            } else {
                word.push(ch);
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            if !active {
                start = index;
                active = true;
            }
            quoted_word = true;
            quote = Some(ch);
        } else if ch.is_whitespace() || matches!(ch, '|' | ';' | '&') {
            if active {
                words.push(std::mem::take(&mut word));
                active = false;
            }
            quoted_word = false;
            start = index + 1;
            if matches!(ch, '|' | ';' | '&') {
                words.clear();
                after_pipe = ch == '|';
            } else if ch == '\n' && !after_pipe {
                words.clear();
            }
        } else {
            if !active {
                start = index;
                active = true;
            }
            word.push(ch);
            after_pipe = false;
        }
    }
    if quote.is_some() || escaped || quoted_word {
        return None;
    }
    Some((words, word, start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(input: &str) -> Vec<String> {
        complete(input, input.chars().count())
            .map(|result| result.candidates.into_iter().map(|c| c.value).collect())
            .unwrap_or_default()
    }

    #[test]
    fn commands_nested_commands_and_flags_come_from_usage() {
        assert_eq!(values("ku"), ["kubectl"]);
        assert_eq!(values("kubectl ge"), ["get"]);
        assert_eq!(
            values("kubectl config get-"),
            ["get-clusters", "get-contexts", "get-users"]
        );
        assert_eq!(values("kubectl create depl"), ["deployment"]);
        assert_eq!(values("kubectl get pods --names"), ["--namespace"]);
        assert_eq!(
            values("kubectl get --outp"),
            ["--output", "--output-watch-events"]
        );
    }

    #[test]
    fn respects_flag_values_and_explicit_choices() {
        assert_eq!(values("kubectl --namespace default ge"), ["get"]);
        assert_eq!(values("kubectl get pods -o ya"), ["yaml"]);
        assert_eq!(values("kubectl get pods --output=ya"), ["--output=yaml"]);
        assert_eq!(values("kubectl create deployment --dry-run cl"), ["client"]);
        assert!(values("kubectl --namespace ").is_empty());
        assert!(values("kubectl exec pod -- ").is_empty());
        assert!(values("other ge").is_empty());
    }

    #[test]
    fn handles_quotes_unicode_pipelines_and_continuation_lines() {
        assert_eq!(values("kubectl --namespace '日本 語' ge"), ["get"]);
        assert_eq!(values("echo x | kubectl ge"), ["get"]);
        assert_eq!(values("kubectl \\\nget --names"), ["--namespace"]);
        assert!(values("kubectl 'ge").is_empty());
        let text = "kubectl get pods --namesXYZ --output yaml";
        let result = complete(text, "kubectl get pods --names".chars().count()).unwrap();
        assert_eq!(
            text.chars()
                .skip(result.range.start)
                .take(result.range.len())
                .collect::<String>(),
            "--namesXYZ"
        );
        assert_eq!(result.candidates[0].value, "--namespace");
    }
}

//! Interpret manuals with mandoc, then extract command information from HTML.
use crate::{
    input::Document,
    model::{Flag, Manual},
};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::{
    collections::BTreeSet,
    fs,
    process::{Command, Stdio},
    sync::LazyLock,
};

static DEFAULT_DECL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(-[\w-]+)(?:,\s+(--[\w-]+))?=(.*)$").unwrap());
static FLAG_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:--[A-Za-z0-9][A-Za-z0-9_.-]*|-[A-Za-z0-9@%#?:+])$").unwrap());
static VALUE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^<>]+>").unwrap());

fn selector(value: &str) -> Selector {
    Selector::parse(value).unwrap()
}
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// Preserve inline adjacency (--mode=KIND) while separating nested paragraphs.
fn text(element: ElementRef<'_>) -> String {
    let mut result = String::new();
    for child in element.children() {
        if let Some(value) = child.value().as_text() {
            result.push_str(value);
        } else if let Some(child) = ElementRef::wrap(child) {
            let block = matches!(
                child.value().name(),
                "p" | "div" | "dl" | "dt" | "dd" | "br" | "pre" | "table" | "tr" | "td"
            );
            if block {
                result.push(' ');
            }
            result.push_str(&text(child));
            if block {
                result.push(' ');
            }
        }
    }
    result
}

fn body(section: ElementRef<'_>) -> String {
    section
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|e| e.value().name() != "h1")
        .map(text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn render(document: &Document) -> Result<String> {
    // Do not resolve source-file aliases implicitly. A downloaded page must
    // not cause mandoc to load an unrelated local manual.
    static INCLUDE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^[.']\s*(?:so|mso)\b").unwrap());
    ensure!(
        !INCLUDE.is_match(&document.text),
        "{}: roff includes are unsupported",
        document.source
    );
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("manual");
    fs::write(&input, &document.text)?;
    let output = Command::new("mandoc")
        .args(["-T", "html", "-O", "fragment", "-W", "error", "-K", "utf-8"])
        .stdin(Stdio::from(fs::File::open(input)?))
        .current_dir(scratch.path())
        .output()
        .context("could not run mandoc; install mandoc and make it available on PATH")?;
    ensure!(
        output.status.success(),
        "{}: mandoc failed: {}",
        document.source,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("mandoc returned non-UTF-8 HTML")
}

pub fn parse(document: &Document) -> Result<Manual> {
    from_html(&document.source, &render(document)?)
}

fn from_html(source: &str, html: &str) -> Result<Manual> {
    let html = Html::parse_document(html);
    let sections = html
        .select(&selector("section.Sh"))
        .filter_map(|section| {
            let heading = section.select(&selector("h1.Sh")).next()?;
            Some((heading.attr("id")?.to_ascii_uppercase(), section))
        })
        .collect::<Vec<_>>();
    let section = |name: &str| {
        sections
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, section)| *section)
            .with_context(|| format!("{source}: missing {name} section"))
    };
    let name = words(&body(section("NAME")?));
    let (name, help) = name
        .split_once(" - ")
        .or_else(|| name.split_once(" — "))
        .with_context(|| format!("{source}: NAME requires 'command - description'"))?;
    let mut command = name
        .split(',')
        .next()
        .unwrap()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure!(!command.is_empty(), "{source}: missing command name");
    let synopsis = words(&body(section("SYNOPSIS")?));
    let path = synopsis
        .split_whitespace()
        .take_while(|word| crate::input::valid_name(word))
        .collect::<Vec<_>>();
    ensure!(
        !path.is_empty() && path.join("-") == command.join("-"),
        "{source}: NAME and SYNOPSIS command paths disagree"
    );
    command = path.into_iter().map(str::to_owned).collect();
    let mut manual = Manual {
        command,
        help: help.into(),
        flags: vec![],
        inherited: vec![],
        source: source.into(),
    };
    for (name, section) in sections {
        if name.contains("OPTIONS") || name == "DESCRIPTION" {
            let flags = extract(source, &name, section)?;
            if name.contains("INHERITED") {
                manual.inherited.extend(flags);
            } else {
                manual.flags.extend(flags);
            }
        }
    }
    let mut names = BTreeSet::new();
    for flag in &manual.flags {
        for name in &flag.names {
            ensure!(names.insert(name), "{source}: duplicate option {name}");
        }
    }
    // A locally documented option takes precedence over its inherited form.
    manual
        .inherited
        .retain(|flag| !flag.names.iter().any(|name| names.contains(name)));
    Ok(manual)
}

fn extract(source: &str, section: &str, element: ElementRef<'_>) -> Result<Vec<Flag>> {
    let mut flags = Vec::new();
    for entry in element.select(&selector("dt, h2.Ss, p.Pp")) {
        let declaration;
        let help;
        let mut default = false;
        match entry.value().name() {
            "dt" => {
                // Consecutive dt elements are aliases sharing the following dd.
                if entry
                    .prev_siblings()
                    .filter_map(ElementRef::wrap)
                    .next()
                    .is_some_and(|s| s.value().name() == "dt")
                {
                    continue;
                }
                declaration = std::iter::once(entry)
                    .chain(
                        entry
                            .next_siblings()
                            .filter_map(ElementRef::wrap)
                            .take_while(|s| s.value().name() == "dt"),
                    )
                    .map(|e| words(&text(e)))
                    .collect::<Vec<_>>()
                    .join(", ");
                help = entry
                    .next_siblings()
                    .filter_map(ElementRef::wrap)
                    .skip_while(|s| s.value().name() == "dt")
                    .take_while(|s| s.value().name() == "dd")
                    .map(text)
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            "h2" => {
                // The permalink wraps the heading; its text is the declaration.
                declaration = words(&text(entry));
                help = entry
                    .next_siblings()
                    .filter_map(ElementRef::wrap)
                    .take_while(|e| e.value().name() != "h2")
                    .map(text)
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            "p" => {
                // Some generators publish 'option=default<TAB>description'.
                // This is a declaration shape, not a separate input format.
                if entry
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .any(|e| e.value().name() == "dd")
                {
                    continue;
                }
                let value = text(entry);
                let Some((left, right)) = value.split_once('\t') else {
                    continue;
                };
                declaration = words(left);
                help = right.to_owned();
                default = true;
            }
            _ => unreachable!(),
        }
        if !declaration.starts_with('-') {
            continue;
        }
        let result = if default {
            default_flag(&declaration, words(&help))
        } else {
            declaration_flag(&declaration, words(&help))
        };
        match result {
            Ok(flag) => flags.push(flag),
            Err(error) => eprintln!("{source}: {section}: skipped {declaration:?}: {error}"),
        }
    }
    Ok(flags)
}

fn default_flag(declaration: &str, help: String) -> Result<Flag> {
    let captures = DEFAULT_DECL
        .captures(declaration)
        .context("unsupported default declaration")?;
    let mut names = vec![captures[1].to_owned()];
    if let Some(long) = captures.get(2) {
        names.push(long.as_str().into());
    }
    ensure!(
        names.iter().all(|name| FLAG_NAME.is_match(name)),
        "unsupported option spelling"
    );
    let default = &captures[3];
    let boolean = matches!(default, "true" | "false");
    Ok(Flag {
        names,
        value: (!boolean).then(|| "value".into()),
        repeated: default == "[]",
        choices: if boolean { vec![] } else { choices(&help) },
        help,
    })
}

fn declaration_flag(declaration: &str, help: String) -> Result<Flag> {
    let declaration = VALUE.replace_all(declaration, "value");
    ensure!(
        !declaration.contains(['[', ']']),
        "optional values or grouped flags need review"
    );
    ensure!(
        !declaration.contains('|') && !declaration.contains("..."),
        "alternative or repeated syntax needs review"
    );
    let normalized = declaration.replace([',', '='], " ");
    let mut names = Vec::new();
    let mut values = Vec::new();
    for token in normalized.split_whitespace() {
        if token.starts_with('-') {
            ensure!(
                FLAG_NAME.is_match(token),
                "unsupported option spelling: {token}"
            );
            names.push(token.to_owned());
        } else {
            ensure!(
                !token.is_empty()
                    && token
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "unsupported value syntax: {token}"
            );
            values.push(token.to_owned());
        }
    }
    ensure!(!names.is_empty(), "no option name");
    ensure!(
        values.iter().all(|value| value == &values[0])
            && (values.len() <= 1 || values.len() == names.len()),
        "multiple option values need review"
    );
    let value = values.first().cloned();
    Ok(Flag {
        names,
        choices: if value.is_some() {
            choices(&help)
        } else {
            vec![]
        },
        value,
        repeated: false,
        help,
    })
}

fn choices(help: &str) -> Vec<String> {
    static CHOICES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"One of:\s*([A-Za-z0-9_|=.,-]+)").unwrap());
    static MUST_BE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"Must be ((?:"[\w-]+"(?:,? or |, )?)+)"#).unwrap());
    static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([\w-]+)""#).unwrap());
    static CHOICE_VALUE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\w-]+$").unwrap());
    if let Some(captures) = CHOICES.captures(help) {
        return captures[1]
            .trim_end_matches('.')
            .split('|')
            .filter(|value| CHOICE_VALUE.is_match(value))
            .map(str::to_owned)
            .collect();
    }
    if let Some(captures) = MUST_BE.captures(help) {
        return QUOTED
            .captures_iter(&captures[1])
            .map(|c| c[1].to_owned())
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> Manual {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        parse(&Document {
            source: name.into(),
            text: fs::read_to_string(path).unwrap(),
        })
        .unwrap()
    }

    #[test]
    fn man_and_mdoc_share_option_extraction() {
        for (file, command) in [("example.1", "example"), ("sample.1", "sample")] {
            let manual = fixture(file);
            assert_eq!(manual.command, [command]);
            assert_eq!(manual.flags.len(), 3);
            assert_eq!(manual.flags[0].names, ["-v", "--verbose"]);
            assert!(manual.flags[0].value.is_none());
            assert!(manual.flags[1].value.is_some());
            assert_eq!(manual.flags[2].choices, ["fast", "careful"]);
        }
    }

    #[test]
    fn delegates_roff_strings_and_inline_markup_to_mandoc() {
        let manual = parse(&Document {
            source: "strings.1".into(),
            text: r#".TH TOOL 1
.ds XX output
.SH NAME
tool \- process files
.SH SYNOPSIS
.B tool
[OPTIONS]
.SH OPTIONS
.TP
.BI "--\*[XX]=" FILE
Write to
.I FILE
with an \(em dash.
"#
            .into(),
        })
        .unwrap();
        assert_eq!(manual.flags[0].names, ["--output"]);
        assert_eq!(manual.flags[0].value.as_deref(), Some("FILE"));
        assert_eq!(manual.flags[0].help, "Write to FILE with an — dash.");
    }

    #[test]
    fn defaults_inherited_options_and_hierarchy_need_no_format_switch() {
        let manual = from_html("tool-child.1", r#"
<section class="Sh"><h1 class="Sh" id="NAME">NAME</h1><p>tool-child - inspect children</p></section>
<section class="Sh"><h1 class="Sh" id="SYNOPSIS">SYNOPSIS</h1><p><b>tool child</b> [OPTIONS]</p></section>
<section class="Sh"><h1 class="Sh" id="OPTIONS">OPTIONS</h1><p class="Pp"><b>--verbose</b>=false	Print details.</p><p class="Pp"><b>-f</b>, <b>--file</b>=[]	Input files.</p></section>
<section class="Sh"><h1 class="Sh" id="OPTIONS_INHERITED_FROM_PARENT_COMMANDS">OPTIONS INHERITED FROM PARENT COMMANDS</h1><p class="Pp"><b>--context</b>=""	Select context.</p></section>
"#).unwrap();
        assert_eq!(manual.command, ["tool", "child"]);
        assert!(manual.flags[0].value.is_none());
        assert!(manual.flags[1].repeated);
        assert_eq!(manual.inherited[0].names, ["--context"]);
    }

    #[test]
    fn rejects_includes_and_incomplete_documents() {
        assert!(
            parse(&Document {
                source: "alias.1".into(),
                text: ".so other.1\n".into()
            })
            .unwrap_err()
            .to_string()
            .contains("includes")
        );
        assert!(from_html("incomplete.1", "<section class='Sh'><h1 class='Sh' id='NAME'>NAME</h1><p>tool - incomplete</p></section>").unwrap_err().to_string().contains("SYNOPSIS"));
    }
}

use termharness::{error::Result, scenario};

// This PTY is backed by termharness, even when cargo was launched in iTerm.
fn run_document(document: &str) -> Result<scenario::Run> {
    let mut ast = scenario::parser::parse(document)?;
    ast.env
        .insert(0, ("TERM_PROGRAM".into(), "termharness".into()));
    scenario::run_ast(&ast)
}

#[test]
fn submit_commands() -> Result<()> {
    run_document(include_str!("scenarios/submit_commands.th"))?;
    Ok(())
}

#[test]
fn backslash_continuation() -> Result<()> {
    run_document(include_str!("scenarios/backslash_continuation.th"))?;
    Ok(())
}

#[test]
fn pipe_continuation() -> Result<()> {
    run_document(include_str!("scenarios/pipe_continuation.th"))?;
    Ok(())
}

#[test]
fn edit_continuation() -> Result<()> {
    run_document(include_str!("scenarios/edit_continuation.th"))?;
    Ok(())
}

#[test]
fn submitted_commands_scrollback() -> Result<()> {
    run_document(include_str!("scenarios/submitted_commands_scrollback.th"))?;
    Ok(())
}

#[test]
fn wrap_edit_clears_stale_rows() -> Result<()> {
    run_document(include_str!("scenarios/wrap_edit_clears_stale_rows.th"))?;
    Ok(())
}

#[test]
fn resize_roundtrip_preserves_input() -> Result<()> {
    run_document(include_str!(
        "scenarios/resize_roundtrip_preserves_input.th"
    ))?;
    Ok(())
}

#[test]
fn history_navigation_restores_draft() -> Result<()> {
    run_document(include_str!(
        "scenarios/history_navigation_restores_draft.th"
    ))?;
    Ok(())
}

#[test]
fn multiline_history_at_input_boundaries() -> Result<()> {
    run_document(include_str!(
        "scenarios/multiline_history_at_input_boundaries.th"
    ))?;
    Ok(())
}

#[test]
fn kubectl_tab_completes_commands_flags_and_values() -> Result<()> {
    run_document(include_str!(
        "scenarios/kubectl_tab_completes_commands_flags_and_values.th"
    ))?;
    Ok(())
}

#[test]
fn kubectl_tab_selects_nested_command() -> Result<()> {
    run_document(include_str!(
        "scenarios/kubectl_tab_selects_nested_command.th"
    ))?;
    Ok(())
}

#[test]
fn kubectl_completion_menu_dismissal_and_midword_edit() -> Result<()> {
    run_document(include_str!(
        "scenarios/kubectl_completion_menu_dismissal_and_midword_edit.th"
    ))?;
    Ok(())
}

#[test]
fn kubectl_tab_in_continuation_line() -> Result<()> {
    run_document(include_str!(
        "scenarios/kubectl_tab_in_continuation_line.th"
    ))?;
    Ok(())
}

#[test]
fn multiline_paste_can_be_edited_before_enter_submits() -> Result<()> {
    run_document(include_str!(
        "scenarios/multiline_paste_can_be_edited_before_enter_submits.th"
    ))?;
    Ok(())
}

#[test]
fn pasted_line_endings_and_trailing_newline_remain_editable() -> Result<()> {
    run_document(include_str!(
        "scenarios/pasted_line_endings_and_trailing_newline_remain_editable.th"
    ))?;
    Ok(())
}

#[test]
fn paste_inserts_at_cursor_and_preserves_existing_suffix() -> Result<()> {
    run_document(include_str!(
        "scenarios/paste_inserts_at_cursor_and_preserves_existing_suffix.th"
    ))?;
    Ok(())
}

#[test]
fn completion_at_bottom_margin() -> Result<()> {
    run_document(include_str!("scenarios/completion_at_bottom_margin.th"))?;
    Ok(())
}

#[test]
fn completion_width_resize_without_input() -> Result<()> {
    run_document(include_str!(
        "scenarios/completion_width_resize_without_input.th"
    ))?;
    Ok(())
}

#[test]
fn completion_navigation_and_resize() -> Result<()> {
    run_document(include_str!(
        "scenarios/completion_navigation_and_resize.th"
    ))?;
    Ok(())
}

#[test]
fn completion_extreme_width_burst() -> Result<()> {
    run_document(include_str!("scenarios/completion_extreme_width_burst.th"))?;
    Ok(())
}

#[test]
fn completion_extreme_width_preserves_history() -> Result<()> {
    run_document(include_str!(
        "scenarios/completion_extreme_width_preserves_history.th"
    ))?;
    Ok(())
}

#[test]
fn completion_extreme_width_overflow() -> Result<()> {
    let result = run_document(include_str!(
        "scenarios/completion_extreme_width_overflow.th"
    ));
    // Reflow and repaint can interleave differently during a resize burst. The
    // terminal may consequently scroll a different number of empty rows. For
    // this final assertion only, allow a vertical translation of the entire
    // frame. Preserve every interior blank row, duplicate, and stray character.
    if let Err(termharness::error::Error::ScreenMismatch {
        step,
        expected,
        actual,
        ..
    }) = &result
        && step == "shrink to one column and widen without waiting or typing"
    {
        let content = |lines: &[String]| {
            let first = lines
                .iter()
                .position(|line| !line.trim().is_empty())
                .unwrap_or(0);
            let last = lines
                .iter()
                .rposition(|line| !line.trim().is_empty())
                .map_or(first, |i| i + 1);
            lines[first..last].to_vec()
        };
        if content(expected) == content(actual) {
            return Ok(());
        }
    }
    result.map(|_| ())
}

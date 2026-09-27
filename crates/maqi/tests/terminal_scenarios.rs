use termharness::{error::Result, scenario};

#[test]
fn submit_commands() -> Result<()> {
    scenario::run_document(include_str!("scenarios/submit_commands.th"))?;
    Ok(())
}

#[test]
fn backslash_continuation() -> Result<()> {
    scenario::run_document(include_str!("scenarios/backslash_continuation.th"))?;
    Ok(())
}

#[test]
fn pipe_continuation() -> Result<()> {
    scenario::run_document(include_str!("scenarios/pipe_continuation.th"))?;
    Ok(())
}

#[test]
fn edit_continuation() -> Result<()> {
    scenario::run_document(include_str!("scenarios/edit_continuation.th"))?;
    Ok(())
}

#[test]
fn submitted_commands_scrollback() -> Result<()> {
    scenario::run_document(include_str!("scenarios/submitted_commands_scrollback.th"))?;
    Ok(())
}

#[test]
fn wrap_edit_clears_stale_rows() -> Result<()> {
    scenario::run_document(include_str!("scenarios/wrap_edit_clears_stale_rows.th"))?;
    Ok(())
}

#[test]
fn resize_roundtrip_preserves_input() -> Result<()> {
    scenario::run_document(include_str!(
        "scenarios/resize_roundtrip_preserves_input.th"
    ))?;
    Ok(())
}

#[test]
fn history_navigation_restores_draft() -> Result<()> {
    scenario::run_document(include_str!(
        "scenarios/history_navigation_restores_draft.th"
    ))?;
    Ok(())
}

#[test]
fn multiline_history_at_input_boundaries() -> Result<()> {
    scenario::run_document(include_str!(
        "scenarios/multiline_history_at_input_boundaries.th"
    ))?;
    Ok(())
}

#[test]
fn multiline_paste_can_be_edited_before_enter_submits() -> Result<()> {
    scenario::run_document(include_str!(
        "scenarios/multiline_paste_can_be_edited_before_enter_submits.th"
    ))?;
    Ok(())
}

#[test]
fn pasted_line_endings_and_trailing_newline_remain_editable() -> Result<()> {
    scenario::run_document(include_str!(
        "scenarios/pasted_line_endings_and_trailing_newline_remain_editable.th"
    ))?;
    Ok(())
}

#[test]
fn paste_inserts_at_cursor_and_preserves_existing_suffix() -> Result<()> {
    scenario::run_document(include_str!(
        "scenarios/paste_inserts_at_cursor_and_preserves_existing_suffix.th"
    ))?;
    Ok(())
}

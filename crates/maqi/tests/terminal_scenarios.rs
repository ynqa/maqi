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

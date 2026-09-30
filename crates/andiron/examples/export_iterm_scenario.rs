//! Export termharness's parsed .th document for the optional iTerm backend.
use serde_json::{Value, json};
use termharness::scenario::{ast::*, parser};

fn action(action: ActionAst) -> Value {
    match action {
        ActionAst::Input(input) => {
            let text = match input {
                InputAst::Text(text) => text,
                InputAst::Key { key, count } => match key {
                    KeyAst::Left => "\x1b[D",
                    KeyAst::Right => "\x1b[C",
                    KeyAst::Up => "\x1b[A",
                    KeyAst::Down => "\x1b[B",
                    KeyAst::Enter => "\r",
                    KeyAst::Backspace => "\x7f",
                    KeyAst::Tab => "\t",
                    KeyAst::Escape => "\x1b",
                }
                .repeat(count.into()),
            };
            json!({"input": text})
        }
        ActionAst::Paste(text) => json!({"input": format!("\x1b[200~{text}\x1b[201~")}),
        ActionAst::Resize(size) => json!({"rows": size.rows, "cols": size.cols}),
        ActionAst::Scroll { direction, lines } => json!({
            "scroll": match direction { ScrollDirection::Up => "up", ScrollDirection::Down => "down" },
            "lines": lines,
        }),
        ActionAst::WaitFrontendLineStartsWith { text, timeout_ms } => {
            json!({"wait_frontend": text, "timeout_ms": timeout_ms})
        }
        ActionAst::WaitBackendLineStartsWith { text, timeout_ms } => {
            json!({"wait_backend": text, "timeout_ms": timeout_ms})
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected a .th path")?;
    let ast = parser::parse(&std::fs::read_to_string(path)?)?;
    let steps: Vec<_> = ast
        .steps
        .into_iter()
        .map(|step| {
            json!({
                "label": step.label,
                "actions": step.actions.into_iter().map(action).collect::<Vec<_>>(),
                "settle_ms": step.settle_ms,
                "timeout_ms": step.expect_timeout_ms,
                "expect": step.expect,
            })
        })
        .collect();
    println!(
        "{}",
        json!({
            "name": ast.name, "command": ast.command, "args": ast.args, "env": ast.env,
            "rows": ast.terminal.rows, "cols": ast.terminal.cols,
            "cursor_row": ast.cursor.row, "cursor_col": ast.cursor.col,
            "steps": steps,
        })
    );
    Ok(())
}

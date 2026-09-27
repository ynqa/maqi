mod completion;
mod continuation;
mod readline;
mod terminal;
mod ui;
mod usage_spec;

use std::io;

use futures::StreamExt;
use promkit::{
    core::{
        crossterm::{event::EventStream, execute, style::Print},
        render::Renderer,
    },
    TerminalModes, TerminalSession,
};

use readline::{Action, Readline};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _session =
        TerminalSession::try_new(TerminalModes::RAW_MODE | TerminalModes::HIDDEN_CURSOR)?;
    let _paste_mode = terminal::BracketedPaste::enable()?;
    let mut events = EventStream::new();
    let mut readline = Readline::default();

    loop {
        readline.reset_input();
        let renderer = Renderer::try_new_with_graphemes(readline.render_items()?, true).await?;

        let action = loop {
            let Some(event) = events.next().await else {
                break Action::Exit;
            };
            let action = readline.handle_event(event?);
            if action != Action::Continue {
                break action;
            }
            renderer.update(readline.render_items()?).render().await?;
        };

        readline.finish(&renderer).await?;
        match action {
            Action::Submit(command) if !command.trim().is_empty() => {
                execute!(
                    io::stdout(),
                    Print(command.replace('\n', "\r\n")),
                    Print("\r\n")
                )?;
            }
            Action::Exit => break,
            Action::Submit(_) | Action::Cancel => {}
            Action::Continue => unreachable!("only completed input leaves the read loop"),
        }
    }

    Ok(())
}

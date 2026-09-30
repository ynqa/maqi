mod completion;
mod continuation;
mod readline;
mod ui;
mod usage_spec;

use std::io::{self, Write};

use andiron::{Renderer, TerminalSession, event};
use readline::{Action, Readline};

fn main() -> anyhow::Result<()> {
    let _session = TerminalSession::new()?;
    let mut readline = Readline::default();

    loop {
        readline.reset_input();
        let mut renderer = Renderer::new()?;
        readline.render(&mut renderer)?;

        let action = loop {
            if renderer.resize_polling() && !event::poll(std::time::Duration::from_millis(16))? {
                readline.refresh(&mut renderer)?;
                continue;
            }
            let event = event::read()?;
            if matches!(event, event::Event::Resize(..)) {
                renderer.resize();
            }
            let action = readline.handle_event(event);
            if action != Action::Continue {
                break action;
            }
            readline.render(&mut renderer)?;
        };

        readline.finish(&mut renderer)?;
        match action {
            Action::Submit(command) if !command.trim().is_empty() => {
                let mut output = io::stdout().lock();
                write!(output, "{}\r\n", command.replace('\n', "\r\n"))?;
                output.flush()?;
            }
            Action::Exit => break,
            Action::Submit(_) | Action::Cancel => {}
            Action::Continue => unreachable!("only completed input leaves the read loop"),
        }
    }

    Ok(())
}

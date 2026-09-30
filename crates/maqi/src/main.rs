mod completion;
mod continuation;
mod readline;
mod ui;
mod usage_spec;

use std::{
    collections::VecDeque,
    io::{self, Write},
};

use andiron::{Renderer, TerminalSession, event};
use readline::{Action, Readline};

fn main() -> anyhow::Result<()> {
    let _session = TerminalSession::new()?;
    let mut readline = Readline::default();
    let mut pending = VecDeque::new();

    loop {
        readline.reset_input();
        let mut renderer = Renderer::new()?;
        readline.render(&mut renderer)?;

        let action = loop {
            if renderer.input_deferred() {
                if event::poll(std::time::Duration::from_millis(16))? {
                    let next = event::read()?;
                    if matches!(next, event::Event::Resize(..)) {
                        renderer.resize();
                    } else {
                        pending.push_back(next);
                    }
                }
                readline.refresh(&mut renderer)?;
                continue;
            }
            if renderer.resize_polling() && !event::poll(std::time::Duration::from_millis(16))? {
                readline.refresh(&mut renderer)?;
                continue;
            }
            let event = match pending.pop_front() {
                Some(next) => next,
                None => event::read()?,
            };
            if matches!(event, event::Event::Resize(..)) {
                renderer.resize();
                readline.refresh(&mut renderer)?;
                continue;
            }
            let action = readline.handle_event(event);
            if action != Action::Continue {
                break action;
            }
            readline.render(&mut renderer)?;
        };

        loop {
            match readline.finish(&mut renderer) {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if event::poll(std::time::Duration::from_millis(16))? {
                        let next = event::read()?;
                        if matches!(next, event::Event::Resize(..)) {
                            renderer.resize();
                        } else {
                            pending.push_back(next);
                        }
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
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

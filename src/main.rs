use std::io::stdout;

use color_eyre::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::ui::render;

pub mod app;
// Wired into main() by config-store task 2.4; test-only until then.
#[cfg(test)]
pub mod config;
pub mod handler;
pub mod patch;
#[cfg(test)]
mod regression;
pub mod theme;
pub mod ui;

fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;

    // ratatui::init() already installed a panic hook that restores raw
    // mode/the alternate screen; chain onto it so mouse capture is also
    // disabled before that hook runs, for a clean terminal on panic too.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        previous_hook(info);
    }));

    let result = run(terminal);

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal) -> Result<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|frame| render(frame, &mut app))?;

        match event::read()? {
            Event::Key(key) => {
                if handler::handle_event(key, &mut app) {
                    break;
                }
                // Task 4/8: viewer routing is handled in handler::handle_event
                // (ESC closes, j/k navigates, readonly). No unconditional close here.
            }
            Event::Mouse(mouse) => {
                handler::handle_mouse_event(mouse, &mut app);
            }
            // No state to update: panel layout is computed fresh from
            // frame.area() every draw() call, so the next iteration's draw
            // already reflows against the new terminal size.
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    Ok(())
}

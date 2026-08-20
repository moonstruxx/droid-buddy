use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::ui::render;

pub mod app;
pub mod handler;
pub mod patch;
pub mod ui;

fn main() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = run(terminal);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal) -> Result<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|frame| render(frame, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if handle_key_event(key, &mut app) {
                break;
            }
        }
    }

    Ok(())
}

fn handle_key_event(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        _ => {
            app.handle_input(key);
            false
        }
    }
}

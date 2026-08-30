use std::io::stdout;

use color_eyre::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use ratatui::DefaultTerminal;

use droid_tui::app::App;
use droid_tui::latency::CostModel;
use droid_tui::ui::render;
use droid_tui::{config, handler, theme};

fn main() -> Result<()> {
    color_eyre::install()?;
    // Config load runs before terminal init so stderr warnings are visible
    // and rendering never starts with a half-selected theme.
    let settings = config::load(&theme::canonical_theme_name, theme::THEMES);
    theme::init(*theme::resolve(&settings.theme));
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

    let result = run(terminal, &settings);

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal, settings: &config::Settings) -> Result<()> {
    let mut app = App::new();
    // The shared per-circuit cost provider (design D2): config `[latency]`
    // overrides layered over the ramsize heuristic, consumed by every graph
    // build so latency coloring and the optimizer stay coherent.
    app.cost_model = CostModel::from_config(settings);

    // [physical] view defaults (design D12): zoom seeds both the UI scale
    // factor and the linked physical zoom (the `+`/`-` presets set both),
    // offset seeds the pan origin, show_skeleton seeds the presentation
    // mode, and rack seeds the case the physical view packs into. Absent
    // `[physical]` defaults mirror App::new, so out-of-box behavior is
    // unchanged.
    app.scale_factor = settings.physical.zoom as f32;
    app.physical_zoom = settings.physical.zoom as f32;
    app.physical_offset = (
        settings.physical.offset_x as f32,
        settings.physical.offset_y as f32,
    );
    app.physical_show_skeleton = settings.physical.show_skeleton;
    app.physical_rack_spec = settings.physical.rack.clone();

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

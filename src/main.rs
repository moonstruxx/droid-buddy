use std::io::stdout;

use color_eyre::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;

use droid_tui::app::App;
use droid_tui::latency::CostModel;
use droid_tui::{config, schema, theme};

fn main() -> Result<()> {
    color_eyre::install()?;
    // Config load runs before terminal init so stderr warnings are visible
    // and rendering never starts with a half-selected theme.
    let settings = config::load(&theme::canonical_theme_name, theme::THEMES);
    theme::init(*theme::resolve(&settings.theme));
    // Install the merged schema (embedded + plugin overlay) before the
    // terminal switches to the alternate screen so plugin shadow/skip
    // warnings print on a clean terminal (ADR 14).
    schema::init(
        &settings,
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    );
    let terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;

    // ratatui::init() already installed a panic hook that restores raw
    // mode/the alternate screen; chain onto it so mouse capture is also
    // disabled before that hook runs, for a clean terminal on panic too.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        // With the gui feature, destroy the graph window before the terminal
        // restore hook runs so a panic leaves no orphaned desktop window.
        #[cfg(feature = "gui")]
        droid_tui::gui::destroy_window_for_panic();
        previous_hook(info);
    }));

    #[cfg(feature = "gui")]
    let result = windowed::run(terminal, &settings);
    #[cfg(not(feature = "gui"))]
    let result = run(terminal, &settings);

    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

/// App state seeded from `settings`, shared by the terminal-only and the
/// windowed event loops so both surfaces start from identical state.
fn seed_app(app: &mut App, settings: &config::Settings) {
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
}

#[cfg(not(feature = "gui"))]
fn run(mut terminal: ratatui::DefaultTerminal, settings: &config::Settings) -> Result<()> {
    use crossterm::event::{self, Event};

    use droid_tui::handler;
    use droid_tui::ui::render;

    let mut app = App::new();
    seed_app(&mut app, settings);

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

#[cfg(feature = "gui")]
mod windowed {
    use std::time::Duration;

    use color_eyre::Result;
    use crossterm::event::{self, Event};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::WindowId;

    use droid_tui::app::{App, GraphWindowRequest};
    use droid_tui::gui::GraphWindow;
    use droid_tui::ui::render;
    use droid_tui::{config, handler};

    use super::seed_app;

    /// The windowed application: owns the single `App`, the terminal, and the
    /// graph window, driven by winit's event loop (gpu-graph-window design D1).
    struct AppHandler {
        app: App,
        terminal: ratatui::DefaultTerminal,
        window: GraphWindow,
        needs_redraw: bool,
        error: Option<color_eyre::Report>,
    }

    impl AppHandler {
        /// One frame of the multiplexed loop: drain terminal input, then
        /// redraw the terminal when anything changed.
        fn pump(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_open() {
                // Non-blocking drain while winit drives the window's redraws.
                while matches!(event::poll(Duration::ZERO), Ok(true)) {
                    if let Ok(event) = event::read() {
                        self.dispatch(event, event_loop);
                    }
                }
            } else {
                // The terminal blocks on its next event like the terminal-only
                // loop; draw first so the current frame shows before blocking.
                self.redraw_if_needed(event_loop);
                match event::read() {
                    Ok(event) => self.dispatch(event, event_loop),
                    Err(err) => {
                        self.fail(event_loop, err.into());
                        return;
                    }
                }
            }
            self.act_on_window_request(event_loop);
            self.redraw_if_needed(event_loop);
        }

        /// Consume the handler's pending GPU-graph-window request (design D6):
        /// the handler cannot reach `GraphWindow` (owned by this loop), so it
        /// queues Open/Toggle on `App` and the loop performs the matching
        /// open/close on its next frame. A failed open (headless display)
        /// falls back to the terminal graph tile.
        fn act_on_window_request(&mut self, event_loop: &ActiveEventLoop) {
            match self.app.take_graph_window_request() {
                GraphWindowRequest::None => {}
                GraphWindowRequest::Open => {
                    if !self.window.is_open() {
                        self.open_window_or_tile(event_loop);
                    }
                }
                GraphWindowRequest::Toggle => {
                    if self.window.is_open() {
                        self.window.close();
                    } else {
                        self.open_window_or_tile(event_loop);
                    }
                }
            }
        }

        fn open_window_or_tile(&mut self, event_loop: &ActiveEventLoop) {
            if let Err(err) = self.window.open(event_loop) {
                eprintln!("[warn] graph window open failed ({err}); using terminal tile");
                self.app.open_graph();
            }
        }

        fn dispatch(&mut self, event: Event, event_loop: &ActiveEventLoop) {
            self.needs_redraw = true;
            match event {
                Event::Key(key) => {
                    if handler::handle_event(key, &mut self.app) {
                        event_loop.exit();
                    }
                    // Task 4/8: viewer routing is handled in handler::handle_event
                    // (ESC closes, j/k navigates, readonly). No unconditional close here.
                }
                Event::Mouse(mouse) => handler::handle_mouse_event(mouse, &mut self.app),
                // Layout is computed fresh from frame.area() every draw, so a
                // resize needs no state update (same as the terminal-only loop).
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        fn redraw_if_needed(&mut self, event_loop: &ActiveEventLoop) {
            if self.needs_redraw {
                self.needs_redraw = false;
                if let Err(err) = self.terminal.draw(|frame| render(frame, &mut self.app)) {
                    self.fail(event_loop, err.into());
                }
            }
        }

        fn fail(&mut self, event_loop: &ActiveEventLoop, err: color_eyre::Report) {
            self.error = Some(err);
            event_loop.exit();
        }
    }

    impl ApplicationHandler for AppHandler {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            // Poll keeps the multiplexed loop hot so window and terminal
            // events both drain every frame; the terminal blocks instead
            // while the window is closed (see AppHandler::pump).
            event_loop.set_control_flow(ControlFlow::Poll);
        }

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            // The app has a single window; ignore events for any other id.
            if self.window.window_id() != Some(window_id) {
                return;
            }
            match event {
                WindowEvent::CloseRequested => self.window.close(),
                WindowEvent::RedrawRequested => {
                    // Task 3.1: the frame reports the window's input state;
                    // the loop owns `app`, so it maps the interactions here
                    // (D6: the loop acts on what it owns).
                    if let Some(frame) = self.window.fill_placeholder() {
                        handler::handle_graph_window_frame(&frame, &mut self.app);
                    }
                    self.needs_redraw = true;
                }
                // Task 3.1: every other event feeds the egui input pipeline
                // so pointer/keyboard input reaches the painter's WinitState.
                other => self.window.window_event(&other),
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.pump(event_loop);
        }
    }

    /// The multiplexed main loop (design D1): winit's event loop owns the
    /// thread and drives both the window and the polled terminal.
    pub(super) fn run(
        terminal: ratatui::DefaultTerminal,
        settings: &config::Settings,
    ) -> Result<()> {
        // winit allows exactly one EventLoop per process, so it is created
        // here once; GraphWindow::open later creates windows on it.
        let event_loop = EventLoop::new().map_err(|err| {
            color_eyre::Report::msg(format!("failed to create winit event loop: {err}"))
        })?;
        let mut app = App::new();
        seed_app(&mut app, settings);
        let mut handler = AppHandler {
            app,
            terminal,
            window: GraphWindow::new(),
            needs_redraw: true,
            error: None,
        };
        event_loop
            .run_app(&mut handler)
            .map_err(|err| color_eyre::Report::msg(format!("winit event loop failed: {err}")))?;
        handler.error.map_or(Ok(()), Err)
    }
}

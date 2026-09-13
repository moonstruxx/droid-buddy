use color_eyre::Result;

use droid_tui::app::App;
use droid_tui::latency::CostModel;
use droid_tui::{config, schema, theme};

fn main() -> Result<()> {
    color_eyre::install()?;
    // Config load runs before the window opens so stderr warnings are visible.
    let settings = config::load(&theme::canonical_theme_name, theme::THEMES);
    theme::init(*theme::resolve(&settings.theme));
    // Install the merged schema (embedded + plugin overlay) before the window
    // opens so plugin shadow/skip warnings print on a clean stderr (ADR 14).
    schema::init(
        &settings,
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    );
    // Destroy the graph window on panic so a panic leaves no orphaned desktop
    // window, then chain the default hook.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        droid_tui::gui::destroy_window_for_panic();
        previous_hook(info);
    }));

    windowed::run(&settings)
}

/// App state seeded from `settings`, shared by the windowed event loop.
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

    // [gui] graph_window: seed the GPU-window preference (gpu-graph-window D6);
    // matching App::new's default.
    app.graph_window_enabled = settings.gui.graph_window;
}

mod windowed {
    use color_eyre::Result;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::WindowId;

    use droid_tui::app::{App, GraphWindowRequest};
    use droid_tui::gui::{self, GraphWindow};
    use droid_tui::{config, handler, theme};

    use super::seed_app;

    /// The native application: owns the single `App` and the graph window,
    /// driven by winit's event loop (gpu-graph-window design D1).
    struct AppHandler {
        app: App,
        window: GraphWindow,
        error: Option<color_eyre::Report>,
    }

    impl AppHandler {
        /// Consume the handler's pending GPU-graph-window request (design D6):
        /// the handler cannot reach `GraphWindow` (owned by this loop), so it
        /// queues Open/Toggle on `App` and the loop performs the matching
        /// open/close on its next frame. A failed open (headless display)
        /// leaves the window closed; the app keeps running.
        fn act_on_window_request(&mut self, event_loop: &ActiveEventLoop) {
            match self.app.take_graph_window_request() {
                GraphWindowRequest::None => {}
                GraphWindowRequest::Open => {
                    if !self.window.is_open() {
                        self.open_window(event_loop);
                    }
                }
                GraphWindowRequest::Toggle => {
                    if self.window.is_open() {
                        self.window.close();
                    } else {
                        self.open_window(event_loop);
                    }
                }
            }
        }

        fn open_window(&mut self, event_loop: &ActiveEventLoop) {
            if let Err(err) = self.window.open(event_loop) {
                eprintln!("[warn] graph window open failed ({err})");
            }
        }
    }

    impl ApplicationHandler for AppHandler {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            event_loop.set_control_flow(ControlFlow::Poll);
            // Native-only: the window is the whole app, so it opens at startup
            // and every frame is driven by the window's redraw events.
            if !self.window.is_open() {
                self.open_window(event_loop);
            }
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
                    // Rebuild the scene from App state and push it before
                    // painting so the window shows the current graph (the
                    // loop owns both; `set_scene` no-ops on an unchanged
                    // scene, so an idle loop never re-arms the redraw).
                    if self.app.graph_camera.is_none() {
                        // First-frame camera fit (bug: the window opened
                        // black, painting the layered seed plane through the
                        // identity camera): seed `App.graph_camera` when
                        // unset, dependency-filter aware, and reuse it after
                        // so pan/zoom survive.
                        let fit: Vec<(f32, f32)> = if self.app.dependency_nodes.is_empty() {
                            self.app.graph_positions.clone()
                        } else {
                            self.app
                                .dependency_nodes
                                .iter()
                                .map(|&i| self.app.graph_positions[i])
                                .collect()
                        };
                        self.app.graph_camera = Some(gui::graph_window_fit_camera(&fit));
                    }
                    let scene = gui::build_scene_spec(&self.app, theme::active());
                    self.window.set_scene(scene.as_ref());
                    // The frame reports the window's input state; the loop
                    // owns `app`, so it maps the interactions here (D6: the
                    // loop acts on what it owns).
                    if let Some(frame) = self.window.fill_placeholder() {
                        handler::handle_graph_window_frame(&frame, &mut self.app);
                    }
                }
                // Every other event feeds the egui input pipeline so pointer
                // and keyboard input reach the painter's WinitState.
                other => self.window.window_event(&other),
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.act_on_window_request(event_loop);
        }
    }

    /// The native main loop (design D1): winit's event loop owns the thread
    /// and drives the graph window.
    pub(super) fn run(settings: &config::Settings) -> Result<()> {
        // winit allows exactly one EventLoop per process, so it is created
        // here once; GraphWindow::open later creates windows on it.
        let event_loop = EventLoop::new().map_err(|err| {
            color_eyre::Report::msg(format!("failed to create winit event loop: {err}"))
        })?;
        let mut app = App::new();
        seed_app(&mut app, settings);
        let mut handler = AppHandler {
            app,
            window: GraphWindow::new(),
            error: None,
        };
        event_loop
            .run_app(&mut handler)
            .map_err(|err| color_eyre::Report::msg(format!("winit event loop failed: {err}")))?;
        handler.error.map_or(Ok(()), Err)
    }
}

/// `seed_app` wires the `[gui] graph_window` preference into `App` (design D6).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_app_enables_graph_window_from_setting() {
        let mut settings = config::Settings::default();
        settings.gui.graph_window = true;
        let mut app = App::new();
        seed_app(&mut app, &settings);
        assert!(app.graph_window_enabled);
    }

    #[test]
    fn seed_app_keeps_graph_window_disabled_by_default() {
        let settings = config::Settings::default();
        let mut app = App::new();
        seed_app(&mut app, &settings);
        assert!(!app.graph_window_enabled);
    }
}

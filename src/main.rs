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

    // The optional `argv[1]` path is the patch the window opens with; without
    // one the bundled sample loads. A patch is always installed so the graph
    // can be built and the window paints content instead of its clear color.
    let initial_patch = std::env::args().nth(1).map(std::path::PathBuf::from);

    windowed::run(&settings, initial_patch)
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

/// Bundled demo patch shown when no `argv[1]` path is given. `Patch::sample()`
/// has no circuit sections, so its signal-flow graph is empty and the window
/// would paint only its clear color; this fixture is a real patch with circuits.
const DEMO_PATCH: &str = include_str!("../fixtures/arpeggio1.ini");

/// Load the patch the window opens with: the `argv[1]` path when one is given,
/// else the bundled demo. A real patch is always installed so `open_graph` can
/// build a non-empty graph and the window paints content on its first frame
/// instead of its clear color.
fn load_initial_patch(app: &mut App, path: Option<&std::path::Path>) {
    if let Some(path) = path.filter(|p| !p.as_os_str().is_empty()) {
        if try_load_file(app, path) {
            return;
        }
    }
    match droid_tui::patch::Patch::from_ini_str(DEMO_PATCH, String::from("demo")) {
        Ok(patch) => {
            app.load_patch(patch);
        }
        Err(err) => eprintln!("[warn] could not parse the bundled demo patch: {err}"),
    }
}

fn try_load_file(app: &mut App, path: &std::path::Path) -> bool {
    match droid_tui::patch::Patch::from_ini_file(path) {
        Ok(patch) => {
            app.load_patch_at(path, patch);
            true
        }
        Err(err) => {
            eprintln!("[warn] could not load {}: {err}", path.display());
            false
        }
    }
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

    use super::{load_initial_patch, seed_app};

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
            // The window opens lazily from `about_to_wait` (which calls
            // `act_on_window_request`) so close/open races are impossible.
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
                    // loop acts on what it owns). `app` flows into the paint
                    // so the quad dispatch builds real per-pane specs and
                    // publishes `pane_rects`.
                    if let Some(frame) = self.window.fill_placeholder(&mut self.app) {
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
    pub(super) fn run(
        settings: &config::Settings,
        initial_patch: Option<std::path::PathBuf>,
    ) -> Result<()> {
        // winit allows exactly one EventLoop per process, so it is created
        // here once; GraphWindow::open later creates windows on it.
        let event_loop = EventLoop::new().map_err(|err| {
            color_eyre::Report::msg(format!("failed to create winit event loop: {err}"))
        })?;
        let mut app = App::new();
        seed_app(&mut app, settings);
        load_initial_patch(&mut app, initial_patch.as_deref());
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

    #[test]
    fn load_initial_patch_without_path_loads_demo() {
        let mut app = App::new();
        load_initial_patch(&mut app, None);
        assert!(app.patch.is_some());
        assert_eq!(app.patch.as_ref().map(|p| p.name.as_str()), Some("demo"));
    }

    #[test]
    fn load_initial_patch_with_missing_path_loads_demo() {
        let mut app = App::new();
        load_initial_patch(
            &mut app,
            Some(std::path::Path::new("/nonexistent/nope.ini")),
        );
        assert!(app.patch.is_some());
        assert_eq!(app.patch.as_ref().map(|p| p.name.as_str()), Some("demo"));
    }

    #[test]
    fn load_initial_patch_with_fixture_loads_it() {
        let mut app = App::new();
        load_initial_patch(
            &mut app,
            Some(std::path::Path::new("fixtures/arpeggio1.ini")),
        );
        assert!(app.patch.is_some());
        assert_eq!(
            app.patch.as_ref().map(|p| p.name.as_str()),
            Some("arpeggio1")
        );
    }

    #[test]
    fn startup_seeding_produces_no_graph_until_requested() {
        // Mirrors the fixed startup: load a patch but do NOT auto-open the graph.
        // The window must open on the physical/panels view, so the startup scene
        // is None/empty until `g g` explicitly builds the graph.
        let mut app = App::new();
        load_initial_patch(&mut app, None);
        let scene = droid_tui::gui::build_scene_spec(&app, theme::active());
        assert!(
            scene.is_none() || scene.is_some_and(|s| s.nodes.is_empty()),
            "startup must not produce a graph scene before g g"
        );
        // Explicit `g g` still produces a non-empty graph.
        app.open_graph();
        let scene = droid_tui::gui::build_scene_spec(&app, theme::active());
        let spec = scene.expect("g g must produce a scene to paint");
        assert!(!spec.nodes.is_empty(), "g g must paint nodes");
    }
}

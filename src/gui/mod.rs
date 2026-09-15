//! GPU-accelerated graph window (egui + wgpu), compiled unconditionally.
//!
//! The window surface and the egui/wgpu canvas live in thread-locals so the
//! panic hook in `main.rs` can tear them down while the stack unwinds;
//! [`GraphWindow`] is the façade around both. The multiplexed winit loop that
//! drives the surface lives in `main.rs`; the egui painter backend attaches
//! here and interaction mapping is task 3.x. Nothing here is ever constructed
//! under `cargo test`: the window is created only by explicit lifecycle calls.
//!
//! Surface dispatch: [`EguiSurface::paint`] is the one-frame entry point that
//! delegates to the per-surface draw routines; the graph canvas lives in
//! [`graph`]. Later surface ports (physical, panels, viewer, picker, overlays,
//! tasks 2.1-2.5) add their own draw routines alongside `graph::paint_scene`
//! without touching this shell.

mod graph;

pub use graph::build_scene_spec;

// Surface ports (physical, panels, viewer, picker, overlays — tasks 2.1-2.5)
// are runtime surfaces: the shell dispatches to them in `EguiSurface::paint`
// and headless egui shape/label tests exercise the same draw routines.
#[allow(dead_code, unused_imports)]
mod overlays;
#[allow(dead_code, unused_imports)]
mod panels;
#[allow(dead_code, unused_imports)]
mod physical;
#[allow(dead_code, unused_imports)]
mod picker;
#[allow(dead_code, unused_imports)]
mod viewer;

// The camera helpers stay crate API (`crate::gui::camera_pan`, used by
// `handler.rs` inside the lib), while `graph_window_fit_camera` is also
// consumed by the windowed loop in the `droid_tui` bin (a separate crate), so
// it re-exports at full visibility. `graph` stays a private canvas module.
pub use graph::graph_window_fit_camera;
pub(crate) use graph::{camera_pan, camera_zoom_about};
#[cfg(test)]
pub(crate) use graph::{MAX_ZOOM_STEP, ZOOM_SENSITIVITY};
#[allow(unused_imports)]
pub(crate) use panels::PanelsFrame;
#[allow(unused_imports)]
pub(crate) use physical::PhysicalFrame;
#[allow(unused_imports)]
pub(crate) use picker::PickerFrame;
#[allow(unused_imports)]
pub(crate) use viewer::ViewerFrame;

use std::cell::RefCell;
use std::fmt;
use std::future::Future;
use std::task::{Context as TaskContext, Poll};

use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::App;
use crate::graph_render::SceneSpec;

/// Lifecycle state of the graph window.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum WindowState {
    /// No window surface exists.
    #[default]
    Closed,
    /// A window surface exists and is being driven.
    Open,
}

/// Graph-window keys mapped onto existing `App` mutations (task 3.1): the
/// windowed loop applies them to the hovered node, mirroring the terminal
/// graph surface's `x`/`p`/`e`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowGraphKey {
    /// `x`: toggle per-circuit processing for the hovered node.
    ToggleProcessing,
    /// `p`: toggle the pin anchor of the hovered node.
    TogglePin,
    /// `e`: begin the circuit label edit overlay for the hovered node.
    BeginEdit,
}

/// Raw window-frame input the loop turns into `App` mutations (task 3.1).
/// Pointer positions are egui points, which the painter maps 1:1 to spec
/// pixels (one spec pixel = one egui point, see [`EguiSurface::paint`]);
/// `None` means the pointer is not over the window. `keys` holds the
/// [`WindowGraphKey`]s pressed during the frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowFrame {
    pub pointer: Option<(f32, f32)>,
    pub primary_pressed: bool,
    pub primary_down: bool,
    pub primary_released: bool,
    pub keys: Vec<WindowGraphKey>,
    /// Middle-button drag pan (spec pixels) to apply to the shared camera.
    pub pan_delta: (f32, f32),
    /// Wheel zoom: scale factor plus the pixel anchor the cursor stays on.
    pub zoom: Option<(f32, (f32, f32))>,
    /// Active marquee selection over nodes (empty-background left drag).
    pub marquee: Option<MarqueeSelection>,
}

/// A marquee (rubber-band) selection over the canvas: the pixel-space drag
/// rect plus the node indices whose frames intersect it (task 3.3). The rect
/// is normalized so the drag direction does not matter.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarqueeSelection {
    /// Pixel rect `(x, y, w, h)` of the drag, in egui points / spec pixels.
    pub rect: (f32, f32, f32, f32),
    /// Indices into `SceneSpec::nodes` selected by the marquee.
    pub nodes: Vec<usize>,
}

/// Why a window could not be opened.
#[derive(Debug)]
pub struct WindowError {
    message: String,
}

impl WindowError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WindowError {}

// The single graph window surface and its egui/wgpu canvas. Parked in
// thread-locals so the panic hook can drop them even while the app stack is
// unwinding; the main thread is the only thread that ever touches them.
//
// Drop order matters: the wgpu surface is created from the window's raw
// handle (unsafe), so the canvas must die before the window. `close()` and
// `destroy_window_for_panic` both clear EGUI first, then WINDOW.
thread_local! {
    static WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
    static EGUI: RefCell<Option<EguiSurface>> = const { RefCell::new(None) };
    static DISPLAY: RefCell<Option<OwnedDisplayHandle>> = const { RefCell::new(None) };
}

/// The wgpu swapchain: surface, device, and the egui renderer bound to it.
struct WgpuCanvas {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
}

/// The egui stack attached to the window surface (task 2.2).
struct EguiSurface {
    context: egui::Context,
    winit_state: egui_winit::State,
    canvas: WgpuCanvas,
    /// Physical pixel size of the surface, for detecting resizes (main.rs
    /// forwards no window events yet; task 3.1 wires them).
    size_px: (u32, u32),
}

/// A GPU graph window bound to the same `App` state as the terminal tile.
///
/// winit permits only one `EventLoop` per process, so the loop is created
/// once in `main.rs`; [`GraphWindow::open`] creates the window surface on
/// that loop and returns, leaving the caller to keep driving the multiplexed
/// loop. The egui canvas attaches lazily on the first painted frame (task
/// 2.2); task 3.x maps window interactions onto existing `App` mutations.
#[derive(Debug, Default)]
pub struct GraphWindow {
    state: WindowState,
    /// The scene spec the window draws; `None` paints an empty canvas.
    scene: Option<SceneSpec>,
    /// Window-local marquee selection (task 3.2): indices into the current
    /// scene's nodes picked by an empty-canvas drag. Task 3.3 hooks this into
    /// the shared `App` selection; here it only drives the canvas highlight.
    /// Cleared when a new scene replaces the current one (stale indices must
    /// not survive a graph rebuild).
    selected_nodes: Vec<usize>,
}

/// Reports a failed GPU init once; the window stays open but unpainted and
/// the terminal surfaces keep working (headless/software fallback, D2).
static WGPU_INIT_WARNED: std::sync::Once = std::sync::Once::new();

impl GraphWindow {
    /// A closed window handle. Does not touch the display.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the window.
    ///
    /// Creates the window on the given event loop and returns immediately;
    /// the caller's multiplexed loop keeps driving both surfaces. Rejects a
    /// window that is already open, and fails cleanly when no display is
    /// available (headless or SSH), leaving the terminal surfaces usable.
    pub fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<(), WindowError> {
        if self.is_open() {
            return Err(WindowError::new("graph window is already open"));
        }
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("droid_tui - signal-flow graph")
                    .with_inner_size(LogicalSize::new(1280.0, 800.0)),
            )
            .map_err(|err| WindowError::new(format!("could not create window: {err}")))?;
        // A stale surface must not survive a reopen; the display handle is
        // injected into the wgpu instance so Wayland-GLES can create surfaces.
        EGUI.with(|slot| *slot.borrow_mut() = None);
        DISPLAY.with(|slot| *slot.borrow_mut() = Some(event_loop.owned_display_handle()));
        WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
        self.state = WindowState::Open;
        // Pull the first frame out of winit so the surface is live.
        self.request_redraw();
        Ok(())
    }

    /// Tears down the surface and returns to [`WindowState::Closed`].
    ///
    /// The wgpu surface must die before its window (it was created from the
    /// window's raw handle), so the canvas is dropped first.
    pub fn close(&mut self) {
        EGUI.with(|slot| *slot.borrow_mut() = None);
        DISPLAY.with(|slot| *slot.borrow_mut() = None);
        WINDOW.with(|slot| *slot.borrow_mut() = None);
        self.state = WindowState::Closed;
    }

    /// Whether a window surface currently exists.
    pub fn is_open(&self) -> bool {
        self.state == WindowState::Open
    }

    /// Id of the live surface, for routing winit events to it.
    pub fn window_id(&self) -> Option<WindowId> {
        WINDOW.with(|slot| slot.borrow().as_ref().map(Window::id))
    }

    /// Feeds one winit window event into the egui input pipeline (task 3.1).
    /// The painter consumes the accumulated input via
    /// [`egui_winit::State::take_egui_input`]; forwarding here is the matching
    /// `on_window_event` path, so pointer and keyboard input reaches egui.
    /// No-op while the egui surface is not attached (window closed or not yet
    /// painted).
    pub fn window_event(&self, event: &WindowEvent) {
        WINDOW.with(|slot| {
            let window = slot.borrow();
            let Some(window) = window.as_ref() else {
                return;
            };
            let repaint = EGUI.with(|slot| {
                slot.borrow_mut()
                    .as_mut()
                    .map(|surface| surface.winit_state.on_window_event(window, event).repaint)
                    .unwrap_or(false)
            });
            // egui asks for an immediate redraw (animation, focus change, ...).
            if repaint {
                window.request_redraw();
            }
        });
    }

    /// Borrows the live window for a short-lived operation.
    ///
    /// Returns `None` while the window is closed. The painter reads the
    /// window's size and scale factor and builds its surface here.
    pub fn with_window<R>(&self, f: impl FnOnce(&Window) -> R) -> Option<R> {
        WINDOW.with(|slot| slot.borrow().as_ref().map(f))
    }

    /// Asks winit to schedule the next frame.
    pub fn request_redraw(&self) {
        self.with_window(|window| window.request_redraw());
    }

    /// Sets the scene spec the window draws on its next frame; `None` clears
    /// the canvas. Task 3.1 calls this from `App` with the shared camera and
    /// theme each frame, so both surfaces show the same graph. A scene with a
    /// different node count replaces the previous one (graph rebuilt), so the
    /// window-local marquee selection is dropped rather than left stale.
    /// Identical re-pushes (same frame, no state change) are no-ops: they
    /// neither re-clone the spec nor re-arm the redraw loop.
    pub fn set_scene(&mut self, scene: Option<&SceneSpec>) {
        let unchanged = match (&self.scene, scene) {
            (Some(prev), Some(next)) => prev == next,
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        let replaced = match (&self.scene, scene) {
            (Some(prev), Some(next)) => prev.nodes.len() != next.nodes.len(),
            _ => true,
        };
        if replaced {
            self.selected_nodes.clear();
        }
        self.scene = scene.cloned();
        self.request_redraw();
    }

    /// Paints one frame into the window (task 2.2: the egui painter backend)
    /// and reports the frame's input state for the loop to map onto `App`
    /// mutations (task 3.1).
    ///
    /// The egui/wgpu stack attaches lazily on the first frame: wgpu init is
    /// async and can fail on headless or software setups, and in that case
    /// the window simply stays unpainted while the terminal loop continues
    /// (no panic, matching the existing [`WindowError`] fallback).
    pub fn fill_placeholder(&mut self, app: &mut App) -> Option<WindowFrame> {
        self.with_window(|window| {
            if EGUI.with(|slot| slot.borrow().is_none()) {
                if let Err(err) = init_surface(window) {
                    WGPU_INIT_WARNED.call_once(|| {
                        eprintln!(
                            "[warn] graph window GPU init failed ({err}); keeping terminal surfaces"
                        );
                    });
                }
            }
            let scene = self.scene.as_ref();
            let selected = &self.selected_nodes;
            EGUI.with(|slot| {
                slot.borrow_mut()
                    .as_mut()
                    .map(|surface| surface.paint(window, app, scene, selected))
            })
        })
        .flatten()
        // Commit the frame's marquee state into the window-local selection so
        // the highlight survives the drag (task 3.2).
        .inspect(|frame| {
            self.selected_nodes = graph::next_selection(frame, &self.selected_nodes);
        })
    }
}

/// One-shot async runner for the wgpu init calls (the crate is no-async by
/// design, D1). The waker never wakes: each `poll` makes progress inside the
/// same thread, so we simply re-poll until the future completes.
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = TaskContext::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// Create the egui stack and wgpu swapchain for the live window (lazy, first
/// frame). Errors degrade to the terminal loop; they are logged once in
/// [`GraphWindow::fill_placeholder`].
fn init_surface(window: &Window) -> Result<(), String> {
    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    let scale = window.scale_factor() as f32;
    let display = DISPLAY
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| "graph window has no owned display handle".to_string())?;

    let canvas = block_on(async {
        let setup = egui_wgpu::WgpuSetup::from_display_handle(display);
        let instance = setup.new_instance().await;

        // SAFETY: the window lives in the WINDOW thread-local and is dropped
        // only after this surface (close() and destroy_window_for_panic both
        // clear the EGUI slot before WINDOW).
        let surface = unsafe {
            instance
                .create_surface_unsafe(
                    wgpu::SurfaceTargetUnsafe::from_window(window)
                        .map_err(|err| format!("window handle: {err}"))?,
                )
                .map_err(|err| format!("surface: {err}"))?
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            })
            .await
            .map_err(|err| format!("adapter: {err}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("droid_tui graph window"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|err| format!("device: {err}"))?;

        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "adapter cannot present to this surface".to_string())?;
        surface.configure(&device, &config);

        let renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok::<_, String>(WgpuCanvas {
            surface,
            config,
            device,
            queue,
            renderer,
        })
    })?;

    let context = egui::Context::default();
    context.set_visuals(themed_visuals());
    let winit_state = egui_winit::State::new(
        context.clone(),
        egui::ViewportId::ROOT,
        window,
        Some(scale),
        None,
        None,
    );
    EGUI.with(|slot| {
        *slot.borrow_mut() = Some(EguiSurface {
            context,
            winit_state,
            canvas,
            size_px: (width, height),
        });
    });
    Ok(())
}

/// The egui chrome palette, derived from the active semantic theme
/// (gpu-graph-window design D7): panel/window fills come from the graph canvas
/// background token, text from the text token, and selection/muted surfaces
/// from the accent/muted tokens. Every color flows through [`Theme::egui_color`]
/// (itself [`Theme::rgb`]) — no egui default or hardcoded RGB survives here, so
/// switching the theme in `config.toml` re-themes the window and the terminal
/// together. The scene painters ignore these and use the spec RGB directly;
/// this only styles egui's own chrome (window fill, default text, selection).
fn themed_visuals() -> egui::Visuals {
    let theme = crate::theme::active();
    let bg = theme.egui_color(theme.graph_canvas_bg);
    let text = theme.egui_color(theme.text);
    let accent = theme.egui_color(theme.accent);
    let muted = theme.egui_color(theme.muted);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = bg;
    visuals.window_fill = bg;
    visuals.extreme_bg_color = bg;
    visuals.faint_bg_color = muted.gamma_multiply(0.15);
    visuals.override_text_color = Some(text);
    visuals.selection.bg_fill = accent.gamma_multiply(0.4);
    visuals.selection.stroke.color = accent;
    visuals
}

/// The wgpu clear color for an empty canvas (design D7): the active theme's
/// graph-canvas background token, so letterboxing and the no-scene state show
/// the palette background, never a hardcoded black.
fn theme_clear_color() -> wgpu::Color {
    let theme = crate::theme::active();
    let (r, g, b) = theme.rgb(theme.graph_canvas_bg);
    wgpu::Color {
        r: r as f64 / 255.0,
        g: g as f64 / 255.0,
        b: b as f64 / 255.0,
        a: 1.0,
    }
}

/// The four quad pane rects partitioning `window` (quad-view paint dispatch):
/// a vertical divider at `main_ratio` (left/right split) and a horizontal
/// divider at `left_ratio` (the `\` split). `left_split_active` gates the
/// FILTERED pane; off, the bottom row is the FULL graph alone. Pure geometry
/// so the layout tests headless.
struct QuadRects {
    panels: egui::Rect,
    source: egui::Rect,
    full: egui::Rect,
    filtered: Option<egui::Rect>,
}

fn quad_rects(
    window: egui::Rect,
    main_ratio: f32,
    left_ratio: f32,
    left_split_active: bool,
) -> QuadRects {
    let x = window.min.x + window.width() * main_ratio.clamp(0.3, 0.7);
    let y = window.min.y + window.height() * left_ratio.clamp(0.2, 0.8);
    let panels = egui::Rect::from_min_max(window.min, egui::pos2(x, y));
    let source = egui::Rect::from_min_max(egui::pos2(x, window.min.y), egui::pos2(window.max.x, y));
    let full = if left_split_active {
        egui::Rect::from_min_max(egui::pos2(window.min.x, y), egui::pos2(x, window.max.y))
    } else {
        egui::Rect::from_min_max(egui::pos2(window.min.x, y), window.max)
    };
    let filtered =
        left_split_active.then_some(egui::Rect::from_min_max(egui::pos2(x, y), window.max));
    QuadRects {
        panels,
        source,
        full,
        filtered,
    }
}

/// Round an egui-point rect into the app's cell-grid `Rect` domain (the
/// renderer→handler geometry handoff). The quad panes publish their layout
/// here so focus hit-testing sees the same four regions as the paint.
fn to_cell_rect(r: egui::Rect) -> crate::app::Rect {
    crate::app::Rect::new(
        r.min.x.round().max(0.0) as u16,
        r.min.y.round().max(0.0) as u16,
        r.width().round().max(0.0) as u16,
        r.height().round().max(0.0) as u16,
    )
}

/// The quad panes as `(FocusSlot, cell Rect)` for focus hit-testing: the
/// Panels pane on `FocusSlot::Panels`, the Source pane on the viewer slot,
/// and both graph panes on the graph slot (the FILTERED pane shares the FULL
/// pane's focus, mirroring `App::cycle_quad_focus`).
fn quad_pane_rects(app: &App, q: &QuadRects) -> Vec<(crate::app::FocusSlot, crate::app::Rect)> {
    let viewer_slot = app
        .tile_stack
        .slots
        .iter()
        .position(|v| *v == crate::app::ViewType::SourceViewer);
    let graph_slot = app
        .tile_stack
        .slots
        .iter()
        .position(|v| *v == crate::app::ViewType::Graph);
    let mut out = vec![(crate::app::FocusSlot::Panels, to_cell_rect(q.panels))];
    if let Some(i) = viewer_slot {
        out.push((crate::app::FocusSlot::Slot(i), to_cell_rect(q.source)));
    }
    if let Some(i) = graph_slot {
        out.push((crate::app::FocusSlot::Slot(i), to_cell_rect(q.full)));
        if let Some(f) = q.filtered {
            out.push((crate::app::FocusSlot::Slot(i), to_cell_rect(f)));
        }
    }
    out
}

/// Paint the quad 2x2 layout into the window: top row Panels | Source, bottom
/// row Graph FULL | Graph FILTERED (`left_split_active` gates the FILTERED
/// pane). Publishes `pane_rects` for focus hit-testing. The caller falls
/// back to the single-pane path when quad is inactive or the window is too
/// narrow.
fn paint_quad(app: &mut App, ui: &mut egui::Ui, scene: Option<&SceneSpec>, selected: &[usize]) {
    let t = crate::theme::active();
    let window_rect = ui.max_rect();
    let q = quad_rects(
        window_rect,
        app.main_split_ratio,
        app.left_split_ratio,
        app.left_split_active,
    );
    app.pane_rects = quad_pane_rects(app, &q);
    let bg = t.egui_color(t.graph_canvas_bg);

    // Panels (top-left): real hw components grouped by controller.
    let painter = ui.painter().with_clip_rect(q.panels);
    painter.rect_filled(q.panels, 0.0, bg);
    drop(painter);
    let focused = app.quad_focus == crate::app::QuadFocus::Panels;
    let spec = panels::panels_spec(app, focused, q.panels);
    let _ = panels::paint_panels(ui, q.panels, Some(&spec));

    let ctx = ui.ctx();
    // Source (top-right): the raw/prettified viewer.
    let painter = ui.painter().with_clip_rect(q.source);
    painter.rect_filled(q.source, 0.0, bg);
    let spec = viewer::viewer_spec(app);
    let _ = viewer::paint_viewer(&painter, q.source, ctx, Some(&spec));

    // Graph FULL (bottom-left): the shared full-graph scene (influence
    // highlight/dim), clipped to the pane.
    graph::paint_scene_in(ui.painter(), q.full, scene, ctx, selected);

    // Graph FILTERED (bottom-right): the influence-induced subgraph freshly
    // fit into its pane with its own compact camera.
    if let Some(f) = q.filtered {
        let subset = graph::build_subset_scene(app, t, f);
        graph::paint_scene_in(ui.painter(), f, subset.as_ref(), ctx, selected);
    }
}

impl EguiSurface {
    /// Paint one egui frame into the window and present it, reporting the
    /// frame's input state (task 3.1) for the loop to map onto `App`
    /// mutations. Pointer positions are egui points; the scene is painted 1:1
    /// (one spec pixel = one egui point), so they double as spec-pixel coords.
    /// `selected` is the window-local marquee selection driving the highlight.
    /// `app` flows through so the quad dispatch builds real per-pane specs and
    /// publishes `pane_rects` (renderer-owns-geometry contract).
    ///
    /// Surface dispatch: in quad mode (wide enough window) this frame paints
    /// the four panes (Panels | Source / Graph FULL | Graph FILTERED);
    /// otherwise it paints the single graph canvas via
    /// [`graph::paint_scene`] with the surface dummy specs (tasks 2.1-2.5).
    fn paint(
        &mut self,
        window: &Window,
        app: &mut App,
        scene: Option<&SceneSpec>,
        selected: &[usize],
    ) -> WindowFrame {
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let scale = window.scale_factor() as f32;

        // main.rs forwards no window events yet (task 3.1 wires them), so
        // resizes are detected here and mirrored into the wgpu surface.
        if (w, h) != self.size_px {
            self.size_px = (w, h);
            self.canvas.config.width = w;
            self.canvas.config.height = h;
            self.canvas
                .surface
                .configure(&self.canvas.device, &self.canvas.config);
        }

        let mut raw_input = self.winit_state.take_egui_input(window);
        // The window is the single source of truth for size/scale until real
        // winit events flow; egui must see the current values every frame.
        raw_input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(w as f32 / scale, h as f32 / scale),
        ));
        raw_input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(scale);

        let mut full_output = self.context.run_ui(raw_input, |ui| {
            let window_rect = ui.max_rect();
            if app.is_quad() && window_rect.width() >= crate::app::QUAD_WIDTH_THRESHOLD {
                paint_quad(app, ui, scene, selected);
            } else {
                app.pane_rects.clear();
                graph::paint_scene(
                    ui,
                    window_rect.size(),
                    scene,
                    selected,
                );
            // Runtime dispatch for the remaining surfaces (native-rendering wiring):
            // physical + panels are always visible (main view); viewer/picker/overlays
            // use minimal dummy specs so headless tests and the runtime both see
            // shapes even when `App` has no patch loaded. When `App` is wired
            // through the windowed loop, these dummies are replaced by
            // `viewer::viewer_spec(app)` / `picker::picker_spec(app)` /
            // `overlays::*_spec_for(app)` payloads — the painter call shape stays
            // the same.
            let pane = ui.max_rect();
            let canvas = pane.size();
            let t = crate::theme::active();
            // Physical: one case rect + one cell so the rack geometry paints.
            {
                let dummy_physical = physical::PhysicalSpec {
                    background: t.graph_canvas_bg,
                    case_rect: egui::Rect::from_min_size(
                        egui::Pos2::new(4.0, 4.0),
                        egui::Vec2::new((canvas.x - 8.0).max(20.0), (canvas.y - 8.0).max(20.0)),
                    ),
                    mounts: Vec::new(),
                    fold_bars: Vec::new(),
                    modules: vec![physical::ModuleSpec {
                        rect: egui::Rect::from_min_size(
                            egui::Pos2::new(8.0, 8.0),
                            egui::Vec2::new(120.0, 48.0),
                        ),
                        title: "P2B8 1".to_string(),
                    }],
                    cells: vec![physical::CellSpec {
                        rect: egui::Rect::from_min_size(
                            egui::Pos2::new(12.0, 14.0),
                            egui::Vec2::new(40.0, 28.0),
                        ),
                        glyph: "\u{25CF}".to_string(),
                        label: "B1.1".to_string(),
                        state_text: "ON".to_string(),
                        color: t.button,
                        is_fader: false,
                        fader_value: 0.0,
                        global_index: 0,
                        mark: physical::PortMark::Cell,
                        highlighted: false,
                        shift_color: None,
                        kind: crate::patch::ComponentKind::Button,
                    }],
                    db8e_bands: Vec::new(),
                    grid_lines: Vec::new(),
                    cell_scale: 10.0,
                    skeleton: false,
                    paused: false,
                };
                let _ = physical::paint_physical(ui, pane, Some(&dummy_physical));
            }
            // Panels: one sub-block + one cell with a shift border.
            {
                let dummy_panels = panels::PanelsSpec {
                    title: " Panels ".to_string(),
                    focused: true,
                    modules: vec![physical::ModuleSpec {
                        rect: egui::Rect::from_min_size(
                            egui::Pos2::new(6.0, 6.0),
                            egui::Vec2::new(96.0, 38.0),
                        ),
                        title: "P2B8 1".to_string(),
                    }],
                    cells: vec![physical::CellSpec {
                        rect: egui::Rect::from_min_size(
                            egui::Pos2::new(10.0, 12.0),
                            egui::Vec2::new(40.0, 28.0),
                        ),
                        glyph: "\u{25CF}".to_string(),
                        label: "B1.1".to_string(),
                        state_text: "ON".to_string(),
                        color: t.button,
                        is_fader: false,
                        fader_value: 0.0,
                        global_index: 0,
                        mark: physical::PortMark::Cell,
                        highlighted: false,
                        shift_color: Some(t.shift2),
                        kind: crate::patch::ComponentKind::Button,
                    }],
                    paused: false,
                };
                let _ = panels::paint_panels(ui, pane, Some(&dummy_panels));
            }
            // Viewer: one raw line + status so the content column paints.
            let ctx = ui.ctx();
            {
                let dummy_viewer = viewer::ViewerSpec {
                    title: " Source [raw] ".to_string(),
                    focused: true,
                    lines: vec![viewer::LineSpec {
                        text: "[motorfader]".to_string(),
                        fragments: vec![],
                    }],
                    scroll: 0,
                    empty_message: None,
                    sidebar: None,
                    minimap: None,
                    status: vec![viewer::StatusFragment {
                        text: "Source Viewer".to_string(),
                        color: t.text,
                        bold: true,
                    }],
                };
                let _ = viewer::paint_viewer(ui.painter(), pane, ctx, Some(&dummy_viewer));
            }
            // Picker: one favourite row + one listing row.
            {
                let dummy_picker = picker::PickerSpec {
                    title: " File Picker ".to_string(),
                    picker_dir: "/tmp".to_string(),
                    filter: None,
                    rows: vec![
                        picker::PickerRow {
                            label: "★ patch.ini".to_string(),
                            is_favourite: true,
                            is_dir: false,
                            is_parent: false,
                        },
                        picker::PickerRow {
                            label: "other.ini".to_string(),
                            is_favourite: false,
                            is_dir: false,
                            is_parent: false,
                        },
                    ],
                    selected: 0,
                    has_favourites: false,
                    fav_count: 0,
                };
                let _ = picker::paint_picker(ui.painter(), canvas, ctx, Some(&dummy_picker));
            }
            // Overlays: each overlay paints its chrome at its canvas size.
            {
                let dummy_validation = overlays::ValidationSpec {
                    title: " Validation (1) 1E 0W 0H ".to_string(),
                    hint: " e:toggle j/k:navigate Enter:jump Esc:close ".to_string(),
                    rows: vec![overlays::ValidationRow {
                        location: "L1:1".to_string(),
                        severity: crate::validation::Severity::Error,
                        code: "unknown_circuit".to_string(),
                        message: "unknown circuit foo".to_string(),
                        selected: true,
                    }],
                    empty_message: None,
                };
                let _ = overlays::paint_validation_modal(
                    ui.painter(),
                    canvas,
                    ctx,
                    Some(&dummy_validation),
                );
                let dummy_select = overlays::SelectMenuSpec {
                    title: " Select state (1) ".to_string(),
                    hint: " j/k:navigate [/]:cycle Esc:clear ".to_string(),
                    rows: vec![overlays::SelectRow {
                        signal: "sel".to_string(),
                        kind_label: "register".to_string(),
                        usage: 2,
                        candidates: "0, 1".to_string(),
                        current: "1".to_string(),
                        selected: true,
                    }],
                    empty_message: None,
                };
                overlays::paint_select_menu(ui.painter(), canvas, ctx, Some(&dummy_select));
                let dummy_label = overlays::LabelEditSpec {
                    draft: "MyLabel".to_string(),
                    hint: "Enter save | Esc cancel | 1..4 layer".to_string(),
                    hue: Some(t.shift1),
                };
                overlays::paint_label_editor(ui.painter(), canvas, ctx, Some(&dummy_label));
                let dummy_diff = overlays::DiffSpec {
                    title: " Diff (1) ".to_string(),
                    added: vec!["_CABLE_A".to_string()],
                    removed: vec![],
                    changed: vec![],
                };
                overlays::paint_diff_surface(ui.painter(), canvas, ctx, Some(&dummy_diff));
                let dummy_optimizer = overlays::OptimizerSpec {
                    header: " Optimizer (1) \u{00B7} w = 0.5 ".to_string(),
                    hint:
                        " j/k select \u{00B7} Enter preview \u{00B7} r restore \u{00B7} s export \u{00B7} Esc close "
                            .to_string(),
                    rows: vec![overlays::OptimizerRow {
                        label: "\u{25B6} candidate 1".to_string(),
                        weighted_obj: 1.23,
                        avg_before: 2.0,
                        avg_after: 1.5,
                        max_before: 5.0,
                        max_after: 3.0,
                        selected: true,
                    }],
                    empty_message: None,
                };
                overlays::paint_optimizer(ui.painter(), canvas, ctx, Some(&dummy_optimizer));
            }
            }
        });

        // Task 3.1: snapshot the processed input so the loop can drive `App`
        // mutations (hit-testing happens in handler.rs against the shared
        // camera). `e` requires no modifiers, mirroring the terminal `e`;
        // `x`/`p` have no modifier guard there either.
        let window_frame = self.context.input(|i| {
            let pointer = i.pointer.latest_pos().map(|p| (p.x, p.y));
            let mut keys = Vec::new();
            if i.key_pressed(egui::Key::X) {
                keys.push(WindowGraphKey::ToggleProcessing);
            }
            if i.key_pressed(egui::Key::P) {
                keys.push(WindowGraphKey::TogglePin);
            }
            if i.key_pressed(egui::Key::E)
                && !(i.modifiers.shift
                    || i.modifiers.ctrl
                    || i.modifiers.alt
                    || i.modifiers.command)
            {
                keys.push(WindowGraphKey::BeginEdit);
            }
            // Task 3.2/3.3: middle-drag pans the shared camera, wheel zooms
            // about the cursor, and an empty-canvas left drag selects nodes.
            // `smooth_scroll_delta.y` is positive when scrolling down (zoom
            // out), so the negation makes wheel-up zoom in.
            let pan_delta = if i.pointer.middle_down() {
                let d = i.pointer.delta();
                (d.x, d.y)
            } else {
                (0.0, 0.0)
            };
            let zoom = if i.smooth_scroll_delta.y.abs() > f32::EPSILON {
                let factor = ((-i.smooth_scroll_delta.y) * graph::ZOOM_SENSITIVITY).exp();
                let factor = factor.clamp(1.0 / graph::MAX_ZOOM_STEP, graph::MAX_ZOOM_STEP);
                pointer.map(|p| (factor, p))
            } else {
                None
            };
            WindowFrame {
                pointer,
                primary_pressed: i.pointer.primary_pressed(),
                primary_down: i.pointer.primary_down(),
                primary_released: i.pointer.primary_released(),
                keys,
                pan_delta,
                zoom,
                marquee: graph::frame_marquee(scene, i),
            }
        });
        self.winit_state
            .handle_platform_output(window, full_output.platform_output);

        let pixels_per_point = full_output.pixels_per_point;
        let clipped = self
            .context
            .tessellate(full_output.shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point,
        };
        // Canvas clear color (design D7): with a scene it is the scene's
        // already-themed background; without one (empty canvas / letterboxed
        // resize) it falls back to the active theme's graph-canvas token so no
        // egui/wgpu default or hardcoded RGB ever shows through.
        let clear = scene.map_or(theme_clear_color(), |spec| wgpu::Color {
            r: spec.background.0 as f64 / 255.0,
            g: spec.background.1 as f64 / 255.0,
            b: spec.background.2 as f64 / 255.0,
            a: 1.0,
        });

        let mut encoder = self
            .canvas
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let user_cmd_bufs = self.canvas.renderer.update_buffers(
            &self.canvas.device,
            &self.canvas.queue,
            &mut encoder,
            &clipped,
            &screen,
        );
        for (id, deltas) in &full_output.textures_delta.set {
            for delta in deltas {
                self.canvas.renderer.update_texture(
                    &self.canvas.device,
                    &self.canvas.queue,
                    *id,
                    delta,
                );
            }
        }

        let frame = match self.canvas.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            // The surface no longer matches its config; reconfigure now so
            // the next frame presents correctly.
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.canvas.config.width = w;
                self.canvas.config.height = h;
                self.canvas
                    .surface
                    .configure(&self.canvas.device, &self.canvas.config);
                frame
            }
            other => {
                // Lost/outdated/occluded: skip this frame; the next redraw
                // (or resize reconfiguration) retries. Input is still valid.
                eprintln!("[warn] graph window frame acquire failed: {other:?}");
                return window_frame;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui graph canvas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.canvas
                .renderer
                .render(&mut render_pass.forget_lifetime(), &clipped, &screen);
        }
        self.canvas
            .queue
            .submit(user_cmd_bufs.into_iter().chain([encoder.finish()]));
        self.canvas.queue.present(frame);
        // Textures marked for destruction are only freed once the frame that
        // might still reference them has been submitted.
        for id in &full_output.textures_delta.free {
            self.canvas.renderer.free_texture(id);
        }
        // update_texture/free_texture take references and never consume the
        // delta entries, so full_output.textures_delta stays populated after
        // this point. TexturesDelta's Drop asserts it is empty (debug builds
        // only), so clear it explicitly once both deltas are applied.
        full_output.textures_delta.clear();

        // Keep the multiplexed loop hot for egui animations/repaint requests.
        if self.context.has_requested_repaint() {
            window.request_redraw();
        }
        window_frame
    }
}

/// Destroys the graph window surface and its canvas; called by the panic hook
/// in `main.rs` so a panic cannot leave an orphaned window on the desktop.
///
/// Uses `try_borrow_mut` so a panic that happens while a slot is borrowed
/// cannot recurse; unwinding drops the surface anyway. The wgpu canvas dies
/// before the window, matching the surface's raw-handle safety contract.
pub fn destroy_window_for_panic() {
    EGUI.with(|slot| {
        if let Ok(mut handle) = slot.try_borrow_mut() {
            *handle = None;
        }
    });
    DISPLAY.with(|slot| {
        if let Ok(mut handle) = slot.try_borrow_mut() {
            *handle = None;
        }
    });
    WINDOW.with(|slot| {
        if let Ok(mut handle) = slot.try_borrow_mut() {
            *handle = None;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn clean_window_frame_defaults_to_no_gesture() {
        // A fresh frame carries no pointer, no pan/zoom, and no marquee: the
        // loop must not act on state a clean frame does not provide.
        let frame = WindowFrame::default();
        assert_eq!(frame.pointer, None);
        assert!(!frame.primary_pressed);
        assert!(!frame.primary_down);
        assert!(!frame.primary_released);
        assert!(frame.keys.is_empty());
        assert_eq!(frame.pan_delta, (0.0, 0.0));
        assert_eq!(frame.zoom, None);
        assert_eq!(frame.marquee, None);
    }

    #[test]
    fn graph_window_new_stays_closed_without_an_event_loop() {
        // `GraphWindow::new()` must not touch the display or build an event
        // loop (winit permits one loop per process), so under `cargo test` a
        // fresh handle stays closed and windowless.
        let window = GraphWindow::new();
        assert!(!window.is_open());
        assert_eq!(window.window_id(), None);
        assert!(window.with_window(|_| true).is_none());
    }

    #[test]
    fn quad_rects_partition_the_window_without_overlap() {
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let q = quad_rects(window, 0.6, 0.5, true);
        // The vertical divider lands at 0.6 and the horizontal at 0.5
        // (f32 arithmetic: assert with tolerance).
        assert!((q.panels.max.x - 480.0).abs() < 0.01);
        assert!((q.panels.max.y - 300.0).abs() < 0.01);
        assert!((q.source.min.x - 480.0).abs() < 0.01);
        assert!((q.full.min.y - 300.0).abs() < 0.01);
        // With the split active the FULL graph is the bottom-left quadrant.
        assert!((q.full.max.x - 480.0).abs() < 0.01);
        assert_eq!(q.full.max.y, window.max.y);
        let f = q.filtered.expect("split active -> filtered pane");
        assert!((f.min.x - 480.0).abs() < 0.01);
        assert!((f.min.y - 300.0).abs() < 0.01);
        assert_eq!(f.max, window.max);
        // The four panes tile the window exactly, with no gaps or overlap.
        let mut area = 0.0;
        for r in [q.panels, q.source, q.full, f] {
            assert!(window.contains_rect(r));
            area += r.width() * r.height();
        }
        assert!((area - window.width() * window.height()).abs() < 0.5);
    }

    #[test]
    fn quad_rects_hide_filtered_pane_when_split_off() {
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let q = quad_rects(window, 0.6, 0.5, false);
        assert!(q.filtered.is_none());
        // The FULL graph spans the whole bottom row.
        assert_eq!(q.full.min.y, 300.0);
        assert_eq!(q.full.max, window.max);
    }

    #[test]
    fn quad_rects_clamp_ratios() {
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1000.0));
        let q = quad_rects(window, 0.9, 0.1, true);
        // Out-of-range ratios clamp so panes never collapse to zero width:
        // 0.9 -> 0.7, 0.1 -> 0.2.
        assert_eq!(q.panels.max.x, 700.0);
        assert_eq!(q.panels.max.y, 200.0);
    }

    #[test]
    fn quad_pane_rects_map_slots_and_both_graph_panes() {
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        assert!(app.enter_quad());
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let q = quad_rects(
            window,
            app.main_split_ratio,
            app.left_split_ratio,
            app.left_split_active,
        );
        let rects = quad_pane_rects(&app, &q);
        assert_eq!(rects.len(), 4);
        assert_eq!(
            rects[0],
            (crate::app::FocusSlot::Panels, to_cell_rect(q.panels))
        );
        let viewer_slot = app
            .tile_stack
            .slots
            .iter()
            .position(|v| *v == crate::app::ViewType::SourceViewer)
            .unwrap();
        let graph_slot = app
            .tile_stack
            .slots
            .iter()
            .position(|v| *v == crate::app::ViewType::Graph)
            .unwrap();
        assert_eq!(
            rects[1],
            (
                crate::app::FocusSlot::Slot(viewer_slot),
                to_cell_rect(q.source)
            )
        );
        assert_eq!(
            rects[2],
            (
                crate::app::FocusSlot::Slot(graph_slot),
                to_cell_rect(q.full)
            )
        );
        assert_eq!(
            rects[3],
            (
                crate::app::FocusSlot::Slot(graph_slot),
                to_cell_rect(q.filtered.unwrap())
            )
        );
    }

    #[test]
    fn paint_quad_draws_four_panes_headless() {
        // The quad dispatch runs inside a real egui frame headless: shapes
        // land for every pane (Panels + Source top, FULL + FILTERED bottom)
        // and the four pane rects publish for focus hit-testing.
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.select_component(String::from("B1.1"));
        assert!(app.enter_quad());
        assert!(app.influence_subset.is_some());
        let ctx = egui::Context::default();
        let scene = crate::gui::graph::build_scene_spec(&app, crate::theme::active());
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw, |ui| {
            paint_quad(&mut app, ui, scene.as_ref(), &[]);
        });
        assert!(
            !full_output.shapes.is_empty(),
            "quad paint must emit shapes for all four panes"
        );
        let labels: Vec<String> = full_output
            .shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::epaint::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect();
        assert!(
            labels.iter().any(|l| l.contains("Panels")),
            "panels title missing: {labels:?}"
        );
        assert_eq!(app.pane_rects.len(), 4, "quad publishes four pane rects");
        full_output.textures_delta.clear();
    }
}

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

pub use graph::{build_scene_spec, graph_pane_rect};

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
    /// `c`: center the drawn graph in the pane (design D3, mirror of the
    /// terminal graph surface's bare `c`). Acts on the pane, not a node.
    CenterGraph,
    /// `Shift+c`: refit the camera against the pane size (design D3/D4,
    /// mirror of the terminal graph surface's `Shift+c`).
    FitGraph,
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
        // Request keyboard focus/activation immediately: on Wayland a freshly
        // created window maps unfocused unless it asks for it, which left the
        // DROID keybindings dead at startup until the user clicked the window
        // (bead droid_tui-5u9).
        window.focus_window();
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

/// Round an egui-point rect into the app's cell-grid `Rect` domain (the
/// renderer→handler geometry handoff). The paint derives the arrangement
/// band here before `App::pane_geometry` partitions it.
fn to_cell_rect(r: egui::Rect) -> crate::app::Rect {
    crate::app::Rect::new(
        r.min.x.round().max(0.0) as u16,
        r.min.y.round().max(0.0) as u16,
        r.width().round().max(0.0) as u16,
        r.height().round().max(0.0) as u16,
    )
}

/// Pane frame (tiled-window-manager D6): the border stroke carries the focus
/// token when the pane is focused and the unfocused token otherwise, plus the
/// pane title in the same color. Panels, viewer, and graph tiles draw their
/// pane chrome through this one source.
pub(crate) fn draw_pane_frame(
    painter: &egui::Painter,
    rect: egui::Rect,
    focused: bool,
    title: &str,
    t: &crate::theme::Theme,
) {
    let border = t.egui_color(if focused {
        t.pane_focus_border
    } else {
        t.pane_unfocused_border
    });
    painter.rect(
        rect,
        0.0,
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(if focused { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Inside,
    );
    if !title.is_empty() {
        painter.text(
            rect.min + egui::vec2(6.0, 3.0),
            egui::Align2::LEFT_TOP,
            title,
            egui::FontId::proportional(12.0),
            border,
        );
    }
}

/// Whether the band has anything to paint: a maximized pane, or at least one
/// pane holding a view. Panels counts here — the module UI is a view in the
/// class layout (design D2), not a permanent left pane, so a band showing only
/// the module UI still paints through `paint_panes` instead of falling back to
/// the empty-band graph canvas.
fn band_has_views(app: &App, band: crate::app::Rect) -> bool {
    app.layout.maximized.is_some()
        || app
            .pane_geometry(band)
            .iter()
            .any(|(id, _)| app.layout.pane(*id).view.is_some())
}

/// Paint the class-based main band (`pane-class-layout`, design D1): every
/// pane of the current arrangement (`App::pane_geometry` — the left big pane
/// plus either a second big pane or two small panes, and the maximize override
/// that collapses the band to the focused pane) paints its view into its cell.
/// Each pane's border carries the `pane_focus_border` token when it holds
/// `App.layout.focus` and `pane_unfocused_border` otherwise; the panels and
/// viewer surfaces draw their own frame, the graph, physical, and optimizer
/// panes get one here. An empty pane stays a bare background. Publishes
/// `pane_hit_rects` (the drawn geometry, ADR 35) and the legacy `FocusSlot`
/// mirror `pane_rects` for handler/main compat via `App::refresh_hit_geometry`.
fn paint_panes(app: &mut App, ui: &mut egui::Ui, scene: Option<&SceneSpec>, selected: &[usize]) {
    let t = crate::theme::active();
    let bg = t.egui_color(t.graph_canvas_bg);
    // ADR 35: the panes painted here are the hit geometry, so both handoffs
    // derive from the one `pane_geometry` source.
    app.refresh_hit_geometry(to_cell_rect(ui.max_rect()));
    let panes = app.pane_hit_rects.clone();

    for (id, cell) in panes {
        let rect = egui::Rect::from_min_size(
            egui::pos2(cell.x as f32, cell.y as f32),
            egui::vec2(cell.width as f32, cell.height as f32),
        );
        let focused = app.layout.focus == id;
        // Every pane owns its background so an empty pane reads as a
        // deliberate region of the arrangement rather than showing the
        // previous frame through.
        ui.painter().with_clip_rect(rect).rect_filled(rect, 0.0, bg);
        match app.layout.pane(id).view {
            Some(crate::app::ViewType::Physical) => {
                // Module UI: real hw components grouped by controller. The
                // physical surface paints the rack, not its own pane chrome, so
                // this pane draws the shared focus frame like the graph and
                // optimizer panes (spec pane-class-layout: every pane border
                // carries the focus/unfocused token).
                let spec = physical::physical_spec(app);
                let _ = physical::paint_physical(ui, rect, spec.as_ref());
                let painter = ui.painter().with_clip_rect(rect);
                draw_pane_frame(&painter, rect, focused, " Module UI ", t);
            }
            Some(crate::app::ViewType::Graph) => {
                // The pane is the visible graph canvas: publish its size and
                // seed the camera on the first frame (design D1/D2).
                app.graph_canvas_px = Some((rect.width(), rect.height()));
                if app.graph_camera.is_none() {
                    app.fit_graph_camera((rect.width(), rect.height()));
                }
                graph::paint_scene_in(ui, rect, scene, selected);
                // Publish graph minimap rect and transform for click-to-navigate hit testing.
                if let Some(spec) = scene {
                    if let Some(minimap) = graph::minimap_layout(spec, rect) {
                        let panel_egui = egui::Rect::from_min_size(
                            egui::pos2(minimap.panel.0, minimap.panel.1),
                            egui::vec2(minimap.panel.2, minimap.panel.3),
                        );
                        app.graph_minimap_rect = Some(to_cell_rect(panel_egui));
                        app.graph_minimap_panel_egui = Some(panel_egui);
                        app.graph_minimap_transform = Some(minimap.transform);
                    } else {
                        app.graph_minimap_rect = None;
                        app.graph_minimap_panel_egui = None;
                        app.graph_minimap_transform = None;
                    }
                } else {
                    app.graph_minimap_rect = None;
                    app.graph_minimap_panel_egui = None;
                    app.graph_minimap_transform = None;
                }
                // The graph scene paints no pane chrome of its own, so the
                // pane frame marks it like the panels and viewer panes.
                let painter = ui.painter().with_clip_rect(rect);
                draw_pane_frame(&painter, rect, focused, "", t);
            }
            Some(crate::app::ViewType::SourceViewer) => {
                let painter = ui.painter().with_clip_rect(rect);
                let spec = viewer::viewer_spec(app);
                let _ = viewer::paint_viewer(&painter, rect, ui.ctx(), Some(&spec));
            }
            Some(crate::app::ViewType::Optimizer) => {
                // The optimizer lives in its own small pane (spec "Optimizer
                // as side pane"): fill the pane with its candidate list and
                // frame it, never as a card over another pane.
                let painter = ui.painter().with_clip_rect(rect);
                let spec = overlays::optimizer_spec(app);
                overlays::paint_optimizer(&painter, rect, spec.as_ref());
                let painter = ui.painter().with_clip_rect(rect);
                draw_pane_frame(&painter, rect, focused, "", t);
            }
            // An empty pane stays bare.
            None => {}
        }
    }
}

/// Dispatch the overlays over the base surface, bottom of the z-order first:
/// diff, validation, select, picker, label edit, help. Called after the base
/// paint. The optimizer is NOT here — it lives in its own small pane
/// (`paint_panes`).
fn paint_overlays(app: &App, ui: &mut egui::Ui) {
    let painter = ui.painter();
    // Center overlays on the window, not on `ui.max_rect()`: the base paint
    // allocates graph-node rects in scene coordinates, and `allocate_rect`
    // expands the Ui's max_rect to include them, which would drag a centered
    // modal off to the side. `viewport_rect()` is the canvas `paint` published
    // and is unaffected by that expansion.
    let canvas = ui.ctx().viewport_rect().size();
    let ctx = ui.ctx();
    if let Some(spec) = overlays::diff_spec_for(app) {
        overlays::paint_diff_surface(painter, canvas, ctx, Some(&spec));
    }
    if let Some(spec) = overlays::validation_spec_for(app) {
        let _ = overlays::paint_validation_modal(painter, canvas, ctx, Some(&spec));
    }
    if let Some(spec) = overlays::select_menu_spec(app) {
        overlays::paint_select_menu(painter, canvas, ctx, Some(&spec));
    }
    if app.showing_picker {
        let spec = picker::picker_spec(app);
        let _ = picker::paint_picker(painter, canvas, ctx, Some(&spec));
    }
    if let Some(spec) = overlays::label_edit_spec(app) {
        overlays::paint_label_editor(painter, canvas, ctx, Some(&spec));
    }
    if app.showing_help {
        overlays::paint_help(painter, canvas, crate::help::active_view(app));
    }
}

/// Publish the window canvas size in points and seed the first-frame camera
/// fit when none exists (design D1/D2). The window paint path runs without
/// the loop's RedrawRequested seed, so it re-seeds the same
/// dependency-filter aware fit; every frame it also refreshes
/// `graph_canvas_px` for zoom anchoring and arrow-pan gating. Mirrors the
/// window-size fallback in main.rs.
fn publish_window_canvas(app: &mut App, canvas_px: (f32, f32)) {
    // A pane-local graph (graph tile slot or quad FULL pane) publishes and
    // seeds its own pane size from the paint path. Writing the full-window
    // canvas here would clobber it every frame before c / Shift+c, wheel
    // zoom, and arrow-pan gating read the one source of truth, so the window
    // canvas is published and the fit seeded only when the scene fills the
    // whole window.
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(canvas_px.0, canvas_px.1));
    if graph::graph_pane_rect(app, window).is_some() {
        return;
    }
    app.graph_canvas_px = Some(canvas_px);
    if app.graph_camera.is_none() {
        let fit: Vec<(f32, f32)> = if app.dependency_nodes.is_empty() {
            app.graph_positions.clone()
        } else {
            app.dependency_nodes
                .iter()
                .map(|&i| app.graph_positions[i])
                .collect()
        };
        app.graph_camera = Some(graph_window_fit_camera(&fit, canvas_px));
    }
}

impl EguiSurface {
    /// Paint one egui frame into the window and present it, reporting the
    /// frame's input state (task 3.1) for the loop to map onto `App`
    /// mutations. Pointer positions are egui points; the scene is painted 1:1
    /// (one spec pixel = one egui point), so they double as spec-pixel coords.
    /// `selected` is the window-local marquee selection driving the highlight.
    /// `app` flows through so the base paint builds real per-pane specs and
    /// publishes `pane_rects` (renderer-owns-geometry contract).
    ///
    /// Surface dispatch: every pane of the layout arrangement paints its view
    /// (Panels, source viewer, physical, graph, optimizer); with no view open
    /// in any pane the whole window is the bare graph canvas via
    /// [`graph::paint_scene`].
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

        // The canvas is the visible graph pane: publish its size in points
        // every frame and seed the first-frame fit (design D1/D2).
        publish_window_canvas(app, (w as f32 / scale, h as f32 / scale));

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

        // Extract keyboard events from raw_input BEFORE run_ui consumes them.
        // Keyboard events are one-shot in egui — key_pressed() only returns true
        // for the frame the event arrived. After run_ui takes ownership of
        // raw_input, the keyboard events are gone. Pointer state (position,
        // button state) is accumulated and survives, so it can be read after.
        let window_keys: Vec<WindowGraphKey> = raw_input
            .events
            .iter()
            .filter_map(|e| {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                {
                    let no_mod =
                        !(modifiers.shift || modifiers.ctrl || modifiers.alt || modifiers.command);
                    match key {
                        egui::Key::X if no_mod => Some(WindowGraphKey::ToggleProcessing),
                        egui::Key::P if no_mod => Some(WindowGraphKey::TogglePin),
                        egui::Key::E if no_mod => Some(WindowGraphKey::BeginEdit),
                        // Design D3: `c` centers the pane camera, Shift+c
                        // refits against the pane size — mirror of the terminal
                        // graph keys. Ctrl+C quits in the DROID key dispatch
                        // before egui sees the event, and this guard still
                        // excludes ctrl/alt/command so the mapping never
                        // shadows the quit.
                        egui::Key::C if !modifiers.ctrl && !modifiers.alt && !modifiers.command => {
                            Some(if modifiers.shift {
                                WindowGraphKey::FitGraph
                            } else {
                                WindowGraphKey::CenterGraph
                            })
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect();

        let mut full_output = self.context.run_ui(raw_input, |ui| {
            let window_rect = ui.max_rect();
            let band = to_cell_rect(window_rect);
            // ADR 35: the arrangement decides the paint. With nothing open
            // (no view in any pane and no maximize) the whole window is the
            // bare graph canvas; otherwise every pane of the arrangement
            // paints. Panels is a view, so it counts like any other (D2).
            if band_has_views(app, band) {
                paint_panes(app, ui, scene, selected);
            } else {
                app.pane_rects.clear();
                graph::paint_scene(ui, window_rect.size(), scene, selected);
            }
            paint_overlays(app, ui);
        });

        // Task 3.1: snapshot the processed input so the loop can drive `App`
        // mutations (hit-testing happens in handler.rs against the shared
        // camera). `e` requires no modifiers, mirroring the terminal `e`;
        // `x`/`p` have no modifier guard there either.
        let window_frame = self.context.input(|i| {
            let pointer = i.pointer.latest_pos().map(|p| (p.x, p.y));
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
                keys: window_keys,
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
    fn overlays_center_on_the_viewport_after_graph_nodes_expand_the_ui() {
        // Regression: the base paint allocates graph-node rects in scene
        // coordinates, and egui's `allocate_rect` expands `ui.max_rect()` to
        // include them. The overlay canvas must come from the viewport, or a
        // centered modal is dragged off to the side of the window.
        crate::theme::set_test_theme(Some(crate::theme::Theme::classic()));
        let mut app = crate::app::App::new();
        app.showing_help = true;
        let ctx = egui::Context::default();
        let canvas = egui::vec2(1280.0, 800.0);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, canvas)),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            let _ = ui.allocate_rect(
                egui::Rect::from_min_size(egui::pos2(3000.0, 20.0), egui::vec2(60.0, 40.0)),
                egui::Sense::hover(),
            );
            assert!(
                ui.max_rect().width() > canvas.x,
                "base paint expanded max_rect: {:?}",
                ui.max_rect()
            );
            paint_overlays(&app, ui);
        });
        let t = crate::theme::active();
        let border = t.egui_color(t.validation_modal_border);
        let fill = t.egui_color(t.muted);
        let mut border_rect = None;
        let mut fill_alpha = None;
        for cs in &out.shapes {
            if let egui::epaint::Shape::Rect(r) = &cs.shape {
                if r.stroke.color == border && border_rect.is_none() {
                    border_rect = Some(r.rect);
                }
                if r.fill == fill && r.rect.width() > 400.0 {
                    fill_alpha = Some(r.fill.a());
                }
            }
        }
        let rect = border_rect.expect("help modal border drawn");
        assert!(
            (rect.center().x - canvas.x / 2.0).abs() < 0.5,
            "modal centered on the viewport: {:?}",
            rect
        );
        assert!(
            (rect.center().y - canvas.y / 2.0).abs() < 0.5,
            "modal centered on the viewport: {:?}",
            rect
        );
        assert_eq!(fill_alpha, Some(255), "modal fill is opaque");
        out.textures_delta.clear();
        crate::theme::set_test_theme(None);
    }

    /// Build the full graph scene for a paint test (the scene rect only frames
    /// the graph content; each pane publishes its own canvas during paint).
    fn scene_for(app: &App, win: egui::Rect) -> Option<SceneSpec> {
        crate::gui::graph::build_scene_spec(
            app,
            crate::theme::active(),
            crate::gui::graph::graph_pane_rect(app, win).unwrap_or(win),
        )
    }

    /// Run `paint_panes` once inside a headless egui context over an 800x600
    /// band and return the frame output for shape/label assertions.
    fn run_paint_panes(app: &mut App) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let win = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let raw = egui::RawInput {
            screen_rect: Some(win),
            ..Default::default()
        };
        let scene = scene_for(app, win);
        let mut out = ctx.run_ui(raw, |ui| {
            paint_panes(app, ui, scene.as_ref(), &[]);
        });
        // Drop the delta here, not at the end of each test: epaint panics when
        // a FullOutput with unapplied deltas drops, which turns one failed
        // assertion below into a process abort that hides every other result.
        out.textures_delta.clear();
        out
    }

    fn text_labels(out: &egui::FullOutput) -> Vec<String> {
        out.shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::epaint::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn paint_panes_renders_small_arrangement_headless() {
        // Startup arrangement (spec "Startup pane configuration"): the module
        // UI in the left big pane, the source viewer in the first small pane,
        // and the second small pane empty.
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        assert!(
            app.layout.has_small_view(),
            "startup is the small arrangement"
        );

        let mut out = run_paint_panes(&mut app);
        let labels = text_labels(&out);
        assert!(
            labels.iter().any(|l| l.contains("Module UI")),
            "module UI title missing: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("Source")),
            "viewer title missing: {labels:?}"
        );
        assert_eq!(
            app.pane_hit_rects.len(),
            3,
            "three panes in the small arrangement"
        );
        assert_eq!(
            app.pane_rects.len(),
            2,
            "panels + source viewer are hit-testable"
        );
        // Geometry: left big pane 0..400, the right half split into two
        // quarter small panes top and bottom (main and small ratios at 0.5).
        let band = to_cell_rect(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));
        let geom = app.pane_geometry(band);
        assert_eq!(
            geom[0],
            (
                crate::panes::PaneId::BigLeft,
                crate::app::Rect::new(0, 0, 400, 600)
            )
        );
        assert_eq!(
            geom[1],
            (
                crate::panes::PaneId::SmallTop,
                crate::app::Rect::new(400, 0, 400, 300)
            )
        );
        assert_eq!(
            geom[2],
            (
                crate::panes::PaneId::SmallBottom,
                crate::app::Rect::new(400, 300, 400, 300)
            )
        );
        out.textures_delta.clear();
    }

    #[test]
    fn paint_panes_renders_big_arrangement_headless() {
        // No small-class view open: the right half is one big pane, so the band
        // is two equal big panes side by side (design D1).
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.open_graph(); // graph lands in the left big pane
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.layout.big_right.view = Some(crate::app::ViewType::Physical);
        app.layout.focus = crate::panes::PaneId::BigRight;
        assert!(!app.layout.has_small_view(), "big arrangement");

        let mut out = run_paint_panes(&mut app);
        assert!(
            !out.shapes.is_empty(),
            "big arrangement must paint both panes"
        );
        assert_eq!(app.pane_hit_rects.len(), 2, "two big panes");
        assert!(app
            .pane_hit_rects
            .iter()
            .any(|(id, _)| *id == crate::panes::PaneId::BigLeft));
        let (_, right) = app
            .pane_hit_rects
            .iter()
            .find(|(id, _)| *id == crate::panes::PaneId::BigRight)
            .expect("right big pane published");
        assert_eq!(*right, crate::app::Rect::new(400, 0, 400, 600));
        out.textures_delta.clear();
    }

    #[test]
    fn paint_panes_maximize_override_fills_the_band() {
        // `z` maximizes the focused pane to the full band (non-latching display
        // state): the arrangement collapses to that one pane.
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.open_graph(); // graph lands in the left big pane, focused
        app.layout.maximized = Some(crate::panes::PaneId::BigLeft);

        let mut out = run_paint_panes(&mut app);
        assert_eq!(
            app.pane_hit_rects.len(),
            1,
            "maximize collapses to one pane"
        );
        assert_eq!(
            app.pane_hit_rects[0],
            (
                crate::panes::PaneId::BigLeft,
                crate::app::Rect::new(0, 0, 800, 600)
            ),
            "maximized pane fills the whole band"
        );
        // The graph publishes the full band as its canvas.
        assert_eq!(app.graph_canvas_px, Some((800.0, 600.0)));
        // The hidden source viewer must not paint while the graph is maximized.
        let labels = text_labels(&out);
        assert!(
            !labels.iter().any(|l| l.contains("Source")),
            "hidden panes do not paint: {labels:?}"
        );
        out.textures_delta.clear();
    }

    #[test]
    fn paint_panes_focus_border_tokens_headless() {
        // The focused pane's frame uses `pane_focus_border` (2px); every other
        // pane uses `pane_unfocused_border` (1px) (spec "Focus border marks the
        // active pane"). Assert by pane identity, not just color presence.
        let t = crate::theme::active();
        let focus = t.egui_color(t.pane_focus_border);
        let unfocused = t.egui_color(t.pane_unfocused_border);

        // The two big-pane rects in an 800x600 band with the boundary at 0.5.
        let left = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 600.0));
        let right = egui::Rect::from_min_size(egui::pos2(400.0, 0.0), egui::vec2(400.0, 600.0));
        let strokes_at = |out: &egui::FullOutput, target: egui::Rect| -> Vec<egui::Stroke> {
            out.shapes
                .iter()
                .filter_map(|cs| match &cs.shape {
                    egui::epaint::Shape::Rect(r)
                        if r.stroke.width > 0.0
                            && (r.rect.min - target.min).length() < 1.0
                            && (r.rect.max - target.max).length() < 1.0 =>
                    {
                        Some(r.stroke)
                    }
                    _ => None,
                })
                .collect()
        };

        // Big arrangement: graph left, physical right (both panes frame through
        // `draw_pane_frame`), focus the graph.
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.open_graph();
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        app.layout.big_right.view = Some(crate::app::ViewType::Physical);
        app.layout.focus = crate::panes::PaneId::BigLeft;

        let mut out = run_paint_panes(&mut app);
        let left_strokes = strokes_at(&out, left);
        let right_strokes = strokes_at(&out, right);
        assert!(
            left_strokes
                .iter()
                .any(|s| s.color == focus && s.width == 2.0),
            "focused graph pane frame uses the focus token at 2px: {left_strokes:?}"
        );
        assert!(
            right_strokes
                .iter()
                .any(|s| s.color == unfocused && s.width == 1.0),
            "unfocused physical pane frame uses the unfocused token at 1px: {right_strokes:?}"
        );

        // Move focus to the right pane: the tokens swap by pane identity.
        app.layout.focus = crate::panes::PaneId::BigRight;
        let mut out2 = run_paint_panes(&mut app);
        assert!(
            strokes_at(&out2, right).iter().any(|s| s.color == focus),
            "now-focused physical pane uses the focus token"
        );
        assert!(
            strokes_at(&out2, left).iter().any(|s| s.color == unfocused),
            "now-unfocused graph pane uses the unfocused token"
        );
        out.textures_delta.clear();
        out2.textures_delta.clear();
    }

    #[test]
    fn paint_panes_graph_publishes_canvas_and_seeds_camera() {
        // The graph pane publishes its own rect as the visible canvas and the
        // first frame seeds the camera from its size, so fit/center/zoom
        // anchoring and arrow-pan gating see the real pane.
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.open_graph();
        assert!(
            app.graph_camera.is_none(),
            "open_graph leaves the camera unseeded"
        );

        let mut out = run_paint_panes(&mut app);
        // The graph opens in the left big pane: 0.5 of 800 wide, full height.
        let (cw, ch) = app
            .graph_canvas_px
            .expect("canvas published by the pane paint");
        assert!((cw - 400.0).abs() < 0.01, "pane width on canvas: {cw}");
        assert!((ch - 600.0).abs() < 0.01, "pane height on canvas: {ch}");
        assert!(
            app.graph_camera.is_some(),
            "camera seeded from the pane size on the first frame"
        );
        out.textures_delta.clear();
    }

    #[test]
    fn graph_pane_rect_uses_the_class_layout_pane_not_the_tile_mirror() {
        // Bug droid_tui-up5: `graph_pane_rect` derived the drawn graph pane from
        // the legacy `tile_stack` slot mirror (left_x = width * main_split_ratio,
        // equal-height slots) instead of the class layout's `pane_geometry`, so
        // the scene origin, the first-frame fit, and the mouse origin disagreed
        // with the pane `paint_panes` actually draws the graph in. Here the
        // startup arrangement (Panels big-left + SourceViewer small-top) gets the
        // Graph view opened into the left big pane (0,0,400,600); the mirror
        // formula would instead return the top-right small pane (400,0,400,300).
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.open_graph(); // Graph is Big-class -> replaces the module UI in BigLeft.

        let win = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        // The real pane from the one geometry source the paint path uses.
        let real = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 600.0));
        let pane_from_geometry = app
            .pane_geometry(to_cell_rect(win))
            .into_iter()
            .find_map(|(id, cell)| {
                (app.layout.pane(id).view == Some(crate::app::ViewType::Graph)).then(|| {
                    egui::Rect::from_min_size(
                        egui::pos2(cell.x as f32, cell.y as f32),
                        egui::vec2(cell.width as f32, cell.height as f32),
                    )
                })
            })
            .expect("the graph is in a pane of the arrangement");
        assert_eq!(pane_from_geometry, real);

        assert_eq!(
            graph_pane_rect(&app, win),
            Some(real),
            "the scene pane must be the class-layout graph pane"
        );
        // Guard that the two formulas genuinely disagree (the old code returned
        // the top-right small pane), so the assertion above is not vacuous.
        let legacy = egui::Rect::from_min_size(egui::pos2(400.0, 0.0), egui::vec2(400.0, 300.0));
        assert_ne!(legacy, real, "mirror and class formulas disagree by design");
        assert_ne!(
            graph_pane_rect(&app, win),
            Some(legacy),
            "still returns the legacy tile-mirror geometry"
        );
    }

    #[test]
    fn center_graph_camera_centers_nodes_in_the_real_pane() {
        // Bug droid_tui-up5: `c` (`center_graph_camera`) computed a pane-local
        // pan against the real pane size, but `build_scene_spec` then added the
        // legacy mirror pane's origin, so the bounds center landed off by the
        // origin delta ("nodes move but are not centered"). In the two-pane
        // arrangement the y origin was wrong by half the band height.
        let mut app = App::new();
        let patch = crate::patch::Patch::from_ini_file(Path::new("fixtures/source_navigation.ini"))
            .unwrap();
        assert!(app.load_patch(patch));
        app.open_graph();

        let win = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let real = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 600.0));
        // Realistic flow: the paint publishes the real pane size, then `c`.
        app.graph_camera = None;
        app.fit_graph_camera((real.width(), real.height()));
        assert!(
            app.center_graph_camera(),
            "center succeeds with a camera + canvas"
        );

        let pane = graph_pane_rect(&app, win).expect("graph pane open");
        assert_eq!(pane, real);
        let scene = build_scene_spec(&app, crate::theme::active(), pane).expect("scene built");
        // Node spec x/y is the node's top-left at the mapped solver position;
        // the bounding box of those mapped positions must sit on the pane center.
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for n in &scene.nodes {
            min_x = min_x.min(n.x);
            max_x = max_x.max(n.x);
            min_y = min_y.min(n.y);
            max_y = max_y.max(n.y);
        }
        let center = egui::pos2((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        assert!(
            (center.x - real.center().x).abs() < 1.0,
            "graph centered horizontally in the pane: {center:?} vs {:?}",
            real.center()
        );
        assert!(
            (center.y - real.center().y).abs() < 1.0,
            "graph centered vertically in the pane: {center:?} vs {:?}",
            real.center()
        );
    }

    /// Run the full base + overlay paint (`paint_panes` then `paint_overlays`)
    /// in one headless frame over an 800x600 band.
    fn run_band(app: &mut App) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let win = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let raw = egui::RawInput {
            screen_rect: Some(win),
            ..Default::default()
        };
        let scene = scene_for(app, win);
        let mut out = ctx.run_ui(raw, |ui| {
            paint_panes(app, ui, scene.as_ref(), &[]);
            paint_overlays(app, ui);
        });
        // See `run_paint_panes`: release the delta before any assertion can run.
        out.textures_delta.clear();
        out
    }

    #[test]
    fn physical_paint_as_a_pane_view_headless() {
        // 2.3: the module UI is a view, not a permanent left pane. A band
        // showing only the module UI still has something to paint and must not
        // fall through to the empty-band graph canvas.
        let band = to_cell_rect(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));
        let mut app = App::new();
        let patch =
            crate::patch::Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        assert!(app.load_patch(patch));
        // Close the startup source viewer and clear the second small pane so
        // the module UI is the only view open.
        app.layout.small_top.view = None;
        app.layout.small_bottom.view = None;
        assert!(!app.layout.has_small_view());
        assert!(
            band_has_views(&app, band),
            "the module UI counts as an open view"
        );

        let mut out = run_paint_panes(&mut app);
        let labels = text_labels(&out);
        assert!(
            labels.iter().any(|l| l.contains("Module UI")),
            "module UI pane title missing: {labels:?}"
        );
        assert!(
            labels
                .iter()
                .any(|l| l.contains("P2B8") || l.contains("CV I/O")),
            "module UI faceplates rendered: {labels:?}"
        );
        assert_eq!(
            app.pane_rects.len(),
            1,
            "only the module UI pane is hit-testable"
        );
        assert!(app
            .pane_hit_rects
            .iter()
            .any(|(id, _)| *id == crate::panes::PaneId::BigLeft));

        // A band with nothing open has nothing to paint.
        let mut empty = App::new();
        empty.layout.big_left.view = None;
        empty.layout.big_right.view = None;
        empty.layout.small_top.view = None;
        empty.layout.small_bottom.view = None;
        assert!(!band_has_views(&empty, band), "no view open -> empty band");
        out.textures_delta.clear();
    }

    #[test]
    fn optimizer_paints_inside_its_pane_without_overlay_card() {
        // 2.2: the optimizer occupies its own small pane (spec "No overlay
        // card"); running the full base + overlay paint must fill that pane and
        // never emit the legacy overlay card over the band.
        let mut app = App::new();
        let patch =
            crate::patch::Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        assert!(app.load_patch(patch));
        app.open_view(crate::app::ViewType::Optimizer);
        assert_eq!(
            app.layout.small_bottom.view,
            Some(crate::app::ViewType::Optimizer),
            "optimizer opens in the free small pane"
        );

        let mut out = run_band(&mut app);
        let t = crate::theme::active();
        let muted = t.egui_color(t.muted);
        // The optimizer fills its own pane: an opaque muted rect at the
        // SmallBottom pane origin (400, 300).
        let fills_pane = out.shapes.iter().any(|cs| match &cs.shape {
            egui::epaint::Shape::Rect(r) => {
                r.fill == muted
                    && r.fill.a() == 255
                    && (r.rect.min.x - 400.0).abs() < 1.0
                    && (r.rect.min.y - 300.0).abs() < 1.0
            }
            _ => false,
        });
        assert!(fills_pane, "optimizer fills its small pane");
        // No card border over the band (the legacy optimizer overlay stroke).
        let border = t.egui_color(t.validation_modal_border);
        assert!(
            !out.shapes.iter().any(|cs| matches!(
                &cs.shape,
                egui::epaint::Shape::Rect(r)
                    if r.stroke.width > 0.0 && r.stroke.color == border
            )),
            "no optimizer overlay card border over the band"
        );
        let labels = text_labels(&out);
        assert!(labels.iter().any(|l| l.contains("Optimizer")), "{labels:?}");
        out.textures_delta.clear();
    }

    #[test]
    fn paint_dispatches_overlays_over_base() {
        // The overlay dispatch sits after the base paint and runs in both
        // branches; with the flags set every overlay's title lands as a text
        // shape in the same frame (mouse routing is out of scope).
        let mut app = App::new();
        app.showing_picker = true;
        app.showing_help = true;
        app.diff_showing = true;
        app.showing_validation = true;
        app.validation_issues = vec![crate::validation::ValidationIssue {
            span: crate::patch::Span {
                line: 0,
                col_start: 0,
                col_end: 4,
            },
            severity: crate::validation::Severity::Warning,
            code: "unknown_circuit".into(),
            message: "unknown circuit foo".into(),
        }];

        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut full_output = ctx.run_ui(raw, |ui| {
            paint_overlays(&app, ui);
        });
        let labels: Vec<String> = full_output
            .shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::epaint::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect();
        assert!(
            labels.iter().any(|l| l.contains("File Picker")),
            "picker title missing: {labels:?}"
        );
        // Help paints the active view's bare title; the picker's carries
        // padding, so this exact match only the help paint can satisfy.
        assert!(
            labels
                .iter()
                .any(|l| l.as_str() == crate::help::active_view(&app).title()),
            "help title missing: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("Diff")),
            "diff title missing: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("Validation")),
            "validation title missing: {labels:?}"
        );
        full_output.textures_delta.clear();
    }
}

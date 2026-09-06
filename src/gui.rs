//! GPU-accelerated graph window (egui + wgpu), compiled only with the
//! non-default `gui` feature (gpu-graph-window design D2).
//!
//! The window surface and the egui/wgpu canvas live in thread-locals so the
//! panic hook in `main.rs` can tear them down while the stack unwinds;
//! [`GraphWindow`] is the façade around both. The multiplexed winit loop that
//! drives the surface lives in `main.rs` (task 1.2); the egui painter backend
//! attaches here (task 2.2) and interaction mapping is task 3.x. Nothing here
//! is ever constructed under `cargo test`: the window is created only by
//! explicit lifecycle calls.

use std::cell::RefCell;
use std::fmt;
use std::future::Future;
use std::task::{Context as TaskContext, Poll};

use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::graph_render::{GraphCamera, SceneSpec};

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
    pub fn set_scene(&mut self, scene: Option<&SceneSpec>) {
        let replaced = match (&self.scene, scene) {
            (Some(prev), Some(next)) => prev.nodes.len() != next.nodes.len(),
            (None, Some(_)) | (Some(_), None) => true,
            (None, None) => false,
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
    pub fn fill_placeholder(&mut self) -> Option<WindowFrame> {
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
                    .map(|surface| surface.paint(window, scene, selected))
            })
        })
        .flatten()
        // Commit the frame's marquee state into the window-local selection so
        // the highlight survives the drag (task 3.2).
        .inspect(|frame| {
            self.selected_nodes = next_selection(frame, &self.selected_nodes);
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

impl EguiSurface {
    /// Paint one egui frame into the window and present it, reporting the
    /// frame's input state (task 3.1) for the loop to map onto `App`
    /// mutations. Pointer positions are egui points; the scene is painted 1:1
    /// (one spec pixel = one egui point), so they double as spec-pixel coords.
    /// `selected` is the window-local marquee selection driving the highlight.
    fn paint(
        &mut self,
        window: &Window,
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

        let full_output = self.context.run_ui(raw_input, |ui| {
            paint_scene(
                ui.painter(),
                ui.max_rect().size(),
                scene,
                &self.context,
                selected,
            );
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
                let factor = ((-i.smooth_scroll_delta.y) * ZOOM_SENSITIVITY).exp();
                let factor = factor.clamp(1.0 / MAX_ZOOM_STEP, MAX_ZOOM_STEP);
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
                marquee: frame_marquee(scene, i),
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

        // Keep the multiplexed loop hot for egui animations/repaint requests.
        if self.context.has_requested_repaint() {
            window.request_redraw();
        }
        window_frame
    }
}

/// Draw one scene frame into the window canvas (design D3/D5): opaque
/// background, cluster containers, then cables with direction arrows, then
/// node frames with ports and titles. The spec is pixel-space output of the
/// shared `GraphCamera`, painted 1:1 (one spec pixel = one egui point), so
/// both surfaces show the same view. Colors come only from the resolved spec
/// RGB — never theme tokens below the spec.
fn paint_scene(
    painter: &egui::Painter,
    canvas: egui::Vec2,
    scene: Option<&SceneSpec>,
    ctx: &egui::Context,
    selected: &[usize],
) {
    let Some(spec) = scene else {
        return; // No scene: the swapchain clear color shows through.
    };
    painter.rect_filled(
        egui::Rect::from_min_size(egui::Pos2::ZERO, canvas),
        0.0,
        rgb(spec.background),
    );

    for cluster in &spec.clusters {
        let rect = egui::Rect::from_min_size(
            egui::pos2(cluster.x, cluster.y),
            egui::vec2(cluster.w, cluster.h),
        );
        painter.rect(
            rect,
            egui::CornerRadius::same(2),
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(1.0, rgb(cluster.border)),
            egui::StrokeKind::Inside,
        );
        if !cluster.title.is_empty() {
            painter.text(
                rect.left_top() + egui::vec2(6.0, 3.0),
                egui::Align2::LEFT_TOP,
                &cluster.title,
                egui::FontId::proportional(12.0),
                rgb(cluster.title_color),
            );
        }
    }

    for edge in &spec.edges {
        // A quadratic Bézier is exactly a cubic with control points at
        // (2C+S)/3 and (2C+E)/3; egui paints cubic strokes.
        let c1 = egui::pos2(
            (2.0 * edge.ctrl.0 + edge.start.0) / 3.0,
            (2.0 * edge.ctrl.1 + edge.start.1) / 3.0,
        );
        let c2 = egui::pos2(
            (2.0 * edge.ctrl.0 + edge.end.0) / 3.0,
            (2.0 * edge.ctrl.1 + edge.end.1) / 3.0,
        );
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            [
                egui::pos2(edge.start.0, edge.start.1),
                c1,
                c2,
                egui::pos2(edge.end.0, edge.end.1),
            ],
            false,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(edge.width, rgb(edge.color)),
        ));
        paint_arrow(painter, edge);
    }

    for node in &spec.nodes {
        let rect =
            egui::Rect::from_min_size(egui::pos2(node.x, node.y), egui::vec2(node.w, node.h));
        let radius = node.radius.clamp(0.0, node.w.min(node.h) / 2.0);
        let corner = egui::CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: radius as u8,
            se: radius as u8,
        };
        painter.rect_filled(rect, corner, rgb(node.fill));
        if node.border_width > 0.0 {
            painter.rect(
                rect,
                corner,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(node.border_width, rgb(node.border)),
                egui::StrokeKind::Middle,
            );
        }
        // Port markers on the left (input) / right (output) edge midpoint,
        // like the terminal tile's ◉ / ●; drawn in the frame's border color.
        let port_r = (node.h * 0.16).clamp(3.0, 8.0);
        let mid_y = node.y + node.h / 2.0;
        if node.input_port {
            painter.circle_filled(egui::pos2(node.x, mid_y), port_r, rgb(node.border));
        }
        if node.output_port {
            painter.circle_filled(egui::pos2(node.x + node.w, mid_y), port_r, rgb(node.border));
        }
        if !node.label.is_empty() {
            let px = (node.h * 0.42).clamp(8.0, 40.0);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &node.label,
                egui::FontId::monospace(px),
                rgb(node.label_color),
            );
        }
    }

    // Task 3.2/3.3: selection, navigation and inspection overlays.
    paint_polish(painter, canvas, scene, ctx, selected);
}

/// Fill the direction arrow at an edge's `end`, mirroring the tiny-skia
/// painter: tangent `B'(1) = 2·(end − ctrl)`, triangle sized from the width.
fn paint_arrow(painter: &egui::Painter, edge: &crate::graph_render::EdgeSpec) {
    let (tx, ty) = (
        2.0 * (edge.end.0 - edge.ctrl.0),
        2.0 * (edge.end.1 - edge.ctrl.1),
    );
    let len = (tx * tx + ty * ty).sqrt();
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (tx / len, ty / len);
    let (nx, ny) = (-uy, ux);
    let arrow_len = (8.0 + edge.width * 2.0).min(16.0);
    let half = (4.0 + edge.width).min(8.0);
    let base = egui::pos2(edge.end.0 - ux * arrow_len, edge.end.1 - uy * arrow_len);
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(edge.end.0, edge.end.1),
            base + egui::vec2(nx * half, ny * half),
            base - egui::vec2(nx * half, ny * half),
        ],
        rgb(edge.color),
        egui::Stroke::NONE,
    ));
}

fn rgb((r, g, b): (u8, u8, u8)) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

fn rgba((r, g, b): (u8, u8, u8), a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Wheel-zoom sensitivity: exponent multiplier on the scroll delta so a
/// typical wheel tick reads as a gentle zoom step.
const ZOOM_SENSITIVITY: f32 = 0.01;
/// Largest factor a single scroll event may apply; bounds wheel zoom so a
/// fast spin cannot blow the camera off the scene.
const MAX_ZOOM_STEP: f32 = 1.5;
/// Fixed minimap size `(w, h)` in egui points, bottom-left corner.
const MINIMAP_SIZE: (f32, f32) = (180.0, 120.0);
/// Margin between the minimap panel and the canvas edge.
const MINIMAP_PAD: f32 = 10.0;
/// Pixel margin around a node frame when attributing an edge endpoint to it.
const EDGE_ATTRIB_MARGIN: f32 = 8.0;
/// Minimum drag extent before a marquee is reported; a click on empty canvas
/// (no movement) is not a marquee.
const MARQUEE_MIN_DRAG: f32 = 2.0;

/// Copy of `cam` panned by `(dx, dy)` spec pixels. Exposed so the windowed
/// loop can apply the window's middle-drag pan to the shared
/// `App::graph_camera` through the existing `GraphCamera::pan_by` API.
/// Pure and window-free.
pub fn camera_pan(cam: &GraphCamera, dx: f32, dy: f32) -> GraphCamera {
    let mut next = *cam;
    next.pan_by(dx, dy);
    next
}

/// Copy of `cam` zoomed by `factor` about the spec-pixel anchor `(ax, ay)`:
/// the world point under the anchor stays under the anchor after the zoom.
/// Mirrors the terminal `+`/`-` zoom, but anchored at the cursor instead of
/// the canvas centre. Pure and window-free.
pub fn camera_zoom_about(cam: &GraphCamera, factor: f32, anchor_px: (f32, f32)) -> GraphCamera {
    let mut next = *cam;
    let (wx, wy) = next.pixel_to_world(anchor_px.0, anchor_px.1);
    next.zoom_by(factor, (wx, wy));
    next
}

/// Index of the `scene` node whose pixel frame contains `(px, py)`, first
/// match wins (mirrors the terminal handler's hit-testing over the spec's own
/// pixel rects). `None` over empty canvas.
fn node_at(scene: &SceneSpec, px: f32, py: f32) -> Option<usize> {
    scene
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| px >= n.x && px < n.x + n.w && py >= n.y && py < n.y + n.h)
        .map(|(i, _)| i)
}

/// The axis-aligned rect spanning two pixel corners, normalized so the drag
/// direction is irrelevant.
fn normalize_rect(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    (
        a.0.min(b.0),
        a.1.min(b.1),
        (a.0 - b.0).abs(),
        (a.1 - b.1).abs(),
    )
}

/// Indices of `scene` nodes whose pixel frames intersect `rect`, strict AABB
/// overlap (a zero-area touch on a shared edge does not count).
fn nodes_in_rect(scene: &SceneSpec, (x, y, w, h): (f32, f32, f32, f32)) -> Vec<usize> {
    scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.x < x + w && n.x + n.w > x && n.y < y + h && n.y + n.h > y)
        .map(|(i, _)| i)
        .collect()
}

/// The active marquee over `scene` for the given pointer state: present only
/// while the primary button is held and the press began on empty canvas (a
/// press on a node is a node drag, not a marquee). `None` when idle.
fn frame_marquee(scene: Option<&SceneSpec>, i: &egui::InputState) -> Option<MarqueeSelection> {
    if !i.pointer.primary_down() {
        return None;
    }
    let origin = i.pointer.press_origin()?;
    let scene = scene?;
    if node_at(scene, origin.x, origin.y).is_some() {
        return None;
    }
    let cur = i.pointer.latest_pos()?;
    let rect = normalize_rect((origin.x, origin.y), (cur.x, cur.y));
    if rect.2 < MARQUEE_MIN_DRAG && rect.3 < MARQUEE_MIN_DRAG {
        return None; // a click, not a marquee drag
    }
    let nodes = nodes_in_rect(scene, rect);
    Some(MarqueeSelection { rect, nodes })
}

/// The window-local selection after this frame: a live or committed marquee
/// replaces the previous selection (so the highlight follows the drag), a
/// fresh press that is not a marquee (a node grab or a plain empty click)
/// clears it, and anything else keeps it. Pure so the marquee lifecycle tests
/// without a window.
fn next_selection(frame: &WindowFrame, previous: &[usize]) -> Vec<usize> {
    if let Some(marquee) = &frame.marquee {
        return marquee.nodes.clone();
    }
    if frame.primary_pressed {
        return Vec::new();
    }
    previous.to_vec()
}

/// Scene pixel bounds `(min_x, min_y, max_x, max_y)` over every node frame,
/// for the minimap's world-to-mini mapping. All zeros on an empty scene.
fn scene_bounds(scene: &SceneSpec) -> (f32, f32, f32, f32) {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for n in &scene.nodes {
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
        max_x = max_x.max(n.x + n.w);
        max_y = max_y.max(n.y + n.h);
    }
    if scene.nodes.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (min_x, min_y, max_x, max_y)
    }
}

/// Minimap layout: the bottom-left panel rect, the scaled viewport box (the
/// whole canvas in scene space), and each node's scaled frame. Pure geometry
/// so it tests without a window.
struct Minimap {
    panel: (f32, f32, f32, f32),
    viewport: (f32, f32, f32, f32),
    nodes: Vec<(f32, f32, f32, f32)>,
}

fn minimap_layout(scene: &SceneSpec, canvas: egui::Vec2) -> Option<Minimap> {
    if scene.nodes.is_empty() {
        return None;
    }
    let (mw, mh) = MINIMAP_SIZE;
    let (px, py) = (MINIMAP_PAD, canvas.y - mh - MINIMAP_PAD);
    let panel = (px, py, mw, mh);
    let (bx, by, bw, bh) = scene_bounds(scene);
    if bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    // A layout that fits the canvas needs no map; only show the minimap when
    // the world overflows the viewport in at least one axis.
    if bw <= canvas.x && bh <= canvas.y {
        return None;
    }
    let inner = (px + 4.0, py + 4.0, mw - 8.0, mh - 8.0);
    let (ix, iy, iw, ih) = inner;
    let sx = iw / bw;
    let sy = ih / bh;
    let map = |wx: f32, wy: f32| (ix + (wx - bx) * sx, iy + (wy - by) * sy);
    let nodes = scene
        .nodes
        .iter()
        .map(|n| {
            let (x0, y0) = map(n.x, n.y);
            let (x1, y1) = map(n.x + n.w, n.y + n.h);
            (x0, y0, x1 - x0, y1 - y0)
        })
        .collect();
    // The visible canvas occupies `[0,0] x canvas` in scene-pixel space.
    let (v0, v0y) = map(0.0, 0.0);
    let (v1, v1y) = map(canvas.x, canvas.y);
    // Clamp the viewport box to the panel: when the canvas is larger than the
    // scene (zoomed out so the whole scene fits), the full-canvas box would
    // overrun the panel; clamping makes it read as "the whole scene is
    // visible". When zoomed in, the box is already a sub-rect and is
    // unchanged.
    let (px2, py2, pw2, ph2) = panel;
    let cx = v0.clamp(px2, px2 + pw2);
    let cy = v0y.clamp(py2, py2 + ph2);
    let cx2 = (v1).clamp(px2, px2 + pw2);
    let cy2 = (v1y).clamp(py2, py2 + ph2);
    let viewport = (cx, cy, cx2 - cx, cy2 - cy);
    Some(Minimap {
        panel,
        viewport,
        nodes,
    })
}

/// The node whose frame is nearest to a scene point, within a margin: used to
/// attribute an edge's start/end port to its source/sink node for the latency
/// readout. The nearest-centre tiebreak keeps two abutting nodes from both
/// claiming a shared-edge port.
fn nearest_node_at(scene: &SceneSpec, px: f32, py: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, n) in scene.nodes.iter().enumerate() {
        if px >= n.x - EDGE_ATTRIB_MARGIN
            && px <= n.x + n.w + EDGE_ATTRIB_MARGIN
            && py >= n.y - EDGE_ATTRIB_MARGIN
            && py <= n.y + n.h + EDGE_ATTRIB_MARGIN
        {
            let dx = n.x + n.w / 2.0 - px;
            let dy = n.y + n.h / 2.0 - py;
            let d = dx * dx + dy * dy;
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Tooltip text for a hovered node: the circuit label (with the occurrence
/// index when the name repeats) plus a latency readout aggregated from the
/// node's outgoing edges' resolved [`EdgeLatency`] states. No latency data
/// yields just the label. The window cannot consult `latency::CostModel` (the
/// scene is the only shared source it sees), so it reports the same ramp
/// classification the terminal colours edges with.
fn node_tooltip(scene: &SceneSpec, node_index: usize) -> String {
    let node = &scene.nodes[node_index];
    let repeated = scene
        .nodes
        .iter()
        .filter(|n| n.circuit == node.circuit && !node.circuit.is_empty())
        .count()
        > 1;
    let mut label = if node.circuit.is_empty() {
        format!("node #{node_index}")
    } else if repeated {
        format!("{} ({})", node.circuit, node.instance_index)
    } else {
        node.circuit.clone()
    };
    let outgoing = scene
        .edges
        .iter()
        .filter(|e| nearest_node_at(scene, e.start.0, e.start.1) == Some(node_index))
        .collect::<Vec<_>>();
    let latencies = outgoing
        .iter()
        .filter_map(|e| e.latency)
        .collect::<Vec<_>>();
    if latencies.is_empty() {
        return label;
    }
    let hot = latencies.iter().map(|l| l.ramp_stop).max().unwrap_or(0);
    let back = latencies.iter().filter(|l| l.back_edge).count();
    label.push_str(&format!(
        " · latency {hot}/4 · {back} back edge{}",
        if back == 1 { "" } else { "s" }
    ));
    label
}

/// Draws the canvas polish overlays on top of `paint_scene`: the minimap, the
/// active marquee selection, the committed selection's node highlight, and the
/// hovered node's latency tooltip. Every color derives from the resolved scene
/// spec RGB (never hardcoded). The tooltip is drawn last so it stays readable
/// above marquee, selection, and minimap.
fn paint_polish(
    painter: &egui::Painter,
    canvas: egui::Vec2,
    scene: Option<&SceneSpec>,
    ctx: &egui::Context,
    selected: &[usize],
) {
    let Some(spec) = scene else {
        return;
    };
    let accent = spec
        .nodes
        .first()
        .map(|n| n.border)
        .or_else(|| spec.clusters.first().map(|c| c.border))
        .unwrap_or(spec.background);
    if let Some(minimap) = minimap_layout(spec, canvas) {
        paint_minimap(painter, spec, &minimap, accent);
    }
    // The committed marquee selection: an accent overlay border on each picked
    // node frame, drawn above the scene but below the in-progress marquee rect
    // and the tooltip. Out-of-range indices (stale after a rebuild) are skipped.
    for &i in selected {
        let Some(node) = spec.nodes.get(i) else {
            continue;
        };
        let rect =
            egui::Rect::from_min_size(egui::pos2(node.x, node.y), egui::vec2(node.w, node.h));
        painter.rect(
            rect,
            egui::CornerRadius::same(node.radius.clamp(0.0, node.w.min(node.h) / 2.0) as u8),
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(2.0, rgb(accent)),
            egui::StrokeKind::Middle,
        );
    }
    ctx.input(|i| {
        if let Some(marquee) = frame_marquee(Some(spec), i) {
            let (x, y, w, h) = marquee.rect;
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h));
            painter.rect_filled(rect, 0.0, rgba(accent, 30));
            painter.rect(
                rect,
                0.0,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(1.0, rgb(accent)),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(pos) = i.pointer.latest_pos() {
            if let Some(idx) = node_at(spec, pos.x, pos.y) {
                paint_tooltip(painter, spec, canvas, pos, idx);
            }
        }
    });
}

/// The hovered node's tooltip card: a translucent backdrop with the accent
/// border, placed above-right of the cursor and clamped to the canvas.
fn paint_tooltip(
    painter: &egui::Painter,
    spec: &SceneSpec,
    canvas: egui::Vec2,
    pos: egui::Pos2,
    node_index: usize,
) {
    let node = &spec.nodes[node_index];
    let font = egui::FontId::proportional(13.0);
    let galley =
        painter.layout_no_wrap(node_tooltip(spec, node_index), font, rgb(node.label_color));
    let pad = 6.0;
    let size = galley.size() + egui::vec2(pad * 2.0, pad * 2.0);
    let min_x = (pos.x + 12.0).clamp(0.0, (canvas.x - size.x).max(0.0));
    let min_y = (pos.y - size.y - 8.0).clamp(0.0, (canvas.y - size.y).max(0.0));
    let min = egui::pos2(min_x, min_y);
    let rect = egui::Rect::from_min_size(min, size);
    painter.rect(
        rect,
        egui::CornerRadius::same(4),
        rgba(spec.background, 235),
        egui::Stroke::new(1.0, rgb(node.border)),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        rect.min + egui::vec2(pad, pad),
        galley,
        rgb(node.label_color),
    );
}

/// The minimap panel: translucent scene-background backdrop, accent-bordered,
/// node frames as accent rects, and a viewport indicator showing what the
/// canvas currently displays.
fn paint_minimap(
    painter: &egui::Painter,
    spec: &SceneSpec,
    minimap: &Minimap,
    accent: (u8, u8, u8),
) {
    let (px, py, pw, ph) = minimap.panel;
    let panel = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(pw, ph));
    painter.rect_filled(
        panel,
        egui::CornerRadius::same(4),
        rgba(spec.background, 215),
    );
    painter.rect(
        panel,
        egui::CornerRadius::same(4),
        egui::Color32::TRANSPARENT,
        egui::Stroke::new(1.0, rgb(accent)),
        egui::StrokeKind::Inside,
    );
    for &(x, y, w, h) in &minimap.nodes {
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w.max(1.0), h.max(1.0))),
            egui::CornerRadius::same(1),
            rgb(accent),
        );
    }
    let (vx, vy, vw, vh) = minimap.viewport;
    let vrect = egui::Rect::from_min_size(egui::pos2(vx, vy), egui::vec2(vw, vh));
    painter.rect(
        vrect,
        0.0,
        rgba(accent, 40),
        egui::Stroke::new(1.0, rgb(accent)),
        egui::StrokeKind::Inside,
    );
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
#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;
    use crate::graph_render::{CableKind, EdgeLatency, EdgeSpec, NodeSpec};

    fn scene() -> SceneSpec {
        SceneSpec {
            background: (10, 10, 10),
            nodes: vec![
                NodeSpec {
                    x: 0.0,
                    y: 0.0,
                    w: 200.0,
                    h: 80.0,
                    radius: 8.0,
                    fill: (20, 20, 20),
                    border: (200, 200, 200),
                    border_width: 1.0,
                    label: "copy".into(),
                    label_color: (255, 255, 255),
                    circuit: "copy".into(),
                    instance_index: 0,
                    input_port: false,
                    output_port: true,
                },
                NodeSpec {
                    x: 260.0,
                    y: 0.0,
                    w: 200.0,
                    h: 80.0,
                    radius: 8.0,
                    fill: (20, 20, 20),
                    border: (200, 200, 200),
                    border_width: 1.0,
                    label: "seq".into(),
                    label_color: (255, 255, 255),
                    circuit: "seq".into(),
                    instance_index: 0,
                    input_port: true,
                    output_port: true,
                },
                NodeSpec {
                    x: 500.0,
                    y: 300.0,
                    w: 200.0,
                    h: 80.0,
                    radius: 8.0,
                    fill: (20, 20, 20),
                    border: (200, 200, 200),
                    border_width: 1.0,
                    label: "seq".into(),
                    label_color: (255, 255, 255),
                    circuit: "seq".into(),
                    instance_index: 1,
                    input_port: true,
                    output_port: false,
                },
            ],
            edges: vec![EdgeSpec {
                start: (200.0, 40.0),
                end: (260.0, 40.0),
                ctrl: (230.0, 40.0),
                color: (0, 255, 0),
                width: 2.0,
                kind: CableKind::Audio,
                error: false,
                dim: false,
                diff: None,
                latency: Some(EdgeLatency {
                    ramp_stop: 2,
                    back_edge: false,
                }),
            }],
            clusters: vec![],
        }
    }

    #[test]
    fn camera_pan_shifts_pan() {
        let cam = GraphCamera::default();
        let next = camera_pan(&cam, 12.0, -5.0);
        assert_eq!(next.pan, (cam.pan.0 + 12.0, cam.pan.1 - 5.0));
    }

    #[test]
    fn camera_zoom_about_keeps_anchor_pixel_stable() {
        let cam = GraphCamera::default();
        let anchor = (333.0, 111.0);
        let (wx, wy) = cam.pixel_to_world(anchor.0, anchor.1);
        let next = camera_zoom_about(&cam, 1.5, anchor);
        let (px, py) = next.world_to_pixel(wx, wy);
        assert!((px - anchor.0).abs() < 1e-3 && (py - anchor.1).abs() < 1e-3);
    }

    #[test]
    fn node_at_hits_frame_and_misses_empty() {
        let s = scene();
        assert_eq!(node_at(&s, 10.0, 10.0), Some(0));
        assert_eq!(node_at(&s, 199.0, 79.0), Some(0));
        assert_eq!(node_at(&s, 200.0, 40.0), None); // shared edge is excluded
        assert_eq!(node_at(&s, 999.0, 999.0), None);
    }

    #[test]
    fn nodes_in_rect_selects_intersecting_frames() {
        let s = scene();
        // Span the top row only: nodes 0 and 1.
        assert_eq!(nodes_in_rect(&s, (50.0, -10.0, 600.0, 100.0)), vec![0, 1]);
        // A rect over node 2 alone.
        assert_eq!(nodes_in_rect(&s, (510.0, 310.0, 10.0, 10.0)), vec![2]);
        // No overlap at all.
        assert!(nodes_in_rect(&s, (900.0, 900.0, 10.0, 10.0)).is_empty());
    }

    #[test]
    fn normalize_rect_orders_any_drag_direction() {
        let a = (300.0, 50.0);
        let b = (100.0, 200.0);
        let (x, y, w, h) = normalize_rect(a, b);
        assert_eq!((x, y, w, h), (100.0, 50.0, 200.0, 150.0));
        assert_eq!(normalize_rect(b, a), normalize_rect(a, b));
    }

    #[test]
    fn tooltip_shows_circuit_and_latency_readout() {
        let s = scene();
        // Node 0 has an outgoing edge with ramp_stop 2, no back edge.
        let tip = node_tooltip(&s, 0);
        assert!(tip.contains("copy"));
        assert!(tip.contains("latency 2/4"));
        assert!(tip.contains("0 back edges"));
        // Node 2 has no outgoing edges: label only.
        assert_eq!(node_tooltip(&s, 2), "seq (1)");
    }

    #[test]
    fn tooltip_disambiguates_repeated_circuit_names() {
        let s = scene();
        let tip = node_tooltip(&s, 1);
        assert!(tip.contains("seq (0)"));
    }

    #[test]
    fn minimap_maps_nodes_and_viewport_into_panel() {
        let s = scene();
        // Scene bounds (700×380) overflow the 400×300 canvas, so the map shows.
        let m = minimap_layout(&s, egui::vec2(400.0, 300.0)).unwrap();
        let (px, py, pw, ph) = m.panel;
        assert_eq!((px, py, pw, ph), (10.0, 170.0, 180.0, 120.0));
        // Every node frame lands inside the panel's inner area.
        for &(x, y, w, h) in &m.nodes {
            assert!(x >= px && x + w <= px + pw && y >= py && y + h <= py + ph);
        }
        // The viewport box is the whole canvas mapped into mini space; the
        // scene starts at the origin, so the viewport's top-left is the
        // mapping of (0,0) and must sit inside the panel.
        let (vx, vy, vw, vh) = m.viewport;
        assert!(vx >= px && vy >= py && vx + vw <= px + pw && vy + vh <= py + ph);
        assert!(vw > 0.0 && vh > 0.0);
    }

    #[test]
    fn minimap_hidden_when_layout_fits_canvas() {
        let s = scene();
        // Scene bounds (700×380) fit the 1000×800 canvas: no map needed.
        assert!(minimap_layout(&s, egui::vec2(1000.0, 800.0)).is_none());
    }

    #[test]
    fn next_selection_commits_marquee_and_clears_on_plain_press() {
        let s = scene();
        let mut prev: Vec<usize> = Vec::new();
        // A live marquee over the top row selects nodes 0 and 1.
        let marquee = MarqueeSelection {
            rect: (50.0, -10.0, 600.0, 100.0),
            nodes: nodes_in_rect(&s, (50.0, -10.0, 600.0, 100.0)),
        };
        let dragging = WindowFrame {
            marquee: Some(marquee.clone()),
            primary_pressed: false,
            ..WindowFrame::default()
        };
        prev = next_selection(&dragging, &prev);
        assert_eq!(prev, vec![0, 1]);
        // The release frame has no marquee (primary no longer down) and no
        // fresh press: the committed selection survives.
        let released = WindowFrame::default();
        assert_eq!(next_selection(&released, &prev), vec![0, 1]);
        // A fresh press that is not a marquee (a node grab or empty click)
        // clears the window-local selection.
        let pressed = WindowFrame {
            primary_pressed: true,
            ..WindowFrame::default()
        };
        assert!(next_selection(&pressed, &prev).is_empty());
    }

    #[test]
    fn minimap_empty_scene_is_none() {
        let empty = SceneSpec {
            background: (0, 0, 0),
            nodes: vec![],
            edges: vec![],
            clusters: vec![],
        };
        assert!(minimap_layout(&empty, egui::vec2(800.0, 600.0)).is_none());
    }
}

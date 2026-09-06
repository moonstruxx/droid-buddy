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
    /// theme each frame, so both surfaces show the same graph.
    pub fn set_scene(&mut self, scene: Option<&SceneSpec>) {
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
            EGUI.with(|slot| {
                slot.borrow_mut()
                    .as_mut()
                    .map(|surface| surface.paint(window, scene))
            })
        })
        .flatten()
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

impl EguiSurface {
    /// Paint one egui frame into the window and present it, reporting the
    /// frame's input state (task 3.1) for the loop to map onto `App`
    /// mutations. Pointer positions are egui points; the scene is painted 1:1
    /// (one spec pixel = one egui point), so they double as spec-pixel coords.
    fn paint(&mut self, window: &Window, scene: Option<&SceneSpec>) -> WindowFrame {
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
            paint_scene(ui.painter(), ui.max_rect().size(), scene);
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
            WindowFrame {
                pointer,
                primary_pressed: i.pointer.primary_pressed(),
                primary_down: i.pointer.primary_down(),
                primary_released: i.pointer.primary_released(),
                keys,
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
        let clear = scene.map_or(wgpu::Color::BLACK, |spec| wgpu::Color {
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
fn paint_scene(painter: &egui::Painter, canvas: egui::Vec2, scene: Option<&SceneSpec>) {
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

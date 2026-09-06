//! GPU-accelerated graph window (egui + wgpu), compiled only with the
//! non-default `gui` feature (gpu-graph-window design D2).
//!
//! The window surface lives in a thread-local so the panic hook in
//! `main.rs` can destroy it while the stack unwinds; [`GraphWindow`] is the
//! façade around that surface. The multiplexed winit loop that drives the
//! surface lives in `main.rs` (task 1.2), the egui painter attaches in task
//! 2.2 (see [`GraphWindow::fill_placeholder`]), and interaction mapping is
//! task 3.x. Nothing here is ever constructed under `cargo test`: the window
//! is created only by explicit lifecycle calls.

use std::cell::RefCell;
use std::fmt;

use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

/// Lifecycle state of the graph window.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum WindowState {
    /// No window surface exists.
    #[default]
    Closed,
    /// A window surface exists and is being driven.
    Open,
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

// The single graph window surface. Parked in a thread-local so the panic
// hook can drop it even while the app stack is unwinding; the main thread
// is the only thread that ever touches it.
thread_local! {
    static WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
}

/// A GPU graph window bound to the same `App` state as the terminal tile.
///
/// winit permits only one `EventLoop` per process, so the loop is created
/// once in `main.rs`; [`GraphWindow::open`] creates the window surface on
/// that loop and returns, leaving the caller to keep driving the multiplexed
/// loop. Task 2.2 attaches the `egui::Context` + `egui_winit::State` +
/// `egui_wgpu::Renderer` canvas inside [`GraphWindow::fill_placeholder`];
/// task 3.x maps window interactions onto existing `App` mutations.
#[derive(Debug, Default)]
pub struct GraphWindow {
    state: WindowState,
}

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
        WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
        self.state = WindowState::Open;
        // Pull the first frame out of winit so the surface is live.
        self.request_redraw();
        Ok(())
    }

    /// Tears down the surface and returns to [`WindowState::Closed`].
    pub fn close(&mut self) {
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

    /// Borrows the live window for a short-lived operation.
    ///
    /// Returns `None` while the window is closed. The painter (task 2.2)
    /// reads the window's size and scale factor and builds its surface here.
    pub fn with_window<R>(&self, f: impl FnOnce(&Window) -> R) -> Option<R> {
        WINDOW.with(|slot| slot.borrow().as_ref().map(f))
    }

    /// Asks winit to schedule the next frame.
    pub fn request_redraw(&self) {
        self.with_window(|window| window.request_redraw());
    }

    /// Paints one frame into the window.
    ///
    /// Placeholder until task 2.2 attaches the egui/wgpu painter; the window
    /// shows the platform default background in the meantime.
    pub fn fill_placeholder(&mut self) {
        // The egui::Context + egui_winit::State + egui_wgpu::Renderer canvas
        // lands here in task 2.2; nothing to draw yet.
    }
}

/// Destroys the graph window surface; called by the panic hook in `main.rs`
/// so a panic cannot leave an orphaned window on the desktop.
///
/// Uses `try_borrow_mut` so a panic that happens while the window is borrowed
/// cannot recurse; unwinding drops the surface anyway.
pub fn destroy_window_for_panic() {
    WINDOW.with(|slot| {
        if let Ok(mut handle) = slot.try_borrow_mut() {
            *handle = None;
        }
    });
}

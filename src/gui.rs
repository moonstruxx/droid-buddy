//! GPU-accelerated graph window (egui + wgpu), compiled only with the
//! non-default `gui` feature (gpu-graph-window design D2).
//!
//! Not wired into `main.rs` yet: this module only declares the window surface
//! and its lifecycle. The multiplexed winit loop that drives it is task 1.2,
//! the egui painter is task 2.2, and interaction mapping is task 3.x. Nothing
//! here is ever constructed: `cargo test` runs without the feature, and the
//! window is created only by explicit lifecycle calls.

use std::fmt;

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

/// A GPU graph window bound to the same `App` state as the terminal tile.
///
/// Later tasks give it the real surface: the winit event loop and window
/// (1.2), the `egui::Context` + `egui_winit::State` + `egui_wgpu::Renderer`
/// canvas (2.2), and the interaction mapping onto existing `App` mutations
/// (3.x). `open`/`close` are stubs until 1.2; they never block and never
/// take over the process main loop.
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
    /// Stub until task 1.2 lands: it will create the winit event loop and
    /// window plus the egui/wgpu context, then return so the caller keeps
    /// driving the multiplexed loop. Rejects a window that is already open.
    pub fn open(&mut self) -> Result<(), WindowError> {
        if self.is_open() {
            return Err(WindowError::new("graph window is already open"));
        }
        self.state = WindowState::Open;
        Ok(())
    }

    /// Tears down the surface and returns to [`WindowState::Closed`].
    pub fn close(&mut self) {
        self.state = WindowState::Closed;
    }

    /// Whether a window surface currently exists.
    pub fn is_open(&self) -> bool {
        self.state == WindowState::Open
    }
}

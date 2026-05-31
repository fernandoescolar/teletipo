use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::ModifiersState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    MacOS,
    Windows,
    Linux,
    Unknown,
}

/// Cross-platform window events forwarded from the GPU back-end to the
/// application layer.  Defined here (in `platform-abstraction`) so that the
/// UI logic crate can consume events without taking a direct dependency on the
/// GPU renderer.
#[derive(Debug, Clone)]
pub enum AppWindowEvent {
    CloseRequested,
    /// New top-left position of the window in physical pixels.
    WindowMoved {
        x: i32,
        y: i32,
    },
    /// Physical pixel dimensions of the window plus the actual cell size
    /// (physical px) as measured from the loaded font.
    Resized {
        width: u32,
        height: u32,
        scale_factor: f64,
        cell_w: f32,
        cell_h: f32,
    },
    CursorMoved {
        x: f64,
        y: f64,
    },
    MouseInput {
        state: ElementState,
        button: MouseButton,
    },
    MouseWheel {
        delta_lines: f32,
    },
    ModifiersChanged(ModifiersState),
    KeyboardInput(KeyEvent),
    ImeCommit(String),
    /// A file was dropped onto the window.
    DroppedFile(std::path::PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayBackend {
    Wayland,
    X11,
    Unknown,
}

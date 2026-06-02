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

// ── Accessibility tree ────────────────────────────────────────────────────────

/// A single node in the semantic accessibility tree exposed to screen readers.
#[derive(Debug, Clone)]
pub enum AccessNode {
    /// The top-level terminal viewport with its raw text content.
    Terminal {
        /// Total visible rows.
        rows: usize,
        /// Total visible columns.
        cols: usize,
        /// Plain-text dump of the visible terminal grid (newline-separated).
        text: String,
    },
    /// A completed shell command with its associated output text.
    CommandZone {
        /// Absolute row where the prompt was first displayed.
        prompt_row: usize,
        /// The shell command as typed by the user.
        command_text: String,
        /// Exit code (None if the shell hasn't reported it yet).
        exit_code: Option<i32>,
        /// The command's output text.
        output_text: String,
    },
    /// A clickable OSC 8 hyperlink in the terminal output.
    Hyperlink {
        /// Viewport row of the link.
        row: usize,
        /// Starting column of the link text (inclusive).
        col_start: usize,
        /// Ending column of the link text (exclusive).
        col_end: usize,
        /// The link's display text (characters in the cell range).
        label: String,
        /// The target URI.
        uri: String,
    },
    /// A tab in the tab bar.
    Tab {
        /// Zero-based index.
        index: usize,
        /// The label text shown in the tab bar.
        label: String,
        /// Whether this is the currently active tab.
        active: bool,
    },
}

/// The full accessibility tree for the current terminal window state.
///
/// Passed to [`crate::traits::Accessibility::update_tree`] after each
/// render frame so screen readers have access to the semantic structure.
#[derive(Debug, Clone, Default)]
pub struct AccessibilityTree {
    /// All nodes making up the tree, in reading order.
    pub nodes: Vec<AccessNode>,
}

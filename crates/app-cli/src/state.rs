use crate::launch::FontEntry;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Currently held keyboard modifier keys, refreshed on every
/// [`winit::event::WindowEvent::ModifiersChanged`].
#[derive(Default)]
pub(crate) struct ModifierState {
    pub(crate) ctrl_down: bool,
    /// Whether the Super/Command key (⌘ on macOS) is currently held.
    pub(crate) super_down: bool,
    pub(crate) shift_down: bool,
    /// Whether the Alt/Option key is currently held.
    pub(crate) alt_down: bool,
}

/// State for the various pointer-driven drag interactions (separator,
/// scrollbars, tab reorder). All fields default to a neutral "no drag in
/// progress" state.
#[derive(Default)]
pub(crate) struct DragState {
    /// Whether the user is currently dragging the separator bar.
    pub(crate) dragging_separator: bool,
    /// Whether the user is currently dragging the terminal scrollbar thumb.
    pub(crate) dragging_terminal_scrollbar: bool,
    /// Whether the user is currently dragging the editor scrollbar thumb.
    pub(crate) dragging_editor_scrollbar: bool,
    /// Index of the tab being dragged, if any.
    pub(crate) tab_drag: Option<usize>,
    /// Cursor x position at the moment the tab drag began.
    pub(crate) tab_drag_start_x: f64,
}

/// Last known cursor position and held mouse button. Updated on
/// [`winit::event::WindowEvent::CursorMoved`] and `MouseInput`.
#[derive(Default)]
pub(crate) struct CursorState {
    pub(crate) cursor_x: f64,
    pub(crate) cursor_y: f64,
    /// Which mouse button (0=left, 1=mid, 2=right) is currently held, for
    /// motion-reporting passthrough to the PTY (modes 1002/1003).
    pub(crate) mouse_btn_held: Option<u8>,
}

/// Window geometry plus the renderer's reported per-cell physical pixel size.
/// Updated on `Resized`, `WindowMoved`, and `ScaleFactorChanged` events.
pub(crate) struct LayoutState {
    pub(crate) window_width: u32,
    pub(crate) window_height: u32,
    /// Last known window top-left position in physical pixels.
    pub(crate) window_x: i32,
    pub(crate) window_y: i32,
    /// Current display scale factor (1.0 on standard, 2.0 on Retina, etc.).
    pub(crate) scale_factor: f64,
    /// Actual physical-pixel cell dimensions from the renderer font.
    pub(crate) cell_w: f32,
    pub(crate) cell_h: f32,
}

/// Transient UI overlay state: cursor blink, BEL flash, last-resize hint,
/// update-available banner, and right-click tab context menu.
pub(crate) struct OverlayState {
    /// Time and dimensions of the last PTY resize, shown as an overlay for 1 s.
    pub(crate) last_resize: Option<(Instant, u16, u16)>,
    /// Transient PTY/session status message shown in the resize overlay slot.
    pub(crate) pty_status: Option<(Instant, String)>,
    /// Context menu opened by right-clicking a tab. (tab_idx, menu_x_px, menu_y_px)
    pub(crate) tab_context_menu: Option<(usize, f64, f64)>,
    /// Currently highlighted item inside the open context menu (0-3).
    pub(crate) tab_context_hover: Option<usize>,
    /// Status banner for the last background update check.
    pub(crate) pending_update: Option<UpdateBanner>,
    /// When `Some`, flash the terminal background as a visual BEL indicator
    /// until the contained `Instant`.
    pub(crate) bell_flash_until: Option<Instant>,
    /// Time the cursor blink half-cycle last toggled.
    pub(crate) cursor_blink_last: Instant,
    /// `true` = cursor visible (on-phase); `false` = cursor hidden (off-phase).
    pub(crate) cursor_blink_phase: bool,
    /// Queue of transient toast notifications shown at the bottom-right.
    pub(crate) toasts: VecDeque<Toast>,
    /// The last search query entered by the user so it can be restored on re-open.
    pub(crate) last_search_query: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

pub(crate) struct Toast {
    pub(crate) text: String,
    pub(crate) kind: ToastKind,
    pub(crate) expires_at: Instant,
}

impl Toast {
    pub(crate) fn new(text: impl Into<String>, kind: ToastKind, ttl: Duration) -> Self {
        Self { text: text.into(), kind, expires_at: Instant::now() + ttl }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum UpdateBanner {
    Available(String),
    Failed(String),
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            last_resize: None,
            pty_status: None,
            tab_context_menu: None,
            tab_context_hover: None,
            pending_update: None,
            bell_flash_until: None,
            cursor_blink_last: Instant::now(),
            cursor_blink_phase: true,
            toasts: VecDeque::new(),
            last_search_query: None,
        }
    }
}

/// Theme and font catalogues discovered at startup plus the index of the
/// currently active preset (if any).
#[derive(Default)]
pub(crate) struct ThemeFontState {
    /// All theme files discovered at startup (sorted by name).
    pub(crate) available_themes: Vec<crate::theme::ThemeFile>,
    /// Index into `available_themes` of the currently active preset, or `None`
    /// when the user is using custom colors.
    pub(crate) active_theme_idx: Option<usize>,
    /// All font families discovered at startup (index 0 = "(default)").
    pub(crate) available_fonts: Vec<FontEntry>,
    /// Index into `available_fonts` of the currently selected font.
    /// 0 means "(default)", i.e. no font family override.
    pub(crate) active_font_idx: usize,
}

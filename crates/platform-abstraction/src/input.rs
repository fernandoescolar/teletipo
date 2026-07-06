//! Backend-neutral input event types.
//!
//! These types replace winit-specific input events, allowing app-cli and other layers
//! to remain independent of the renderer backend (winit/glutin for glow, gpui, etc.).

/// Button state: pressed or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputState {
    /// Button was pressed down.
    Pressed,
    /// Button was released.
    Released,
}

/// Mouse/pointer button identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// Left mouse button.
    Left,
    /// Middle mouse button.
    Middle,
    /// Right mouse button.
    Right,
    /// Other button (e.g., back, forward, extra).
    Other(u16),
}

/// Keyboard modifier key state (Ctrl, Shift, Alt, Super/Cmd).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierKeys {
    /// Ctrl or Cmd on macOS.
    pub ctrl: bool,
    /// Shift key.
    pub shift: bool,
    /// Alt or Option on macOS.
    pub alt: bool,
    /// Super/Windows key, or Cmd on macOS (in addition to ctrl).
    pub super_key: bool,
}

/// Printable/character logical key (may contain multiple Unicode codepoints).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalKey {
    /// A character key (may be multi-char for composed keys).
    Character(String),
    /// A named (non-character) key such as Escape, Enter, Tab.
    Named(NamedKey),
}

/// Named keys that are not printable characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Escape,
    Enter,
    Tab,
    Backspace,
    Space,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    CapsLock,
    Shift,
    Control,
    Alt,
    Super,
    AltGraph,
    Meta,
    Hyper,
    Paste,
    /// Unknown or unsupported named key.
    Other,
}

/// Physical key code: location-independent key identifier (e.g., KeyA regardless of layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalKey {
    /// Keyboard key by code.
    Code(KeyCode),
    /// Unidentified key.
    Unidentified,
}

/// Physical keyboard key codes (US layout basis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Row 1
    Escape,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Digit0,
    Minus,
    Equal,
    Backspace,

    // Row 2
    Tab,
    KeyQ,
    KeyW,
    KeyE,
    KeyR,
    KeyT,
    KeyY,
    KeyU,
    KeyI,
    KeyO,
    KeyP,
    BracketLeft,
    BracketRight,
    Enter,

    // Row 3
    ControlLeft,
    KeyA,
    KeyS,
    KeyD,
    KeyF,
    KeyG,
    KeyH,
    KeyJ,
    KeyK,
    KeyL,
    Semicolon,
    Quote,
    Backquote,

    // Row 4
    ShiftLeft,
    Backslash,
    KeyZ,
    KeyX,
    KeyC,
    KeyV,
    KeyB,
    KeyN,
    KeyM,
    Comma,
    Period,
    Slash,
    ShiftRight,

    // Row 5
    AltLeft,
    Space,
    CapsLock,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Editing / Navigation
    Home,
    ArrowUp,
    PageUp,
    ArrowLeft,
    ArrowRight,
    End,
    ArrowDown,
    PageDown,
    Insert,
    Delete,
    /// Any other key not listed above.
    Other(u32),
}

impl KeyCode {
    /// Check if this key is in the main alphanumeric/symbol row.
    pub fn is_main_row(&self) -> bool {
        matches!(
            self,
            KeyCode::Digit1
                | KeyCode::Digit2
                | KeyCode::Digit3
                | KeyCode::Digit4
                | KeyCode::Digit5
                | KeyCode::Digit6
                | KeyCode::Digit7
                | KeyCode::Digit8
                | KeyCode::Digit9
                | KeyCode::Digit0
                | KeyCode::Minus
                | KeyCode::Equal
        )
    }

    /// Check if this key is a letter key (A-Z).
    pub fn is_letter(&self) -> bool {
        matches!(
            self,
            KeyCode::KeyA
                | KeyCode::KeyB
                | KeyCode::KeyC
                | KeyCode::KeyD
                | KeyCode::KeyE
                | KeyCode::KeyF
                | KeyCode::KeyG
                | KeyCode::KeyH
                | KeyCode::KeyI
                | KeyCode::KeyJ
                | KeyCode::KeyK
                | KeyCode::KeyL
                | KeyCode::KeyM
                | KeyCode::KeyN
                | KeyCode::KeyO
                | KeyCode::KeyP
                | KeyCode::KeyQ
                | KeyCode::KeyR
                | KeyCode::KeyS
                | KeyCode::KeyT
                | KeyCode::KeyU
                | KeyCode::KeyV
                | KeyCode::KeyW
                | KeyCode::KeyX
                | KeyCode::KeyY
                | KeyCode::KeyZ
        )
    }

    /// Check if this key is a function key (F1-F12).
    pub fn is_function(&self) -> bool {
        matches!(
            self,
            KeyCode::F1
                | KeyCode::F2
                | KeyCode::F3
                | KeyCode::F4
                | KeyCode::F5
                | KeyCode::F6
                | KeyCode::F7
                | KeyCode::F8
                | KeyCode::F9
                | KeyCode::F10
                | KeyCode::F11
                | KeyCode::F12
        )
    }
}

/// Keyboard event: combines logical/physical keys, state, and modifiers.
#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    /// Logical key (character or named).
    pub logical_key: LogicalKey,
    /// Physical key code (layout-independent).
    pub physical_key: PhysicalKey,
    /// Whether the button is pressed or released.
    pub state: InputState,
    /// Modifier key state (Ctrl, Shift, Alt, Super).
    pub modifiers: ModifierKeys,
    /// Whether this is a repeat event (key held down).
    pub repeat: bool,
    /// The text produced by this key event, if any (may be multi-char for composed input).
    pub text: Option<String>,
}

impl KeyboardEvent {
    /// Create a new keyboard event with all fields.
    pub fn new(
        logical_key: LogicalKey,
        physical_key: PhysicalKey,
        state: InputState,
        modifiers: ModifierKeys,
        repeat: bool,
        text: Option<String>,
    ) -> Self {
        Self {
            logical_key,
            physical_key,
            state,
            modifiers,
            repeat,
            text,
        }
    }
}

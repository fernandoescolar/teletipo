//! Conversion utilities from winit types to neutral platform-abstraction types.
//!
//! This module translates winit-specific events to backend-neutral types.

use platform_abstraction::{
    InputState, KeyCode, KeyboardEvent, LogicalKey, ModifierKeys, NamedKey, PhysicalKey,
    PointerButton,
};
use winit::event::Modifiers;

/// Convert a winit keyboard event and modifier state to a neutral KeyboardEvent.
pub(crate) fn keyboard_event_from_winit(
    key_event: &winit::event::KeyEvent,
    modifiers: &Modifiers,
) -> KeyboardEvent {
    let logical_key = match &key_event.logical_key {
        winit::keyboard::Key::Character(c) => LogicalKey::Character(c.to_string()),
        winit::keyboard::Key::Named(nk) => LogicalKey::Named(named_key_from_winit(nk)),
        _ => LogicalKey::Named(NamedKey::Other), // Dead key, etc.
    };

    let physical_key = physical_key_from_winit(key_event.physical_key);

    let state = match key_event.state {
        winit::event::ElementState::Pressed => InputState::Pressed,
        winit::event::ElementState::Released => InputState::Released,
    };

    let modifier_keys = modifiers_from_winit(modifiers);

    let text = key_event.text.as_ref().map(|s| s.to_string());

    KeyboardEvent::new(
        logical_key,
        physical_key,
        state,
        modifier_keys,
        key_event.repeat,
        text,
    )
}

/// Convert winit NamedKey to neutral NamedKey.
fn named_key_from_winit(nk: &winit::keyboard::NamedKey) -> NamedKey {
    use winit::keyboard::NamedKey::*;
    match nk {
        Escape => NamedKey::Escape,
        Enter => NamedKey::Enter,
        Tab => NamedKey::Tab,
        Backspace => NamedKey::Backspace,
        Space => NamedKey::Space,
        Delete => NamedKey::Delete,
        ArrowUp => NamedKey::ArrowUp,
        ArrowDown => NamedKey::ArrowDown,
        ArrowLeft => NamedKey::ArrowLeft,
        ArrowRight => NamedKey::ArrowRight,
        Home => NamedKey::Home,
        End => NamedKey::End,
        PageUp => NamedKey::PageUp,
        PageDown => NamedKey::PageDown,
        Insert => NamedKey::Insert,
        F1 => NamedKey::F1,
        F2 => NamedKey::F2,
        F3 => NamedKey::F3,
        F4 => NamedKey::F4,
        F5 => NamedKey::F5,
        F6 => NamedKey::F6,
        F7 => NamedKey::F7,
        F8 => NamedKey::F8,
        F9 => NamedKey::F9,
        F10 => NamedKey::F10,
        F11 => NamedKey::F11,
        F12 => NamedKey::F12,
        CapsLock => NamedKey::CapsLock,
        Shift => NamedKey::Shift,
        Control => NamedKey::Control,
        Alt => NamedKey::Alt,
        Super => NamedKey::Super,
        AltGraph => NamedKey::AltGraph,
        Meta => NamedKey::Meta,
        Hyper => NamedKey::Hyper,
        Paste => NamedKey::Paste,
        _ => NamedKey::Other,
    }
}

/// Convert winit PhysicalKey to neutral PhysicalKey.
fn physical_key_from_winit(pk: winit::keyboard::PhysicalKey) -> PhysicalKey {
    match pk {
        winit::keyboard::PhysicalKey::Code(code) => PhysicalKey::Code(key_code_from_winit(code)),
        winit::keyboard::PhysicalKey::Unidentified(_) => PhysicalKey::Unidentified,
    }
}

/// Convert winit KeyCode to neutral KeyCode.
fn key_code_from_winit(code: winit::keyboard::KeyCode) -> KeyCode {
    use winit::keyboard::KeyCode::*;
    match code {
        Digit1 => KeyCode::Digit1,
        Digit2 => KeyCode::Digit2,
        Digit3 => KeyCode::Digit3,
        Digit4 => KeyCode::Digit4,
        Digit5 => KeyCode::Digit5,
        Digit6 => KeyCode::Digit6,
        Digit7 => KeyCode::Digit7,
        Digit8 => KeyCode::Digit8,
        Digit9 => KeyCode::Digit9,
        Digit0 => KeyCode::Digit0,
        KeyA => KeyCode::KeyA,
        KeyB => KeyCode::KeyB,
        KeyC => KeyCode::KeyC,
        KeyD => KeyCode::KeyD,
        KeyE => KeyCode::KeyE,
        KeyF => KeyCode::KeyF,
        KeyG => KeyCode::KeyG,
        KeyH => KeyCode::KeyH,
        KeyI => KeyCode::KeyI,
        KeyJ => KeyCode::KeyJ,
        KeyK => KeyCode::KeyK,
        KeyL => KeyCode::KeyL,
        KeyM => KeyCode::KeyM,
        KeyN => KeyCode::KeyN,
        KeyO => KeyCode::KeyO,
        KeyP => KeyCode::KeyP,
        KeyQ => KeyCode::KeyQ,
        KeyR => KeyCode::KeyR,
        KeyS => KeyCode::KeyS,
        KeyT => KeyCode::KeyT,
        KeyU => KeyCode::KeyU,
        KeyV => KeyCode::KeyV,
        KeyW => KeyCode::KeyW,
        KeyX => KeyCode::KeyX,
        KeyY => KeyCode::KeyY,
        KeyZ => KeyCode::KeyZ,
        Escape => KeyCode::Escape,
        Space => KeyCode::Space,
        Tab => KeyCode::Tab,
        Enter => KeyCode::Enter,
        Backspace => KeyCode::Backspace,
        Backquote => KeyCode::Backquote,
        BracketLeft => KeyCode::BracketLeft,
        BracketRight => KeyCode::BracketRight,
        Backslash => KeyCode::Backslash,
        Comma => KeyCode::Comma,
        Period => KeyCode::Period,
        Semicolon => KeyCode::Semicolon,
        Quote => KeyCode::Quote,
        Minus => KeyCode::Minus,
        Equal => KeyCode::Equal,
        Slash => KeyCode::Slash,
        F1 => KeyCode::F1,
        F2 => KeyCode::F2,
        F3 => KeyCode::F3,
        F4 => KeyCode::F4,
        F5 => KeyCode::F5,
        F6 => KeyCode::F6,
        F7 => KeyCode::F7,
        F8 => KeyCode::F8,
        F9 => KeyCode::F9,
        F10 => KeyCode::F10,
        F11 => KeyCode::F11,
        F12 => KeyCode::F12,
        Home => KeyCode::Home,
        End => KeyCode::End,
        PageUp => KeyCode::PageUp,
        PageDown => KeyCode::PageDown,
        Insert => KeyCode::Insert,
        Delete => KeyCode::Delete,
        ArrowUp => KeyCode::ArrowUp,
        ArrowDown => KeyCode::ArrowDown,
        ArrowLeft => KeyCode::ArrowLeft,
        ArrowRight => KeyCode::ArrowRight,
        _ => KeyCode::Other(code as u32),
    }
}

/// Convert winit MouseButton to neutral PointerButton.
pub(crate) fn pointer_button_from_winit(button: winit::event::MouseButton) -> PointerButton {
    use winit::event::MouseButton::*;
    match button {
        Left => PointerButton::Left,
        Right => PointerButton::Right,
        Middle => PointerButton::Middle,
        Back => PointerButton::Other(3),
        Forward => PointerButton::Other(4),
        Other(n) => PointerButton::Other(n),
    }
}

/// Convert winit ElementState to neutral InputState.
pub(crate) fn input_state_from_winit(state: winit::event::ElementState) -> InputState {
    match state {
        winit::event::ElementState::Pressed => InputState::Pressed,
        winit::event::ElementState::Released => InputState::Released,
    }
}

/// Convert winit Modifiers to neutral ModifierKeys.
pub(crate) fn modifiers_from_winit(mods: &Modifiers) -> ModifierKeys {
    let state = mods.state();
    ModifierKeys {
        ctrl: state.control_key(),
        shift: state.shift_key(),
        alt: state.alt_key(),
        super_key: state.super_key(),
    }
}

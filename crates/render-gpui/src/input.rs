//! GPUI event conversion to neutral backend-independent types.

use platform_abstraction::{
    AppWindowEvent, InputState, KeyCode, KeyboardEvent, LogicalKey, ModifierKeys, NamedKey,
    PhysicalKey, PointerButton,
};

/// Convert GPUI Modifiers to neutral ModifierKeys.
pub fn convert_modifiers(gpui_mods: &gpui::Modifiers) -> ModifierKeys {
    ModifierKeys {
        ctrl: gpui_mods.control,
        shift: gpui_mods.shift,
        alt: gpui_mods.alt,
        super_key: gpui_mods.platform,
    }
}

/// Convert GPUI key string to neutral LogicalKey and PhysicalKey.
pub fn convert_keys(keystroke: &gpui::Keystroke) -> (LogicalKey, PhysicalKey) {
    let logical = convert_logical_key(&keystroke.key);
    let physical = convert_physical_key(&keystroke.key);
    (logical, physical)
}

fn convert_logical_key(key: &str) -> LogicalKey {
    match key {
        "Enter" => LogicalKey::Named(NamedKey::Enter),
        "Escape" => LogicalKey::Named(NamedKey::Escape),
        "Tab" => LogicalKey::Named(NamedKey::Tab),
        "Backspace" => LogicalKey::Named(NamedKey::Backspace),
        " " => LogicalKey::Named(NamedKey::Space),
        "Delete" => LogicalKey::Named(NamedKey::Delete),
        "ArrowUp" => LogicalKey::Named(NamedKey::ArrowUp),
        "ArrowDown" => LogicalKey::Named(NamedKey::ArrowDown),
        "ArrowLeft" => LogicalKey::Named(NamedKey::ArrowLeft),
        "ArrowRight" => LogicalKey::Named(NamedKey::ArrowRight),
        "Home" => LogicalKey::Named(NamedKey::Home),
        "End" => LogicalKey::Named(NamedKey::End),
        "PageUp" => LogicalKey::Named(NamedKey::PageUp),
        "PageDown" => LogicalKey::Named(NamedKey::PageDown),
        "Insert" => LogicalKey::Named(NamedKey::Insert),
        "F1" => LogicalKey::Named(NamedKey::F1),
        "F2" => LogicalKey::Named(NamedKey::F2),
        "F3" => LogicalKey::Named(NamedKey::F3),
        "F4" => LogicalKey::Named(NamedKey::F4),
        "F5" => LogicalKey::Named(NamedKey::F5),
        "F6" => LogicalKey::Named(NamedKey::F6),
        "F7" => LogicalKey::Named(NamedKey::F7),
        "F8" => LogicalKey::Named(NamedKey::F8),
        "F9" => LogicalKey::Named(NamedKey::F9),
        "F10" => LogicalKey::Named(NamedKey::F10),
        "F11" => LogicalKey::Named(NamedKey::F11),
        "F12" => LogicalKey::Named(NamedKey::F12),
        "CapsLock" => LogicalKey::Named(NamedKey::CapsLock),
        "Shift" => LogicalKey::Named(NamedKey::Shift),
        "Control" => LogicalKey::Named(NamedKey::Control),
        "Alt" => LogicalKey::Named(NamedKey::Alt),
        "Meta" => LogicalKey::Named(NamedKey::Super),
        "AltGraph" => LogicalKey::Named(NamedKey::AltGraph),
        ch => LogicalKey::Character(ch.to_string()),
    }
}

fn convert_physical_key(key: &str) -> PhysicalKey {
    let code = match key {
        "Escape" => KeyCode::Escape,
        "Digit1" => KeyCode::Digit1,
        "Digit2" => KeyCode::Digit2,
        "Digit3" => KeyCode::Digit3,
        "Digit4" => KeyCode::Digit4,
        "Digit5" => KeyCode::Digit5,
        "Digit6" => KeyCode::Digit6,
        "Digit7" => KeyCode::Digit7,
        "Digit8" => KeyCode::Digit8,
        "Digit9" => KeyCode::Digit9,
        "Digit0" => KeyCode::Digit0,
        "Minus" => KeyCode::Minus,
        "Equal" => KeyCode::Equal,
        "Backspace" => KeyCode::Backspace,
        "Tab" => KeyCode::Tab,
        "KeyQ" => KeyCode::KeyQ,
        "KeyW" => KeyCode::KeyW,
        "KeyE" => KeyCode::KeyE,
        "KeyR" => KeyCode::KeyR,
        "KeyT" => KeyCode::KeyT,
        "KeyY" => KeyCode::KeyY,
        "KeyU" => KeyCode::KeyU,
        "KeyI" => KeyCode::KeyI,
        "KeyO" => KeyCode::KeyO,
        "KeyP" => KeyCode::KeyP,
        "BracketLeft" => KeyCode::BracketLeft,
        "BracketRight" => KeyCode::BracketRight,
        "Enter" => KeyCode::Enter,
        "ControlLeft" => KeyCode::ControlLeft,
        "KeyA" => KeyCode::KeyA,
        "KeyS" => KeyCode::KeyS,
        "KeyD" => KeyCode::KeyD,
        "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG,
        "KeyH" => KeyCode::KeyH,
        "KeyJ" => KeyCode::KeyJ,
        "KeyK" => KeyCode::KeyK,
        "KeyL" => KeyCode::KeyL,
        "Semicolon" => KeyCode::Semicolon,
        "Quote" => KeyCode::Quote,
        "Backquote" => KeyCode::Backquote,
        "ShiftLeft" => KeyCode::ShiftLeft,
        "Backslash" => KeyCode::Backslash,
        "KeyZ" => KeyCode::KeyZ,
        "KeyX" => KeyCode::KeyX,
        "KeyC" => KeyCode::KeyC,
        "KeyV" => KeyCode::KeyV,
        "KeyB" => KeyCode::KeyB,
        "KeyN" => KeyCode::KeyN,
        "KeyM" => KeyCode::KeyM,
        "Comma" => KeyCode::Comma,
        "Period" => KeyCode::Period,
        "Slash" => KeyCode::Slash,
        "ShiftRight" => KeyCode::ShiftRight,
        "AltLeft" => KeyCode::AltLeft,
        " " => KeyCode::Space,
        "CapsLock" => KeyCode::CapsLock,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        "Home" => KeyCode::Home,
        "ArrowUp" => KeyCode::ArrowUp,
        "PageUp" => KeyCode::PageUp,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "End" => KeyCode::End,
        "ArrowDown" => KeyCode::ArrowDown,
        "PageDown" => KeyCode::PageDown,
        "Insert" => KeyCode::Insert,
        "Delete" => KeyCode::Delete,
        _ => return PhysicalKey::Unidentified,
    };
    PhysicalKey::Code(code)
}

/// Convert GPUI KeyDownEvent to neutral KeyboardEvent.
pub fn convert_key_down(event: &gpui::KeyDownEvent) -> KeyboardEvent {
    let (logical_key, physical_key) = convert_keys(&event.keystroke);
    let modifiers = convert_modifiers(&event.keystroke.modifiers);

    KeyboardEvent {
        logical_key,
        physical_key,
        state: InputState::Pressed,
        modifiers,
        repeat: event.is_held,
        text: None,
    }
}

/// Convert GPUI KeyUpEvent to neutral KeyboardEvent.
pub fn convert_key_up(event: &gpui::KeyUpEvent) -> KeyboardEvent {
    let (logical_key, physical_key) = convert_keys(&event.keystroke);
    let modifiers = convert_modifiers(&event.keystroke.modifiers);

    KeyboardEvent {
        logical_key,
        physical_key,
        state: InputState::Released,
        modifiers,
        repeat: false,
        text: None,
    }
}

/// Convert GPUI MouseButton to neutral PointerButton.
pub fn convert_mouse_button(button: gpui::MouseButton) -> Option<PointerButton> {
    match button {
        gpui::MouseButton::Left => Some(PointerButton::Left),
        gpui::MouseButton::Right => Some(PointerButton::Right),
        gpui::MouseButton::Middle => Some(PointerButton::Middle),
        gpui::MouseButton::Navigate(_) => {
            // Navigation buttons (back/forward) not yet mapped to PointerButton
            None
        }
    }
}

/// Convert GPUI MouseDownEvent to neutral AppWindowEvent.
pub fn convert_mouse_down(event: &gpui::MouseDownEvent) -> Option<AppWindowEvent> {
    convert_mouse_button(event.button).map(|button| AppWindowEvent::MouseInput {
        state: InputState::Pressed,
        button,
    })
}

/// Convert GPUI MouseUpEvent to neutral AppWindowEvent.
pub fn convert_mouse_up(event: &gpui::MouseUpEvent) -> Option<AppWindowEvent> {
    convert_mouse_button(event.button).map(|button| AppWindowEvent::MouseInput {
        state: InputState::Released,
        button,
    })
}

/// Convert GPUI MouseMoveEvent to neutral AppWindowEvent.
pub fn convert_mouse_move(event: &gpui::MouseMoveEvent) -> AppWindowEvent {
    AppWindowEvent::CursorMoved {
        x: f32::from(event.position.x) as f64,
        y: f32::from(event.position.y) as f64,
    }
}

/// Convert GPUI ScrollWheelEvent to neutral AppWindowEvent.
pub fn convert_scroll_wheel(event: &gpui::ScrollWheelEvent) -> Option<AppWindowEvent> {
    let delta_lines = match event.delta {
        gpui::ScrollDelta::Lines(point) => point.y,
        gpui::ScrollDelta::Pixels(point) => {
            // Estimate lines from pixels (assume ~3 pixels per line)
            f32::from(point.y) / 3.0
        }
    };
    Some(AppWindowEvent::MouseWheel { delta_lines })
}

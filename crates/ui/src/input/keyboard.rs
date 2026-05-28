use crate::actions::{EditorCmd, UiAction};
use crate::state::UiState;
use crate::tab_backend::TabBackend;
use platform_abstraction::AppWindowEvent;
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

pub(super) fn map_keyboard_event<B: TabBackend>(
    state: &UiState<B>,
    event: &AppWindowEvent,
) -> Vec<UiAction> {
    let AppWindowEvent::KeyboardInput(key_event) = event else {
        if let AppWindowEvent::ImeCommit(text) = event
            && !text.is_empty()
            && text != "\r"
            && text != "\n"
        {
            return vec![UiAction::EditorInsert(text.clone())];
        }
        return Vec::new();
    };

    if key_event.state != ElementState::Pressed {
        return Vec::new();
    }

    match &key_event.logical_key {
        Key::Named(NamedKey::Enter) => vec![UiAction::SendReturn],
        Key::Named(NamedKey::Backspace) => vec![UiAction::EditorAction(EditorCmd::Backspace)],
        Key::Named(NamedKey::Delete) => vec![UiAction::EditorAction(EditorCmd::DeleteForward)],
        Key::Named(NamedKey::ArrowLeft) => vec![UiAction::EditorAction(EditorCmd::MoveLeft {
            extend_selection: state.modifiers.shift,
        })],
        Key::Named(NamedKey::ArrowRight) => vec![UiAction::EditorAction(EditorCmd::MoveRight {
            extend_selection: state.modifiers.shift,
        })],
        Key::Named(NamedKey::PageUp) => vec![UiAction::ScrollBy(5)],
        Key::Named(NamedKey::PageDown) => vec![UiAction::ScrollBy(-5)],
        Key::Character(ch) if state.modifiers.super_key && ch.as_str() == "," => {
            vec![UiAction::OpenSettings]
        }
        Key::Character(ch) if !state.modifiers.super_key && !state.modifiers.ctrl => {
            vec![UiAction::EditorInsert(ch.to_string())]
        }
        _ => Vec::new(),
    }
}

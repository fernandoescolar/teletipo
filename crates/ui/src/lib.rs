#![doc = "Shared UI state, actions, and input routing primitives."]
#![warn(missing_docs)]
#![allow(missing_docs)]

pub mod actions;
pub mod components;
pub mod config;
pub mod input;
pub mod state;
pub mod tab_backend;

pub use actions::{EditorCmd, SettingsCmd, UiAction};
pub use components::{
    BellState, ContextMenuState, CursorBlink, DragState, ModifierState, OverlayManager, PaneFocus,
    PaneLayout, ScrollState, SelectionPoint, SelectionState, SettingsState, SuggestionState,
    TabManager, TabPane, UiConfig, WindowMetrics,
};
pub use input::InputRouter;
pub use state::UiState;
pub use tab_backend::TabBackend;

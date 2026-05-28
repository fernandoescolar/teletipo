//! Reusable UI building blocks (tabs, panes, settings, cursor, etc.).
//!
//! Originally a single ~520-line `components.rs`; now split by responsibility:
//! `config`, `selection`, `tabs`, `settings`, `cursor`. All public items are
//! re-exported here so external callers can keep using `ui::components::X`.

mod config;
mod cursor;
mod selection;
mod settings;
mod tabs;

pub use config::UiConfig;
pub use cursor::{BellState, CursorBlink, ModifierState, WindowMetrics};
pub use selection::{ScrollState, SelectionPoint, SelectionState, SuggestionState};
pub use settings::{OverlayManager, SettingsState};
pub use tabs::{ContextMenuState, DragState, PaneFocus, PaneLayout, TabManager, TabPane};

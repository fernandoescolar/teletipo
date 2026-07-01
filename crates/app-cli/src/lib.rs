#![doc = "Application runtime, event loop orchestration, and CLI entry helpers."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod command_registry;
mod commands;
mod completion;
mod config;
mod config_watcher;
mod consts;
mod coords;
mod entry;
mod input;
mod keybindings_ui;
mod launch;
mod layout;
mod metrics;
mod onboarding;
mod palette;
mod runtime;
mod search;
mod settings;
mod shell;
mod snapshot;
mod ssh;
mod state;
mod tab;
mod theme;
mod tick;
pub mod updater;
mod view_model;

#[cfg(test)]
mod input_smoke_tests;

pub(crate) use completion::suggestion_matches_frecency;
pub(crate) use runtime::GpuRuntimeState;
pub(crate) use settings::SettingsUiState;
pub(crate) use state::{
    CursorState, DragState, LayoutState, ModifierState, OverlayState, ThemeFontState, UpdateBanner,
};

pub use entry::run;

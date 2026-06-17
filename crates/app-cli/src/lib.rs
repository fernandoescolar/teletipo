#![doc = "Application runtime, event loop orchestration, and CLI entry helpers."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod commands;
mod completion;
mod config;
mod consts;
mod coords;
mod entry;
mod input;
mod launch;
mod layout;
mod metrics;
mod onboarding;
mod runtime;
mod search;
mod settings;
mod shell;
mod snapshot;
mod ssh;
mod state;
mod tab;
mod theme;
pub mod updater;

#[cfg(test)]
mod input_smoke_tests;

pub(crate) use completion::suggestion_matches_frecency;
pub(crate) use runtime::GpuRuntimeState;
pub(crate) use settings::SettingsUiState;
pub(crate) use state::{
    CursorState, DragState, LayoutState, ModifierState, OverlayState, ThemeFontState, UpdateBanner,
};

pub use entry::run;

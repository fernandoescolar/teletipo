#![doc = "Application orchestration primitives shared by frontend entry points."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod app;
mod runtime;

pub use app::App;
pub use runtime::{AppEvent, AppRuntime, RuntimeConfig};
pub use terminal_core::StyledChars;

pub use terminal_core::{BlockId, ExecutionBlock, ExecutionPhase};

//! Backward-compatibility shim for render model types.
//!
//! The shared render data model now lives in the `render-model` crate so both
//! current and future renderer backends can consume the same types.

pub use render_model::*;

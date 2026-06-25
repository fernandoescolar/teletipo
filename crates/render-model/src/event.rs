// `AppWindowEvent` is defined in `platform-abstraction` so that the UI crate
// can consume window events without depending on a GPU renderer crate.
pub use platform_abstraction::AppWindowEvent;

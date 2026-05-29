#![doc = "PTY backends and session abstractions for terminal process I/O."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod backend;
mod error;
mod mock;
mod session;

pub use backend::PtyBackend;
pub use error::PtyError;
pub use mock::MockPty;
pub use session::PortablePtySession;

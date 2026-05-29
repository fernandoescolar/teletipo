#![doc = "Terminal session orchestration over parser and screen abstractions."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod error;
mod session;

pub use error::TerminalError;
pub use session::{GenericTerminalSession, TerminalDisplay, TerminalParser, TerminalSession};
pub use terminal_screen::{DamageRegion, StyledChars};

#![doc = "ANSI parser and action model for terminal escape handling."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod action;
mod parser;

pub use action::Action;
pub use parser::Parser;

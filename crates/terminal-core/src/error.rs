use thiserror::Error;

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("invalid screen size {rows}x{cols}")]
    InvalidSize { rows: usize, cols: usize },
}

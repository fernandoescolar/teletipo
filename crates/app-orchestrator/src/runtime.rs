use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum AppEvent {
    PtyOutput(Vec<u8>),
    EditorInput(String),
    RenderTick,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub tick_interval: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(16),
        }
    }
}

use crate::app::App;

pub struct AppRuntime {
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    config: RuntimeConfig,
}

impl AppRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, config }
    }

    pub fn sender(&self) -> Sender<AppEvent> {
        self.tx.clone()
    }

    pub fn tick_interval(&self) -> Duration {
        self.config.tick_interval
    }

    pub fn step(&self, app: &mut App) -> bool {
        match self.rx.recv_timeout(self.config.tick_interval) {
            Ok(AppEvent::PtyOutput(bytes)) => {
                app.feed_terminal(&bytes);
                true
            }
            Ok(AppEvent::EditorInput(text)) => {
                app.insert_editor_input(&text);
                true
            }
            Ok(AppEvent::RenderTick) => true,
            Ok(AppEvent::Shutdown) => false,
            Err(mpsc::RecvTimeoutError::Timeout) => true,
            Err(mpsc::RecvTimeoutError::Disconnected) => false,
        }
    }
}

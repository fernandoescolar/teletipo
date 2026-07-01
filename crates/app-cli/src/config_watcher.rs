use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub enum ConfigReloadEvent {
    ConfigChanged,
}

pub struct ConfigWatcher {
    pub rx: Option<Receiver<ConfigReloadEvent>>,
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    pub fn start(config_dir: PathBuf) -> anyhow::Result<Self> {
        let (tx, rx) = channel();
        let last_event = Arc::new(Mutex::new(std::time::Instant::now()));

        let debounce_tx = tx.clone();
        let debounce_time = Duration::from_millis(500);
        let last_event_clone = Arc::clone(&last_event);

        let mut watcher = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                if let Ok(e) = event {
                    // Only care about write/create events on .toml files
                    if matches!(
                        e.kind,
                        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                    ) {
                        // Check if this is a .toml or .yaml file
                        let is_config_file = e.paths.iter().any(|p| {
                            p.extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| ext == "toml" || ext == "yaml" || ext == "yml")
                                .unwrap_or(false)
                        });

                        if is_config_file {
                            let now = std::time::Instant::now();
                            let mut last = last_event_clone.lock().unwrap();

                            // Debounce: only send event if 500ms+ has passed
                            if now.duration_since(*last) >= debounce_time {
                                *last = now;
                                let _ = debounce_tx.send(ConfigReloadEvent::ConfigChanged);
                            }
                        }
                    }
                }
            },
            notify::Config::default(),
        )?;

        watcher.watch(&config_dir, RecursiveMode::Recursive)?;

        Ok(ConfigWatcher {
            rx: Some(rx),
            _watcher: watcher,
        })
    }
}

use anyhow::Context;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{Event, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Icon, Window, WindowBuilder};

use crate::error::RenderError;
use crate::geometry::snapshot_to_ime_area;
use crate::pipeline::GpuState;
use crate::types::{AppWindowEvent, RenderConfig, RenderSnapshot};
use platform_abstraction::{WindowControl, apply_app_icon, apply_titlebar_color};

type Result<T> = std::result::Result<T, RenderError>;

const APP_ICON_PNG: &[u8] = include_bytes!("../../../docs/teletipo128x128.png");

fn load_window_icon() -> Option<Icon> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

pub fn run_gpu_window(snapshot: RenderSnapshot, config: RenderConfig) -> Result<()> {
    run_gpu_window_live(move || snapshot.clone(), config)
}

pub fn run_gpu_window_live<F>(next_snapshot: F, config: RenderConfig) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
{
    run_gpu_window_live_with_events(next_snapshot, |_| {}, config)
}

/// [`WindowControl`] implementation backed by a `'static` reference to the
/// winit [`Window`] owned by the event loop. Constructed once during
/// [`run_gpu_window_live_with_events_and_window`] startup and passed to the
/// caller-supplied `on_window_ready` callback before the loop starts pumping
/// events.
struct WinitWindowControl {
    window: &'static Window,
}

impl WindowControl for WinitWindowControl {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    fn open_url(&self, url: &str) {
        // Only allow well-known safe URL schemes to avoid passing arbitrary
        // strings (e.g. shell metacharacters) to the OS "open" handler.
        const ALLOWED_PREFIXES: &[&str] = &["http://", "https://", "file://", "mailto:", "ftp://"];
        if !ALLOWED_PREFIXES.iter().any(|p| url.starts_with(p)) {
            tracing::warn!(url, "refusing to open URL with disallowed scheme");
            return;
        }
        let result = {
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open").arg(url).spawn()
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("xdg-open").arg(url).spawn()
            }
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("cmd")
                    .args(["/C", "start", "", url])
                    .spawn()
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                Err::<std::process::Child, std::io::Error>(std::io::Error::other(
                    "unsupported platform",
                ))
            }
        };
        if let Err(err) = result {
            tracing::warn!(error = %err, url, "failed to open URL");
        }
    }
}

pub fn run_gpu_window_live_with_events<F, E>(
    next_snapshot: F,
    on_event: E,
    config: RenderConfig,
) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
    E: 'static + FnMut(AppWindowEvent),
{
    run_gpu_window_live_with_events_and_window(next_snapshot, on_event, |_| {}, config)
}

#[allow(clippy::too_many_lines)] // winit event-loop wiring; refactor tracked separately
pub fn run_gpu_window_live_with_events_and_window<F, E, W>(
    mut next_snapshot: F,
    mut on_event: E,
    on_window_ready: W,
    config: RenderConfig,
) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
    E: 'static + FnMut(AppWindowEvent),
    W: FnOnce(Box<dyn WindowControl>),
{
    let initial = next_snapshot();
    let event_loop = EventLoop::new().map_err(RenderError::event_loop)?;
    let title = format!("\u{1F4C2} {}", initial.title_cwd);
    let window = {
        let mut builder = WindowBuilder::new()
            .with_title(title)
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(
                config.initial_size.map_or(1280, |(w, _)| w) as f64,
                config.initial_size.map_or(720, |(_, h)| h) as f64,
            ));
        if let Some((px, py)) = config.initial_position {
            builder = builder.with_position(PhysicalPosition::new(px, py));
        }
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowBuilderExtMacOS;
            builder = builder.with_titlebar_transparent(true);
        }
        builder.build(&event_loop).map_err(RenderError::window)?
    };
    let window: &'static Window = Box::leak(Box::new(window));
    window.set_ime_allowed(true);
    on_window_ready(Box::new(WinitWindowControl { window }));
    apply_app_icon(APP_ICON_PNG);
    apply_titlebar_color(window, initial.theme.terminal_bg);

    let base_font_size = config.font.font_size;
    let mut config = config;
    config.font.font_size *= window.scale_factor() as f32;

    let mut state =
        pollster::block_on(GpuState::new(window, &config)).map_err(RenderError::gpu_init)?;
    #[cfg(target_os = "macos")]
    let mut last_titlebar_bg = initial.theme.terminal_bg;
    let mut last_title: String = format!("\u{1F4C2} {}", initial.title_cwd);

    // Fire an initial resize event so the caller can size the terminal grid to match
    // the actual window dimensions and font metrics before the first frame is drawn.
    {
        let sz = window.inner_size();
        on_event(AppWindowEvent::Resized {
            width: sz.width,
            height: sz.height,
            scale_factor: window.scale_factor(),
            cell_w: state.cell_w_px,
            cell_h: state.cell_h_px,
        });
    }

    #[allow(deprecated)]
    event_loop
        .run(move |event, target| {
            target.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => {
                            on_event(AppWindowEvent::CloseRequested);
                            target.exit();
                        }
                        WindowEvent::Moved(pos) => {
                            on_event(AppWindowEvent::WindowMoved { x: pos.x, y: pos.y });
                        }
                        WindowEvent::Resized(size) => {
                            state.resize(size);
                            on_event(AppWindowEvent::Resized {
                                width: size.width,
                                height: size.height,
                                scale_factor: window.scale_factor(),
                                cell_w: state.cell_w_px,
                                cell_h: state.cell_h_px,
                            });
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = window.inner_size();
                            state.resize(size);
                            let sf = window.scale_factor();
                            state.rescale_font(base_font_size, sf);
                            on_event(AppWindowEvent::Resized {
                                width: size.width,
                                height: size.height,
                                scale_factor: sf,
                                cell_w: state.cell_w_px,
                                cell_h: state.cell_h_px,
                            });
                        }
                        WindowEvent::ModifiersChanged(mods) => {
                            on_event(AppWindowEvent::ModifiersChanged(mods.state()));
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            on_event(AppWindowEvent::CursorMoved {
                                x: position.x,
                                y: position.y,
                            });
                        }
                        WindowEvent::MouseInput { state, button, .. } => {
                            on_event(AppWindowEvent::MouseInput { state, button });
                        }
                        WindowEvent::MouseWheel { delta, .. } => {
                            let dy = match delta {
                                MouseScrollDelta::LineDelta(_, y) => y,
                                MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 20.0,
                            };
                            if dy != 0.0 {
                                on_event(AppWindowEvent::MouseWheel { delta_lines: dy });
                            }
                        }
                        WindowEvent::KeyboardInput { event, .. } => {
                            on_event(AppWindowEvent::KeyboardInput(event));
                        }
                        WindowEvent::Ime(Ime::Commit(text)) => {
                            on_event(AppWindowEvent::ImeCommit(text));
                        }
                        WindowEvent::RedrawRequested => {
                            let snapshot = next_snapshot();
                            if snapshot.request_exit {
                                target.exit();
                                return;
                            }
                            #[cfg(target_os = "macos")]
                            if snapshot.theme.terminal_bg != last_titlebar_bg {
                                last_titlebar_bg = snapshot.theme.terminal_bg;
                                apply_titlebar_color(window, last_titlebar_bg);
                            }
                            let new_title = format!("\u{1F4C2} {}", snapshot.title_cwd);
                            if new_title != last_title {
                                last_title = new_title.clone();
                                window.set_title(&new_title);
                            }
                            if snapshot.editor_focused {
                                let (ime_pos, ime_size) =
                                    snapshot_to_ime_area(&snapshot, state.size);
                                window.set_ime_cursor_area(ime_pos, ime_size);
                            }
                            if let Err(err) = state.render(&snapshot) {
                                tracing::error!(error = %err, "render error");
                                target.exit();
                            }
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => window.request_redraw(),
                _ => {}
            }
        })
        .context("run event loop")
        .map_err(RenderError::event_loop)
}

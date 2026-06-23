use anyhow::Context;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{Event, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use winit::window::{Icon, Window, WindowBuilder};

use crate::error::RenderError;
use crate::geometry::snapshot_to_ime_area;
use crate::pipeline::GpuState;
use crate::types::{AppWindowEvent, RenderConfig, RenderSnapshot};
use platform_abstraction::{WindowControl, apply_app_icon, apply_titlebar_color};

type Result<T> = std::result::Result<T, RenderError>;

const APP_ICON_PNG: &[u8] = include_bytes!("../../../docs/teletipo128x128.png");

fn format_window_title(title_cwd: &str) -> String {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        // Linux/Windows titlebar fonts may not include emoji glyphs.
        format!("teletipo - {title_cwd}")
    }
    #[cfg(target_os = "macos")]
    {
        format!("\u{1F4C2} {title_cwd}")
    }
}

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

struct LoopState<'a, E, F> {
    gpu: GpuState<'a>,
    on_event: E,
    next_snapshot: F,
    base_font_size: f32,
    last_title: String,
    #[cfg(target_os = "macos")]
    last_titlebar_bg: [f32; 4],
}

impl<'a, E, F> LoopState<'a, E, F>
where
    E: FnMut(AppWindowEvent),
    F: FnMut() -> RenderSnapshot,
{
    fn dispatch(
        &mut self,
        event: Event<()>,
        window: &'static Window,
        target: &EventLoopWindowTarget<()>,
    ) {
        target.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                self.handle_window_event(event, window, target);
            }
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    }

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
        window: &'static Window,
        target: &EventLoopWindowTarget<()>,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                (self.on_event)(AppWindowEvent::CloseRequested);
                target.exit();
            }
            WindowEvent::Moved(pos) => {
                (self.on_event)(AppWindowEvent::WindowMoved { x: pos.x, y: pos.y });
            }
            WindowEvent::Resized(size) => {
                self.gpu.resize(size);
                (self.on_event)(AppWindowEvent::Resized {
                    width: size.width,
                    height: size.height,
                    scale_factor: window.scale_factor(),
                    cell_w: self.gpu.cell_w_px,
                    cell_h: self.gpu.cell_h_px,
                });
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = window.inner_size();
                self.gpu.resize(size);
                let sf = window.scale_factor();
                self.gpu.rescale_font(self.base_font_size, sf);
                (self.on_event)(AppWindowEvent::Resized {
                    width: size.width,
                    height: size.height,
                    scale_factor: sf,
                    cell_w: self.gpu.cell_w_px,
                    cell_h: self.gpu.cell_h_px,
                });
            }
            WindowEvent::Focused(focused) => {
                (self.on_event)(AppWindowEvent::WindowFocused(focused));
            }
            WindowEvent::ModifiersChanged(mods) => {
                (self.on_event)(AppWindowEvent::ModifiersChanged(mods.state()));
            }
            WindowEvent::CursorMoved { position, .. } => {
                (self.on_event)(AppWindowEvent::CursorMoved {
                    x: position.x,
                    y: position.y,
                });
            }
            WindowEvent::MouseInput { state, button, .. } => {
                (self.on_event)(AppWindowEvent::MouseInput { state, button });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 20.0,
                };
                if dy != 0.0 {
                    (self.on_event)(AppWindowEvent::MouseWheel { delta_lines: dy });
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                (self.on_event)(AppWindowEvent::KeyboardInput(event));
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                (self.on_event)(AppWindowEvent::ImeCommit(text));
            }
            WindowEvent::DroppedFile(path) => {
                (self.on_event)(AppWindowEvent::DroppedFile(path));
            }
            WindowEvent::RedrawRequested => self.handle_redraw(window, target),
            _ => {}
        }
    }

    fn handle_redraw(&mut self, window: &'static Window, target: &EventLoopWindowTarget<()>) {
        let snapshot = (self.next_snapshot)();
        if snapshot.request_exit {
            target.exit();
            return;
        }
        #[cfg(target_os = "macos")]
        if snapshot.theme.terminal_bg != self.last_titlebar_bg {
            self.last_titlebar_bg = snapshot.theme.terminal_bg;
            apply_titlebar_color(window, self.last_titlebar_bg);
        }
        let new_title = format_window_title(&snapshot.title_cwd);
        if new_title != self.last_title {
            self.last_title = new_title.clone();
            window.set_title(&new_title);
        }
        if snapshot.editor_focused {
            let (ime_pos, ime_size) = snapshot_to_ime_area(&snapshot, self.gpu.size);
            window.set_ime_cursor_area(ime_pos, ime_size);
        }
        if let Err(err) = self.gpu.render(&snapshot) {
            tracing::error!(error = %err, "render error");
            target.exit();
        }
    }
}

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
    let title = format_window_title(&initial.title_cwd);
    let window = {
        let mut builder = WindowBuilder::new()
            .with_title(title)
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(
                config.initial_size.map_or(1280, |(w, _)| w) as f64,
                config.initial_size.map_or(720, |(_, h)| h) as f64,
            ));
        #[cfg(target_os = "linux")]
        {
            use winit::platform::wayland::WindowBuilderExtWayland;
            use winit::platform::x11::WindowBuilderExtX11;
            builder = WindowBuilderExtX11::with_name(builder, "teletipo", "teletipo");
            builder = WindowBuilderExtWayland::with_name(builder, "teletipo", "teletipo");
        }
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
    let gpu = pollster::block_on(GpuState::new(window, &config)).map_err(RenderError::gpu_init)?;

    // Fire an initial resize event so the caller can size the terminal grid to match
    // the actual window dimensions and font metrics before the first frame is drawn.
    let sz = window.inner_size();
    on_event(AppWindowEvent::Resized {
        width: sz.width,
        height: sz.height,
        scale_factor: window.scale_factor(),
        cell_w: gpu.cell_w_px,
        cell_h: gpu.cell_h_px,
    });

    let mut loop_state = LoopState {
        gpu,
        on_event,
        next_snapshot,
        base_font_size,
        last_title: format_window_title(&initial.title_cwd),
        #[cfg(target_os = "macos")]
        last_titlebar_bg: initial.theme.terminal_bg,
    };

    #[allow(deprecated)]
    event_loop
        .run(move |event, target| loop_state.dispatch(event, window, target))
        .context("run event loop")
        .map_err(RenderError::event_loop)
}

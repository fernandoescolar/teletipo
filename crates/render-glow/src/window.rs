use std::ffi::CString;
use std::num::NonZeroU32;

use crate::{AppWindowEvent, RenderConfig, RenderSnapshot};
use anyhow::Context;
use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use platform_abstraction::{WindowControl, apply_app_icon, apply_titlebar_color};
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{Event, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Icon, Window, WindowBuilder};

use crate::painter::GlPainter;

const APP_ICON_PNG: &[u8] = include_bytes!("../../../docs/teletipo128x128.png");

type Result<T> = anyhow::Result<T>;

/// A thread-safe handle that wakes the render loop to schedule a redraw.
/// Wraps an `EventLoopProxy` so PTY reader threads can trigger rendering
/// when new data arrives without waiting for the next compositor frame callback.
#[derive(Clone)]
pub struct Redrawer {
    proxy: EventLoopProxy<()>,
}

impl Redrawer {
    /// Wake the event loop and request a redraw. Safe to call from any thread.
    pub fn request_redraw(&self) {
        let _ = self.proxy.send_event(());
    }
}

fn format_window_title(title_cwd: &str) -> String {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
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

pub fn run_gpu_window_live_with_events<F, E>(
    next_snapshot: F,
    on_event: E,
    config: RenderConfig,
) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
    E: 'static + FnMut(AppWindowEvent),
{
    run_gpu_window_live_with_events_and_window(next_snapshot, on_event, |_, _| {}, config)
}

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

fn build_gl_context(
    window: &'static Window,
    gl_config: &glutin::config::Config,
) -> Result<(PossiblyCurrentContext, Surface<WindowSurface>)> {
    let raw_window_handle = window.raw_window_handle();
    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(None))
        .build(Some(raw_window_handle));
    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));

    let not_current = unsafe {
        gl_config
            .display()
            .create_context(gl_config, &context_attributes)
            .or_else(|_| {
                gl_config
                    .display()
                    .create_context(gl_config, &fallback_context_attributes)
            })
    }
    .context("create OpenGL context")?;

    let size = window.inner_size();
    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).expect("width max(1) is non-zero"),
        NonZeroU32::new(size.height.max(1)).expect("height max(1) is non-zero"),
    );
    let surface = unsafe {
        gl_config
            .display()
            .create_window_surface(gl_config, &attrs)
            .context("create OpenGL surface")?
    };

    let context = not_current
        .make_current(&surface)
        .context("make OpenGL context current")?;

    Ok((context, surface))
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
    W: FnOnce(Box<dyn WindowControl>, Redrawer),
{
    let initial = next_snapshot();
    let event_loop = EventLoop::new().context("create event loop")?;
    let proxy = event_loop.create_proxy();
    let title = format_window_title(&initial.title_cwd);

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

    // Enable window-level transparency when the configured opacity is below 1.0.
    // The alpha channel on the GL config (with_alpha_size(8)) is already requested.
    // On Linux this requires a compositing WM (KWin, Mutter, Picom, etc.).
    if config.opacity < 1.0 {
        builder = builder.with_transparent(true);
    }

    // Note: On macOS, OpenGL/Metal backend imposes ≥2× MSAA automatically —
    // there's no way to disable it from user-land without using Metal directly
    // or wgpu instead. This logging helps diagnose framebuffer allocation issues.
    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(builder));
    let (window_opt, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            // Select the config with the most reasonable tradeoffs (many drivers
            // offer multiple options; prefer better AA over e.g. sRGB if needed).
            let chosen = configs
                .reduce(|best, config| {
                    if config.num_samples() > best.num_samples() {
                        config
                    } else {
                        best
                    }
                })
                .expect("at least one GL config");
            tracing::info!(
                samples = chosen.num_samples(),
                depth_bits = chosen.depth_size(),
                stencil_bits = chosen.stencil_size(),
                "GL config selected"
            );
            chosen
        })
        .map_err(|err| anyhow::anyhow!("create OpenGL display/window: {err}"))?;

    let window = window_opt.context("display builder returned no window")?;
    let window: &'static Window = Box::leak(Box::new(window));
    window.set_ime_allowed(true);

    on_window_ready(Box::new(WinitWindowControl { window }), Redrawer { proxy });
    apply_app_icon(APP_ICON_PNG);
    apply_titlebar_color(window, initial.theme.terminal_bg);

    let (gl_context, gl_surface) = build_gl_context(window, &gl_config)?;

    let _ = gl_surface.set_swap_interval(
        &gl_context,
        SwapInterval::Wait(NonZeroU32::new(1).expect("non-zero swap interval")),
    );

    let gl_display = gl_config.display();
    let gl = unsafe {
        glow::Context::from_loader_function(|symbol| {
            let symbol = CString::new(symbol).expect("symbol names are not null-terminated");
            gl_display.get_proc_address(&symbol) as *const _
        })
    };

    let base_font_size = config.font.font_size;
    let scaled_font_size = base_font_size * window.scale_factor() as f32;
    let mut painter = GlPainter::new(&gl, config.font.font_family.clone(), scaled_font_size)?;
    let (mut cell_w_px, mut cell_h_px) = painter.cell_metrics();
    let mut last_font_size = base_font_size;
    let mut last_title: String = format_window_title(&initial.title_cwd);
    #[cfg(target_os = "macos")]
    let mut last_titlebar_bg = initial.theme.terminal_bg;

    {
        let sz = window.inner_size();
        on_event(AppWindowEvent::Resized {
            width: sz.width,
            height: sz.height,
            scale_factor: window.scale_factor(),
            cell_w: cell_w_px,
            cell_h: cell_h_px,
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
                            gl_surface.resize(
                                &gl_context,
                                NonZeroU32::new(size.width.max(1))
                                    .expect("width max(1) is non-zero"),
                                NonZeroU32::new(size.height.max(1))
                                    .expect("height max(1) is non-zero"),
                            );
                            on_event(AppWindowEvent::Resized {
                                width: size.width,
                                height: size.height,
                                scale_factor: window.scale_factor(),
                                cell_w: cell_w_px,
                                cell_h: cell_h_px,
                            });
                        }
                        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            let size = window.inner_size();
                            gl_surface.resize(
                                &gl_context,
                                NonZeroU32::new(size.width.max(1))
                                    .expect("width max(1) is non-zero"),
                                NonZeroU32::new(size.height.max(1))
                                    .expect("height max(1) is non-zero"),
                            );
                            let scaled_font_size = base_font_size * scale_factor as f32;
                            painter.set_font_size(scaled_font_size);
                            painter.clear_atlas_textures(&gl);
                            (cell_w_px, cell_h_px) = painter.cell_metrics();
                            on_event(AppWindowEvent::Resized {
                                width: size.width,
                                height: size.height,
                                scale_factor,
                                cell_w: cell_w_px,
                                cell_h: cell_h_px,
                            });
                        }
                        WindowEvent::Focused(focused) => {
                            if focused {
                                let size = window.inner_size();
                                gl_surface.resize(
                                    &gl_context,
                                    NonZeroU32::new(size.width.max(1))
                                        .expect("width max(1) is non-zero"),
                                    NonZeroU32::new(size.height.max(1))
                                        .expect("height max(1) is non-zero"),
                                );
                                painter.invalidate_text_atlases(&gl);
                                window.request_redraw();
                            }
                            on_event(AppWindowEvent::WindowFocused(focused));
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
                        WindowEvent::DroppedFile(path) => {
                            on_event(AppWindowEvent::DroppedFile(path));
                        }
                        WindowEvent::RedrawRequested => {
                            let snapshot = next_snapshot();
                            if snapshot.request_exit {
                                target.exit();
                                return;
                            }
                            if snapshot.font_size != last_font_size {
                                last_font_size = snapshot.font_size;
                                let sf = window.scale_factor() as f32;
                                painter.set_font_size(snapshot.font_size * sf);
                                painter.clear_atlas_textures(&gl);
                                (cell_w_px, cell_h_px) = painter.cell_metrics();
                                let sz = window.inner_size();
                                on_event(AppWindowEvent::Resized {
                                    width: sz.width,
                                    height: sz.height,
                                    scale_factor: window.scale_factor(),
                                    cell_w: cell_w_px,
                                    cell_h: cell_h_px,
                                });
                            }
                            #[cfg(target_os = "macos")]
                            if snapshot.theme.terminal_bg != last_titlebar_bg {
                                last_titlebar_bg = snapshot.theme.terminal_bg;
                                apply_titlebar_color(window, last_titlebar_bg);
                            }
                            let new_title = format_window_title(&snapshot.title_cwd);
                            if new_title != last_title {
                                last_title = new_title.clone();
                                window.set_title(&new_title);
                            }
                            // IME cursor area setting disabled - snapshot_to_ime_area was removed with render-wgpu
                            // if snapshot.editor_focused {
                            //     if let Some((_x, _y, _w, _h)) = snapshot_to_ime_area(&snapshot) {
                            //         window.set_ime_cursor_area((x, y), (w, h));
                            //     }
                            // }

                            let sz = window.inner_size();
                            unsafe {
                                gl.viewport(0, 0, sz.width as i32, sz.height as i32);
                                gl.clear_color(
                                    snapshot.theme.terminal_bg[0],
                                    snapshot.theme.terminal_bg[1],
                                    snapshot.theme.terminal_bg[2],
                                    // Multiply the theme's alpha by the user-configured
                                    // opacity so backgrounds can be semi-transparent.
                                    snapshot.theme.terminal_bg[3] * snapshot.opacity,
                                );
                                gl.clear(glow::COLOR_BUFFER_BIT);
                            }

                            painter.render(&gl, &snapshot, sz, cell_w_px, cell_h_px);

                            if let Err(err) = gl_surface.swap_buffers(&gl_context) {
                                tracing::error!(error = %err, "OpenGL swap-buffers error");
                                target.exit();
                            }
                        }
                        _ => {}
                    }
                }
                Event::Resumed => {
                    painter.invalidate_text_atlases(&gl);
                    window.request_redraw();
                }
                Event::UserEvent(()) => {
                    window.request_redraw();
                }
                Event::AboutToWait => window.request_redraw(),
                _ => {}
            }
        })
        .context("run event loop")?;

    Ok(())
}

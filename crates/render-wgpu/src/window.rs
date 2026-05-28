use anyhow::{Context, Result};
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{Event, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Icon, Window, WindowBuilder};

use crate::geometry::snapshot_to_ime_area;
use crate::pipeline::GpuState;
use crate::types::{AppWindowEvent, RenderConfig, RenderSnapshot};

const APP_ICON_PNG: &[u8] = include_bytes!("../../../docs/teletipo128x128.png");

fn load_window_icon() -> Option<Icon> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

/// On macOS, set the Dock/application icon to the embedded Teletipo logo.
#[cfg(target_os = "macos")]
fn apply_app_icon() {
    use objc2::class;
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let data: *mut AnyObject = msg_send![
            class!(NSData),
            dataWithBytes: APP_ICON_PNG.as_ptr() as *const core::ffi::c_void
            length: APP_ICON_PNG.len()
        ];
        if data.is_null() {
            return;
        }
        let img_alloc: *mut AnyObject = msg_send![class!(NSImage), alloc];
        if img_alloc.is_null() {
            return;
        }
        let img: *mut AnyObject = msg_send![img_alloc, initWithData: &*data];
        if img.is_null() {
            return;
        }
        let _: () = msg_send![app, setApplicationIconImage: &*img];
    }
}

/// On macOS, make the native title bar take the given RGBA colour so it blends
/// with the rendered content rather than showing the default vibrancy.
#[cfg(target_os = "macos")]
fn apply_titlebar_color(window: &Window, [r, g, b, a]: [f32; 4]) {
    use objc2::class;
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let ns_view = appkit.ns_view.as_ptr() as *mut AnyObject;

    unsafe {
        // Get the NSWindow from the NSView.
        let ns_window: *mut AnyObject = msg_send![&*ns_view, window];
        if ns_window.is_null() {
            return;
        }
        let cls = class!(NSColor);
        let color: *mut AnyObject = msg_send![
            cls,
            colorWithSRGBRed: (r as f64)
            green: (g as f64)
            blue: (b as f64)
            alpha: (a as f64)
        ];
        let _: () = msg_send![&*ns_window, setBackgroundColor: &*color];

        // Pick dark or light NSAppearance so the title bar text colour (which
        // macOS controls automatically) matches the theme background.
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let name_bytes: &[u8] = if lum < 0.5 {
            b"NSAppearanceNameDarkAqua\0"
        } else {
            b"NSAppearanceNameAqua\0"
        };
        let ns_name: *mut AnyObject = msg_send![
            class!(NSString),
            stringWithUTF8String: name_bytes.as_ptr()
        ];
        let appearance: *mut AnyObject = msg_send![
            class!(NSAppearance),
            appearanceNamed: &*ns_name
        ];
        if !appearance.is_null() {
            let _: () = msg_send![&*ns_window, setAppearance: &*appearance];
        }
    }
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
    mut next_snapshot: F,
    mut on_event: E,
    config: RenderConfig,
) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
    E: 'static + FnMut(AppWindowEvent),
{
    let initial = next_snapshot();
    let event_loop = EventLoop::new().context("create event loop")?;
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
        builder.build(&event_loop).context("create window")?
    };
    let window: &'static Window = Box::leak(Box::new(window));
    window.set_ime_allowed(true);
    #[cfg(target_os = "macos")]
    apply_app_icon();
    #[cfg(target_os = "macos")]
    apply_titlebar_color(window, initial.theme.terminal_bg);

    let base_font_size = config.font.font_size;
    let mut config = config;
    config.font.font_size *= window.scale_factor() as f32;

    let mut state = pollster::block_on(GpuState::new(window, &config))?;
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
    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
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
                        let (ime_pos, ime_size) = snapshot_to_ime_area(&snapshot, state.size);
                        window.set_ime_cursor_area(ime_pos, ime_size);
                    }
                    if let Err(err) = state.render(&snapshot) {
                        eprintln!("render error: {err}");
                        target.exit();
                    }
                }
                _ => {}
            },
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    })
    .context("run event loop")
}

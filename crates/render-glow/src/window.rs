use std::ffi::CString;
use std::num::NonZeroU32;

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
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Icon, Window, WindowBuilder};

// Window functions removed - they depended on render-wgpu types that no longer exist
// use crate::painter::GlPainter;

const APP_ICON_PNG: &[u8] = include_bytes!("../../../docs/teletipo128x128.png");

type Result<T> = anyhow::Result<T>;

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

// Window rendering functions removed - render-wgpu crate has been deleted
// This module is now non-functional and should be removed or rebuilt when types are refactored


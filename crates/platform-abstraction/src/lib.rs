#![doc = "Cross-platform traits and adapters for OS-specific services."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod impls;
mod macos;
mod platform;
mod process;
mod traits;
mod types;

pub use impls::{
    BasicFontFallback, FixedDpi, MemoryAccessibility, MemoryClipboard, MemoryIme, SystemClipboard,
};
pub use macos::{MacOSAccessibility, apply_app_icon, apply_titlebar_color};
pub use platform::{
    NativePlatformServices, PlatformServices, current_platform, default_shell,
    detect_display_backend, detect_display_backend_from, native_services,
};
pub use process::{SystemProcessInfo, current_process_info};
pub use traits::{
    Accessibility, Clipboard, DpiAwareness, FontFallback, Ime, ProcessInfo, WindowControl,
};
pub use types::{AccessNode, AccessibilityTree, AppWindowEvent, DisplayBackend, PlatformKind};

#[cfg(target_os = "linux")]
pub use platform::{LinuxAccessibility, LinuxClipboard, LinuxDpi, LinuxFontFallback, LinuxIme};

#[cfg(target_os = "macos")]
pub use platform::{MacAccessibility, MacClipboard, MacDpi, MacFontFallback, MacIme};

#[cfg(target_os = "windows")]
pub use platform::{
    WindowsAccessibility, WindowsClipboard, WindowsDpi, WindowsFontFallback, WindowsIme,
};

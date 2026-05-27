mod impls;
mod platform;
mod traits;
mod types;

pub use impls::{BasicFontFallback, FixedDpi, MemoryAccessibility, MemoryClipboard, MemoryIme};
pub use platform::{
    current_platform, default_shell, detect_display_backend, detect_display_backend_from,
    native_services, NativePlatformServices, PlatformServices,
};
pub use traits::{Accessibility, Clipboard, DpiAwareness, FontFallback, Ime};
pub use types::{DisplayBackend, PlatformKind};

#[cfg(target_os = "linux")]
pub use platform::{
    LinuxAccessibility, LinuxClipboard, LinuxDpi, LinuxFontFallback, LinuxIme,
};

#[cfg(target_os = "macos")]
pub use platform::{MacAccessibility, MacClipboard, MacDpi, MacFontFallback, MacIme};

#[cfg(target_os = "windows")]
pub use platform::{
    WindowsAccessibility, WindowsClipboard, WindowsDpi, WindowsFontFallback, WindowsIme,
};

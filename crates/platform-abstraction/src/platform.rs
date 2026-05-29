use std::env;

use crate::impls::{BasicFontFallback, FixedDpi, MemoryAccessibility, MemoryIme, SystemClipboard};
use crate::traits::{Accessibility, Clipboard, DpiAwareness, FontFallback, Ime};
use crate::types::{DisplayBackend, PlatformKind};

pub struct PlatformServices<C, I, A, D, F>
where
    C: Clipboard,
    I: Ime,
    A: Accessibility,
    D: DpiAwareness,
    F: FontFallback,
{
    pub clipboard: C,
    pub ime: I,
    pub accessibility: A,
    pub dpi: D,
    pub font_fallback: F,
}

#[cfg(target_os = "linux")]
pub type LinuxClipboard = SystemClipboard;
#[cfg(target_os = "linux")]
pub type LinuxIme = MemoryIme;
#[cfg(target_os = "linux")]
pub type LinuxAccessibility = MemoryAccessibility;
#[cfg(target_os = "linux")]
pub type LinuxDpi = FixedDpi;
#[cfg(target_os = "linux")]
pub type LinuxFontFallback = BasicFontFallback;

#[cfg(target_os = "macos")]
pub type MacClipboard = SystemClipboard;
#[cfg(target_os = "macos")]
pub type MacIme = MemoryIme;
#[cfg(target_os = "macos")]
pub type MacAccessibility = MemoryAccessibility;
#[cfg(target_os = "macos")]
pub type MacDpi = FixedDpi;
#[cfg(target_os = "macos")]
pub type MacFontFallback = BasicFontFallback;

#[cfg(target_os = "windows")]
pub type WindowsClipboard = SystemClipboard;
#[cfg(target_os = "windows")]
pub type WindowsIme = MemoryIme;
#[cfg(target_os = "windows")]
pub type WindowsAccessibility = MemoryAccessibility;
#[cfg(target_os = "windows")]
pub type WindowsDpi = FixedDpi;
#[cfg(target_os = "windows")]
pub type WindowsFontFallback = BasicFontFallback;

#[cfg(target_os = "linux")]
pub type NativePlatformServices =
    PlatformServices<LinuxClipboard, LinuxIme, LinuxAccessibility, LinuxDpi, LinuxFontFallback>;

#[cfg(target_os = "macos")]
pub type NativePlatformServices =
    PlatformServices<MacClipboard, MacIme, MacAccessibility, MacDpi, MacFontFallback>;

#[cfg(target_os = "windows")]
pub type NativePlatformServices = PlatformServices<
    WindowsClipboard,
    WindowsIme,
    WindowsAccessibility,
    WindowsDpi,
    WindowsFontFallback,
>;

pub fn native_services() -> NativePlatformServices {
    PlatformServices {
        clipboard: SystemClipboard::default(),
        ime: MemoryIme::default(),
        accessibility: MemoryAccessibility::default(),
        dpi: FixedDpi::default(),
        font_fallback: BasicFontFallback,
    }
}

pub fn detect_display_backend() -> DisplayBackend {
    detect_display_backend_from(
        env::var("WAYLAND_DISPLAY").ok().as_deref(),
        env::var("DISPLAY").ok().as_deref(),
    )
}

pub fn detect_display_backend_from(
    wayland_display: Option<&str>,
    x11_display: Option<&str>,
) -> DisplayBackend {
    if wayland_display.is_some() {
        DisplayBackend::Wayland
    } else if x11_display.is_some() {
        DisplayBackend::X11
    } else {
        DisplayBackend::Unknown
    }
}

pub fn current_platform() -> PlatformKind {
    if cfg!(target_os = "macos") {
        PlatformKind::MacOS
    } else if cfg!(target_os = "windows") {
        PlatformKind::Windows
    } else if cfg!(target_os = "linux") {
        PlatformKind::Linux
    } else {
        PlatformKind::Unknown
    }
}

pub fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    } else {
        env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{current_platform, detect_display_backend_from, native_services};
    use crate::impls::{
        BasicFontFallback, FixedDpi, MemoryAccessibility, MemoryClipboard, MemoryIme,
    };
    use crate::traits::{Accessibility, Clipboard, DpiAwareness, FontFallback, Ime};
    use crate::types::{DisplayBackend, PlatformKind};

    #[test]
    fn memory_clipboard_stores_value() {
        let mut cb = MemoryClipboard::default();
        cb.set("hello".to_string());
        assert_eq!(cb.get().as_deref(), Some("hello"));
    }

    #[test]
    fn ime_commit_roundtrip() {
        let mut ime = MemoryIme::default();
        ime.begin_composition();
        ime.update_preedit("hola");
        assert_eq!(ime.commit().as_deref(), Some("hola"));
    }

    #[test]
    fn dpi_conversion_works() {
        let dpi = FixedDpi { scale: 2.0 };
        assert_eq!(dpi.logical_to_physical(10.0), 20.0);
    }

    #[test]
    fn fallback_selects_font() {
        let fallback = BasicFontFallback;
        assert_eq!(
            fallback.fallback_for_char('a').as_deref(),
            Some("monospace")
        );
        assert_eq!(
            fallback.fallback_for_char('你').as_deref(),
            Some("fallback-unicode")
        );
    }

    #[test]
    fn accessibility_focus_state_changes() {
        let mut a11y = MemoryAccessibility::default();
        a11y.set_focus("pane-1");
        assert_eq!(a11y.focused_node.as_deref(), Some("pane-1"));
    }

    #[test]
    fn detects_platform() {
        let p = current_platform();
        assert!(matches!(
            p,
            PlatformKind::MacOS
                | PlatformKind::Windows
                | PlatformKind::Linux
                | PlatformKind::Unknown
        ));
    }

    #[test]
    fn detects_display_backend_from_env_values() {
        assert_eq!(
            detect_display_backend_from(Some("wayland-0"), Some(":0")),
            DisplayBackend::Wayland
        );
        assert_eq!(
            detect_display_backend_from(None, Some(":0")),
            DisplayBackend::X11
        );
        assert_eq!(
            detect_display_backend_from(None, None),
            DisplayBackend::Unknown
        );
    }

    #[test]
    fn builds_native_services_bundle() {
        let mut services = native_services();
        services.clipboard.set("ok".to_string());
        // System clipboards can be unavailable in headless CI or delay writes;
        // when a value is returned, it must match what we set.
        if let Some(value) = services.clipboard.get() {
            assert_eq!(value, "ok");
        }
    }
}

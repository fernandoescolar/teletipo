#![allow(unsafe_code)]

use winit::window::Window;

/// On macOS, set the Dock/application icon to the embedded Teletipo logo.
pub fn apply_app_icon(app_icon_png: &[u8]) {
    #[cfg(target_os = "macos")]
    {
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
                dataWithBytes: app_icon_png.as_ptr() as *const core::ffi::c_void
                length: app_icon_png.len()
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

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_icon_png;
    }
}

/// On macOS, make the native title bar take the given RGBA colour so it blends
/// with the rendered content rather than showing the default vibrancy.
pub fn apply_titlebar_color(window: &Window, [r, g, b, a]: [f32; 4]) {
    #[cfg(target_os = "macos")]
    {
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

    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        let _ = (r, g, b, a);
    }
}

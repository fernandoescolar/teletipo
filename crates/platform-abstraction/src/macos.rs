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

// ── VoiceOver accessibility implementation ───────────────────────────────────

/// macOS VoiceOver accessibility adapter.
///
/// Uses `NSAccessibilityPostNotificationWithUserInfo` to announce live-region
/// updates so VoiceOver reads new terminal output aloud without the user
/// having to navigate to it.
///
/// On non-macOS targets this compiles to a zero-cost no-op wrapper so the rest
/// of the codebase can reference it unconditionally.
#[derive(Default)]
pub struct MacOSAccessibility {
    #[cfg(target_os = "macos")]
    previous_zone_count: usize,
}

impl crate::traits::Accessibility for MacOSAccessibility {
    fn announce(&self, text: &str) {
        #[cfg(target_os = "macos")]
        {
            use objc2::class;
            use objc2::msg_send;
            use objc2::runtime::AnyObject;

            if text.is_empty() {
                return;
            }

            // NSAccessibilityPostNotificationWithUserInfo is a plain C function
            // in AppKit — NOT a method on any Objective-C class.
            //
            // Correct string constants (from <AppKit/NSAccessibilityConstants.h>):
            //   NSAccessibilityAnnouncementRequestedNotification = "AXAnnouncementRequested"
            //   NSAccessibilityAnnouncementKey                   = "AXAnnouncement"
            //   NSAccessibilityPriorityKey                       = "AXPriority"
            //   NSAccessibilityPriorityMedium                    = 1100
            unsafe extern "C" {
                fn NSAccessibilityPostNotificationWithUserInfo(
                    element: *mut AnyObject,
                    notification: *mut AnyObject,
                    user_info: *mut AnyObject,
                );
            }

            unsafe {
                let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
                if app.is_null() {
                    return;
                }
                let main_window: *mut AnyObject = msg_send![&*app, mainWindow];
                if main_window.is_null() {
                    return;
                }

                // Null-terminate the string for NSString.
                let mut bytes = text.to_owned();
                bytes.push('\0');
                let ns_str: *mut AnyObject = msg_send![
                    class!(NSString),
                    stringWithUTF8String: bytes.as_ptr() as *const core::ffi::c_char
                ];
                if ns_str.is_null() {
                    return;
                }

                let announcement_key: *mut AnyObject = msg_send![
                    class!(NSString),
                    stringWithUTF8String: c"AXAnnouncement".as_ptr()
                ];
                let priority_key: *mut AnyObject = msg_send![
                    class!(NSString),
                    stringWithUTF8String: c"AXPriority".as_ptr()
                ];
                // NSAccessibilityPriorityMedium = 1100
                let priority_num: *mut AnyObject =
                    msg_send![class!(NSNumber), numberWithInt: 1100i32];

                let keys: [*mut AnyObject; 2] = [announcement_key, priority_key];
                let values: [*mut AnyObject; 2] = [ns_str, priority_num];
                let user_info: *mut AnyObject = msg_send![
                    class!(NSDictionary),
                    dictionaryWithObjects: values.as_ptr()
                    forKeys: keys.as_ptr()
                    count: 2usize
                ];

                let notif_name: *mut AnyObject = msg_send![
                    class!(NSString),
                    stringWithUTF8String: c"AXAnnouncementRequested".as_ptr()
                ];

                NSAccessibilityPostNotificationWithUserInfo(main_window, notif_name, user_info);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = text;
        }
    }

    fn set_focus(&mut self, _node_id: &str) {
        // Focus management is handled by the native accessibility layer; no
        // explicit action needed here beyond what VoiceOver infers from the
        // live-region updates.
    }

    fn update_tree(&mut self, tree: &crate::types::AccessibilityTree) {
        use crate::types::AccessNode;

        // Count completed command zones to detect new ones.
        let zone_count = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, AccessNode::CommandZone { .. }))
            .count();

        if zone_count > self.previous_zone_count {
            // Announce output from the newest zone(s) that weren't present
            // in the previous tree.
            let new_zones = tree
                .nodes
                .iter()
                .filter_map(|n| match n {
                    AccessNode::CommandZone {
                        command_text,
                        output_text,
                        exit_code,
                        ..
                    } => Some((command_text.as_str(), output_text.as_str(), *exit_code)),
                    _ => None,
                })
                .skip(self.previous_zone_count);

            for (cmd, output, code) in new_zones {
                let summary = if let Some(c) = code {
                    if c == 0 {
                        format!("Command completed: {cmd}")
                    } else {
                        format!("Command failed (exit {c}): {cmd}")
                    }
                } else {
                    format!("Running: {cmd}")
                };
                self.announce(&summary);
                if !output.trim().is_empty() {
                    // Announce the first 200 chars of output to avoid overloading VoiceOver.
                    let preview: String = output.chars().take(200).collect();
                    self.announce(preview.trim());
                }
            }
            self.previous_zone_count = zone_count;
        }
    }
}

//! Window management and GPUI view implementation.

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use gpui::{Context, MouseButton, Render, Window, WindowBounds, WindowOptions, canvas, div, fill, point, px, size, prelude::*};
use platform_abstraction::WindowControl;
use render_model::{AppWindowEvent, CellMetrics, RenderConfig, RenderSnapshot, RenderTarget, compute_frame_layout};

use crate::input;
use crate::scene::compose_scene;
use crate::color::background_color;

/// Window control handle for requesting redraws and title updates.
pub struct GpuiWindowControl {
    pub(crate) redraw_requested: Arc<AtomicBool>,
    pub(crate) pending_title: Arc<Mutex<Option<String>>>,
}

impl WindowControl for GpuiWindowControl {
    fn request_redraw(&self) {
        self.redraw_requested.store(true, Ordering::Release);
    }

    fn set_title(&self, title: &str) {
        if let Ok(mut pending_title) = self.pending_title.lock() {
            *pending_title = Some(title.to_string());
        }
        self.request_redraw();
    }

    fn open_url(&self, _url: &str) {}

    fn notify(&self, _title: &str, _body: &str) {}
}

/// GPUI view component for rendering terminal content.
pub struct GpuiView {
    pub(crate) next_snapshot: Box<dyn FnMut() -> RenderSnapshot>,
    pub(crate) on_event: Box<dyn FnMut(AppWindowEvent)>,
    pub(crate) redraw_requested: Arc<AtomicBool>,
    pub(crate) pending_title: Arc<Mutex<Option<String>>>,
    pub(crate) config: RenderConfig,
    pub(crate) last_title: String,
    pub(crate) last_modifiers: platform_abstraction::ModifierKeys,
}

impl Render for GpuiView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.apply_pending_title(window);
        let snapshot = (self.next_snapshot)();

        if snapshot.title_cwd != self.last_title {
            self.last_title = snapshot.title_cwd.clone();
            window.set_window_title(&self.last_title);
        }

        if snapshot.request_exit {
            (self.on_event)(AppWindowEvent::CloseRequested);
        }

        // Check if a redraw was requested, then clear the flag
        let should_request_redraw = self.redraw_requested.swap(false, Ordering::AcqRel);

        let bounds = window.viewport_size();
        let target = RenderTarget::new(f32::from(bounds.width), f32::from(bounds.height));
        let metrics = CellMetrics::new(
            (snapshot.font_size * 0.6).max(1.0),
            (snapshot.font_size * 1.2).max(1.0),
        );
        let layout = compute_frame_layout(&snapshot, target, metrics);
        let scene = compose_scene(&snapshot, &layout, target, metrics);
        let font_size = px(snapshot.font_size.max(1.0));
        let background = background_color(
            snapshot.theme.terminal_bg,
            snapshot.opacity * self.config.opacity,
        );

        // Only request the next animation frame if a redraw was actually needed
        if should_request_redraw {
            window.request_animation_frame();
        }

        let on_key_down = cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, window, cx| {
            let evt = input::convert_key_down(event);
            let current_mods = input::convert_modifiers(&event.keystroke.modifiers);
            if current_mods != this.last_modifiers {
                (this.on_event)(AppWindowEvent::ModifiersChanged(current_mods));
                this.last_modifiers = current_mods;
            }
            (this.on_event)(AppWindowEvent::KeyboardInput(evt));
            this.handle_key_down(event, window, cx);
        });
        let on_key_up = cx.listener(|this: &mut Self, event: &gpui::KeyUpEvent, window, cx| {
            let evt = input::convert_key_up(event);
            let current_mods = input::convert_modifiers(&event.keystroke.modifiers);
            if current_mods != this.last_modifiers {
                (this.on_event)(AppWindowEvent::ModifiersChanged(current_mods));
                this.last_modifiers = current_mods;
            }
            (this.on_event)(AppWindowEvent::KeyboardInput(evt));
            this.handle_key_up(event, window, cx);
        });

        div()
            .size_full()
            .on_key_down(on_key_down)
            .on_key_up(on_key_up)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::handle_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::handle_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::handle_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::handle_mouse_up))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_scroll_wheel(cx.listener(Self::handle_scroll_wheel))
            .child(
                canvas(
                    move |_, _, _| {},
                    move |bounds, _, window, cx| {
                        window.paint_quad(fill(bounds, background));
                        crate::scene::paint_scene(&scene, window, cx, font_size);
                    },
                )
                .size_full(),
            )
    }
}

impl GpuiView {
    fn apply_pending_title(&mut self, window: &mut Window) {
        let Ok(mut pending_title) = self.pending_title.lock() else {
            return;
        };
        let Some(title) = pending_title.take() else {
            return;
        };
        self.last_title = title.clone();
        window.set_window_title(&title);
    }

    fn handle_key_down(&mut self, _: &gpui::KeyDownEvent, _: &mut Window, _: &mut Context<Self>) {
        // Key down handling is attached via on_key_down handler
    }

    fn handle_key_up(&mut self, _: &gpui::KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        // Key up handling is attached via on_key_up handler
    }

    fn handle_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if let Some(evt) = input::convert_mouse_down(event) {
            (self.on_event)(evt);
        }
    }

    fn handle_mouse_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if let Some(evt) = input::convert_mouse_up(event) {
            (self.on_event)(evt);
        }
    }

    fn handle_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        (self.on_event)(input::convert_mouse_move(event));
    }

    fn handle_scroll_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if let Some(evt) = input::convert_scroll_wheel(event) {
            (self.on_event)(evt);
        }
    }
}

/// Create window options from render config.
pub fn window_options(config: &RenderConfig, cx: &gpui::App) -> WindowOptions {
    let (width, height) = config.initial_size.unwrap_or((1280, 720));
    let bounds = if let Some((x, y)) = config.initial_position {
        gpui::Bounds {
            origin: point(px(x as f32), px(y as f32)),
            size: size(px(width as f32), px(height as f32)),
        }
    } else {
        gpui::Bounds::centered(None, size(px(width as f32), px(height as f32)), cx)
    };

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..Default::default()
    }
}

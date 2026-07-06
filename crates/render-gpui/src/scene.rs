//! Scene composition and rendering with clipping support.

use gpui::{App, ContentMask, Pixels, Window, fill};
use render_model::{CellMetrics, RenderCommand, RenderContext, RenderSnapshot, RenderTarget};

use crate::color::background_color;
use crate::text::paint_text;
use crate::geom::rect;

/// Compose a complete render scene from snapshot and layout.
pub fn compose_scene(
    snapshot: &RenderSnapshot,
    layout: &render_model::FrameLayout,
    target: RenderTarget,
    metrics: CellMetrics,
) -> render_model::Scene {
    let mut scene = render_model::build_scene(snapshot, layout, target, metrics);
    let ctx = RenderContext::new(snapshot, layout, target, metrics);

    render_model::overlay::render_resize(&ctx, &mut scene);
    render_model::overlay::render_scroll_indicator(&ctx, &mut scene);
    render_model::components::render_toasts(&ctx, &mut scene);
    render_model::components::render_highlights(&ctx, &mut scene);
    render_model::components::render_selection(&ctx, &mut scene);
    render_model::components::render_cursor(&ctx, &mut scene);
    render_model::components::render_scrollbar(&ctx, &mut scene);
    render_model::components::render_suggestion(&ctx, &mut scene);
    render_model::components::render_tab_bar(&ctx, &mut scene);
    render_model::components::render_search_panel(&ctx, &mut scene);
    render_model::components::render_sticky_command_overlay(&ctx, &mut scene);
    render_model::components::render_command_palette(&ctx, &mut scene);
    render_model::components::render_context_menu(&ctx, &mut scene);
    render_model::components::render_dropdown(&ctx, &mut scene);
    render_model::components::render_settings_overlay(&ctx, &mut scene);
    render_model::components::render_keybindings_overlay(&ctx, &mut scene);

    scene
}

/// Paint a complete scene with all render commands.
pub fn paint_scene(
    scene: &render_model::Scene,
    window: &mut Window,
    cx: &mut App,
    font_size: Pixels,
) {
    for (_, commands) in scene.iter_layers() {
        paint_commands_with_clips(commands, window, cx, font_size);
    }
}

/// Paint commands with clipping support.
fn paint_commands_with_clips(
    commands: &[RenderCommand],
    window: &mut Window,
    cx: &mut App,
    font_size: Pixels,
) {
    paint_commands_recursive(commands, 0, commands.len(), window, cx, font_size);
}

/// Recursively paint commands, handling ClipPush/ClipPop with ContentMask.
fn paint_commands_recursive(
    commands: &[RenderCommand],
    start_index: usize,
    end_index: usize,
    window: &mut Window,
    cx: &mut App,
    font_size: Pixels,
) {
    let mut i = start_index;
    while i < end_index && i < commands.len() {
        match &commands[i] {
            RenderCommand::ClipPush(clip_rect) => {
                let mask = ContentMask {
                    bounds: rect(*clip_rect),
                };

                // Find matching ClipPop by counting depth
                let mut depth = 1;
                let mut clip_end = i + 1;
                while clip_end < end_index && clip_end < commands.len() && depth > 0 {
                    match &commands[clip_end] {
                        RenderCommand::ClipPush(_) => depth += 1,
                        RenderCommand::ClipPop => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        clip_end += 1;
                    }
                }

                // Apply mask and recursively render commands within the clip region
                window.with_content_mask(Some(mask), |w| {
                    paint_commands_recursive(
                        commands,
                        i + 1,
                        clip_end,
                        w,
                        cx,
                        font_size,
                    );
                });

                i = clip_end + 1;
            }
            RenderCommand::ClipPop => {
                // Skip ClipPop at this level (handled by matching ClipPush)
                i += 1;
            }
            RenderCommand::Rect(command) => {
                window.paint_quad(fill(rect(command.rect), background_color(command.color, 1.0)));
                i += 1;
            }
            RenderCommand::Text(command) => {
                paint_text(command, window, cx, font_size);
                i += 1;
            }
            RenderCommand::Emoji(_) => {
                i += 1;
            }
        }
    }
}

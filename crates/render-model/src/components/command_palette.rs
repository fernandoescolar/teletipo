use crate::{RenderContext, Scene, SceneLayer};

const PALETTE_MAX_VISIBLE: usize = 10;
const PALETTE_MAX_CHARS: usize = 48;

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(cp) = &ctx.snapshot.command_palette else {
        return;
    };
    let layout = ctx.layout;
    let visible = cp
        .items
        .len()
        .saturating_sub(cp.scroll_offset)
        .min(PALETTE_MAX_VISIBLE);
    let palette_w = layout.cell_w_px * 50.0;
    let header_h = layout.cell_h_px * 2.2;
    let item_h = layout.cell_h_px * 1.4;
    let cx = layout.width * 0.5;
    let x0 = (cx - palette_w * 0.5).max(0.0);
    let x1 = (cx + palette_w * 0.5).min(layout.width);
    let y0 = layout.tab_bar_h + layout.height * 0.08;

    if let Some(label) = &cp.sub_prompt_label {
        let label_h = layout.cell_h_px * 1.4;
        let input_h = layout.cell_h_px * 1.8;
        let total_h = label_h + input_h;
        let y1 = (y0 + total_h).min(layout.height);
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0 - 2.0,
            y0 - 2.0,
            (x1 + 2.0) - (x0 - 2.0),
            (y1 + 2.0) - (y0 - 2.0),
            [0.35, 0.55, 0.90, 1.0],
        );
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0,
            y0,
            x1 - x0,
            y1 - y0,
            [0.09, 0.11, 0.18, 0.97],
        );
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0,
            y0 + label_h - 1.0,
            x1 - x0,
            1.0,
            [0.30, 0.45, 0.70, 0.80],
        );
        scene.text_to_layer(
            SceneLayer::Overlay,
            x0 + layout.cell_w_px * 0.8,
            y0 + (label_h - layout.cell_h_px) * 0.5,
            truncate_chars(label, PALETTE_MAX_CHARS),
            [0.65, 0.75, 0.95, 1.0],
        );
        scene.text_to_layer(
            SceneLayer::Overlay,
            x0 + layout.cell_w_px * 0.8,
            y0 + label_h + (input_h - layout.cell_h_px) * 0.5,
            truncate_chars(&format!("> {}", cp.query), PALETTE_MAX_CHARS),
            [0.92, 0.94, 0.98, 1.0],
        );
        return;
    }

    let palette_h = header_h + item_h * visible as f32;
    let y1 = (y0 + palette_h).min(layout.height);
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x0 - 2.0,
        y0 - 2.0,
        (x1 + 2.0) - (x0 - 2.0),
        (y1 + 2.0) - (y0 - 2.0),
        [0.35, 0.55, 0.90, 1.0],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x0,
        y0,
        x1 - x0,
        y1 - y0,
        [0.09, 0.11, 0.18, 0.97],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x0,
        y0 + header_h - 1.0,
        x1 - x0,
        1.0,
        [0.30, 0.45, 0.70, 0.80],
    );
    scene.text_to_layer(
        SceneLayer::Overlay,
        x0 + layout.cell_w_px * 0.8,
        y0 + (header_h - layout.cell_h_px) * 0.5,
        truncate_chars(&format!("> {}", cp.query), PALETTE_MAX_CHARS),
        [0.92, 0.94, 0.98, 1.0],
    );

    for i in 0..visible {
        let idx = cp.scroll_offset + i;
        if idx >= cp.items.len() {
            break;
        }
        let row_y = y0 + header_h + i as f32 * item_h;
        if idx == cp.selected {
            scene.rect_to_layer(
                SceneLayer::Overlay,
                x0,
                row_y,
                x1 - x0,
                item_h,
                [0.20, 0.32, 0.58, 0.70],
            );
        }
        scene.text_to_layer(
            SceneLayer::Overlay,
            x0 + layout.cell_w_px * 0.8,
            row_y + (item_h - layout.cell_h_px) * 0.5,
            truncate_chars(&cp.items[idx], PALETTE_MAX_CHARS),
            [0.92, 0.94, 0.98, 1.0],
        );
    }
}

use std::collections::HashMap;

use winit::dpi::PhysicalPosition;
use winit::dpi::PhysicalSize;

use crate::atlas::CachedGlyph;
use crate::batch::CellQuad;
use crate::types::{DamageRegion, PaneLayout, RenderSnapshot};

pub const SCROLLBAR_W_PX: f32 = 10.0;
pub(crate) const EDITOR_PREFIX_COLS: usize = 2;

pub(crate) const VERTEX_BUF_CAPACITY: u64 = 2 << 20;

pub(crate) const SHADER_WGSL: &str = r#"
struct VertIn {
    @location(0) pos:   vec2<f32>,
    @location(1) color: vec4<f32>,
}
struct VertOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       color:    vec4<f32>,
}
@vertex
fn vs_main(v: VertIn) -> VertOut {
    var out: VertOut;
    out.clip_pos = vec4<f32>(v.pos, 0.0, 1.0);
    out.color    = v.color;
    return out;
}
@fragment
fn fs_main(in: VertOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

pub(crate) const TEXT_VERTEX_BUF_CAPACITY: u64 = 4 << 20; // 4 MB – enough for ~131 072 glyphs

pub(crate) const TEXT_SHADER_WGSL: &str = r#"
struct TextVertIn {
    @location(0) pos:   vec2<f32>,
    @location(1) uv:    vec2<f32>,
    @location(2) color: vec4<f32>,
}
struct TextVertOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
    @location(1)       color:    vec4<f32>,
}
@group(0) @binding(0) var t_atlas: texture_2d<f32>;
@group(0) @binding(1) var s_atlas: sampler;
@vertex
fn vs_text(v: TextVertIn) -> TextVertOut {
    var out: TextVertOut;
    out.clip_pos = vec4<f32>(v.pos, 0.0, 1.0);
    out.uv       = v.uv;
    out.color    = v.color;
    return out;
}
@fragment
fn fs_text(in: TextVertOut) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_atlas, s_atlas, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// # Safety
/// `f32` has no invalid bit patterns.
pub(crate) fn floats_as_bytes(v: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 4) }
}

pub(crate) fn quad_verts(x0: f32, y_bottom: f32, x1: f32, y_top: f32, color: [f32; 4]) -> [f32; 36] {
    let [r, g, b, a] = color;
    [
        x0, y_top,    r, g, b, a,
        x1, y_top,    r, g, b, a,
        x1, y_bottom, r, g, b, a,
        x0, y_top,    r, g, b, a,
        x1, y_bottom, r, g, b, a,
        x0, y_bottom, r, g, b, a,
    ]
}

pub(crate) fn build_panel_vertices(
    size: PhysicalSize<u32>,
    snapshot: &RenderSnapshot,
    // Vertical pixel offset from the top of the terminal pane at which row 0 starts.
    // Pass 0.0 for top-aligned; use (pane_h - rows*cell_h) for bottom-aligned rendering.
    term_top_offset_px: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    // Horizontal padding in physical pixels (insets the text grid from the left edge).
    pad_h: f32,
    // Vertical padding in physical pixels (insets the text grid from the pane top).
    pad_v: f32,
) -> Vec<f32> {
    let theme = &snapshot.theme;
    let tab_bar_h = if !snapshot.tab_labels.is_empty() { cell_h_px } else { 0.0 };
    let tab_bar_frac = if size.height > 0 { tab_bar_h / size.height as f32 } else { 0.0 };
    let available_frac = 1.0 - tab_bar_frac;
    let term_top = 1.0 - 2.0 * tab_bar_frac;   // = 1.0 when no tab bar
    let term_bottom = term_top - 2.0 * snapshot.split_ratio * available_frac;
    let edit_top_ndc = term_bottom;
    let edit_bottom = -1.0f32;
    let split_y = term_bottom;

    let sep_h = if size.height > 0 {
        (2.0_f32 / size.height as f32).max(0.004)
    } else {
        0.004
    };
    let sep_half = sep_h * 0.5;

    let sep_color = if snapshot.editor_focused { theme.separator_focused } else { theme.separator };

    let mut verts = Vec::new();
    verts.extend_from_slice(&quad_verts(-1.0, split_y + sep_half, 1.0, term_top,    theme.terminal_bg));
    verts.extend_from_slice(&quad_verts(-1.0, edit_bottom,        1.0, split_y - sep_half, theme.editor_bg));
    verts.extend_from_slice(&quad_verts(-1.0, split_y - sep_half, 1.0, split_y + sep_half, sep_color));

    // Tab bar geometry (rendered whenever there are tabs).
    if tab_bar_h > 0.0 {
        // Tab bar colours derived from the active theme.
        let add_c = |a: [f32; 4], d: f32| -> [f32; 4] {
            [(a[0]+d).clamp(0.0,1.0), (a[1]+d).clamp(0.0,1.0), (a[2]+d).clamp(0.0,1.0), a[3]]
        };
        let mix_c = |a: [f32; 4], b: [f32; 4], t: f32| -> [f32; 4] {
            [a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t, a[2]+(b[2]-a[2])*t, a[3]+(b[3]-a[3])*t]
        };
        let tab_bar_bg   = add_c(theme.terminal_bg, 0.05);
        let tab_inactive = add_c(theme.terminal_bg, 0.02);
        let tab_active   = mix_c(add_c(theme.terminal_bg, 0.05), theme.separator_focused, 0.22);
        let add_btn_bg   = { let [r,g,b,_] = theme.terminal_bg; [(r+0.05).clamp(0.0,1.0),(g+0.10).clamp(0.0,1.0),(b+0.03).clamp(0.0,1.0),0.90_f32] };
        let drag_line    = theme.separator_focused;
        // tab_bar occupies the topmost strip: from term_top (just below the bar) to 1.0.
        let tab_bar_bottom_ndc = term_top;
        verts.extend_from_slice(&quad_verts(-1.0, tab_bar_bottom_ndc, 1.0, 1.0, tab_bar_bg));
        let n = snapshot.tab_labels.len();
        if n > 0 && size.width > 0 {
            let px_x = 2.0 / size.width as f32;
            let px_y = if size.height > 0 { 2.0 / size.height as f32 } else { 0.0 };
            // Reserve the rightmost (2 × cell_w) pixels for the "+" button.
            let add_btn_w_px  = cell_w_px * 2.0;
            let add_btn_w_ndc = add_btn_w_px * px_x;
            let tab_area_right_ndc = 1.0 - add_btn_w_ndc;
            let tab_area_ndc = tab_area_right_ndc + 1.0; // = 2.0 - add_btn_w_ndc
            let tab_w_ndc = tab_area_ndc / n as f32;
            let gap_ndc   = px_x; // 1-pixel inter-tab gap
            let px_inset  = px_y; // 1-pixel vertical inset so tabs don't touch the strip edge
            for i in 0..n {
                let x0 = -1.0 + i as f32 * tab_w_ndc + gap_ndc;
                let x1 = -1.0 + (i + 1) as f32 * tab_w_ndc - gap_ndc;
                let color = if i == snapshot.active_tab { tab_active } else { tab_inactive };
                verts.extend_from_slice(&quad_verts(
                    x0, tab_bar_bottom_ndc + px_inset,
                    x1, 1.0 - px_inset,
                    color,
                ));
            }
            // "+" button background quad.
            let add_x0 = tab_area_right_ndc + gap_ndc;
            let add_x1 = 1.0 - gap_ndc;
            verts.extend_from_slice(&quad_verts(
                add_x0, tab_bar_bottom_ndc + px_inset,
                add_x1, 1.0 - px_inset,
                add_btn_bg,
            ));
            // Drag-reorder insertion indicator: a 2-pixel-wide vertical cyan line.
            if let Some(insert_before) = snapshot.tab_drag_insert_before {
                let ib = insert_before.min(n);
                let x_ins  = -1.0 + ib as f32 * tab_w_ndc;
                let line_w = px_x * 2.0;
                verts.extend_from_slice(&quad_verts(
                    x_ins - line_w, tab_bar_bottom_ndc,
                    x_ins + line_w, 1.0,
                    drag_line,
                ));
            }
        }
    }

    // Context menu overlay — always rendered last so it paints over everything.
    if let Some(ref menu) = snapshot.tab_context_menu
        && size.width > 0 && size.height > 0 && cell_w_px > 0.0 && cell_h_px > 0.0 {
            const MENU_BG:     [f32; 4] = [0.12, 0.15, 0.20, 0.97];
            const MENU_BORDER: [f32; 4] = [0.25, 0.32, 0.42, 1.0];
            const MENU_HOVER:  [f32; 4] = [0.20, 0.28, 0.44, 1.0];
            const ITEM_COUNT: usize = 4;
            let menu_item_h = cell_h_px * 1.15;
            let menu_w      = cell_w_px * 13.0;
            let menu_h      = menu_item_h * ITEM_COUNT as f32;
            let px_x = 2.0 / size.width  as f32;
            let px_y = 2.0 / size.height as f32;
            // Clamp menu so it stays inside the window.
            let mx = menu.x_px.min(size.width  as f32 - menu_w).max(0.0);
            let my = menu.y_px.min(size.height as f32 - menu_h).max(0.0);
            let x0_ndc  = mx           * px_x - 1.0;
            let x1_ndc  = (mx + menu_w) * px_x - 1.0;
            let y_top_ndc = 1.0 - my            * px_y;
            let y_bot_ndc = 1.0 - (my + menu_h) * px_y;
            // 1-pixel border quad behind the menu.
            verts.extend_from_slice(&quad_verts(
                x0_ndc - px_x, y_bot_ndc - px_y,
                x1_ndc + px_x, y_top_ndc + px_y,
                MENU_BORDER,
            ));
            // Menu background.
            verts.extend_from_slice(&quad_verts(x0_ndc, y_bot_ndc, x1_ndc, y_top_ndc, MENU_BG));
            // Per-item hover highlight.
            for item_idx in 0..ITEM_COUNT {
                if menu.hovered_item == Some(item_idx) {
                    let iy_top = 1.0 - (my + item_idx       as f32 * menu_item_h) * px_y;
                    let iy_bot = 1.0 - (my + (item_idx + 1) as f32 * menu_item_h) * px_y;
                    verts.extend_from_slice(&quad_verts(x0_ndc, iy_bot, x1_ndc, iy_top, MENU_HOVER));
                }
            }
    }

    if size.width > 0 && size.height > 0 && cell_w_px > 0.0 && cell_h_px > 0.0 {
        let px_x = 2.0 / size.width as f32;
        let px_y = 2.0 / size.height as f32;
        let pane_top_px = term_top_offset_px;
        let mut char_idx = 0usize;
        let mut row = 0usize;
        let mut col = 0usize;
        for ch in snapshot.terminal_text.chars() {
            if ch == '\n' {
                row += 1;
                col = 0;
                char_idx += 1;
                continue;
            }
            if let Some(Some([r, g, b])) = snapshot.terminal_bg_colors.get(char_idx) {
                let x0 = (pad_h + col as f32 * cell_w_px) * px_x - 1.0;
                let x1 = (pad_h + (col + 1) as f32 * cell_w_px) * px_x - 1.0;
                let y1 = 1.0 - (pane_top_px + pad_v + row as f32 * cell_h_px) * px_y;
                let y0 = 1.0 - (pane_top_px + pad_v + (row + 1) as f32 * cell_h_px) * px_y;
                verts.extend_from_slice(&quad_verts(x0, y0, x1, y1, [*r, *g, *b, 1.0]));
            }
            char_idx += 1;
            col += 1;
        }
    }

    if let Some((raw_sr, raw_sc, raw_er, raw_ec)) = snapshot.selection
        && size.width > 0 && size.height > 0 && cell_w_px > 0.0 && cell_h_px > 0.0 {
            let (sel_sr, sel_sc, sel_er, sel_ec) =
                if (raw_sr, raw_sc) <= (raw_er, raw_ec) {
                    (raw_sr, raw_sc, raw_er, raw_ec)
                } else {
                    (raw_er, raw_ec, raw_sr, raw_sc)
                };
            let px_x = 2.0 / size.width as f32;
            let px_y = 2.0 / size.height as f32;
            let pane_top_px = term_top_offset_px;
            let max_cols = (size.width as f32 / cell_w_px).ceil() as usize + 1;
            let sel_color = [0.20_f32, 0.48, 1.0, 0.38];
            for row in sel_sr..=sel_er {
                let from_col = if row == sel_sr { sel_sc } else { 0 };
                let to_col   = if row == sel_er { sel_ec + 1 } else { max_cols };
                if from_col >= to_col { continue; }
                let x0 = (pad_h + from_col as f32 * cell_w_px) * px_x - 1.0;
                let x1 = (pad_h + to_col   as f32 * cell_w_px) * px_x - 1.0;
                let y1 = 1.0 - (pane_top_px + pad_v + row       as f32 * cell_h_px) * px_y;
                let y0 = 1.0 - (pane_top_px + pad_v + (row + 1) as f32 * cell_h_px) * px_y;
                verts.extend_from_slice(&quad_verts(x0, y0, x1, y1, sel_color));
            }
    }

    if size.width > 0 && snapshot.scrollback_lines > 0 {
        let sb_w_ndc = 2.0 * SCROLLBAR_W_PX / size.width as f32;
        let sb_left = 1.0 - sb_w_ndc;
        let track_color = theme.separator;
        verts.extend_from_slice(&quad_verts(sb_left, term_bottom, 1.0, term_top, track_color));
        let track_h_ndc = term_top - term_bottom;
        let term_h_px = snapshot.split_ratio * size.height as f32;
        let thumb_fraction = if cell_h_px > 0.0 {
            let visible_rows = (term_h_px / cell_h_px).floor();
            let total_rows = visible_rows + snapshot.scrollback_lines as f32;
            (visible_rows / total_rows).clamp(0.05, 1.0)
        } else {
            1.0
        };
        let thumb_h_ndc = thumb_fraction * track_h_ndc;
        let scroll_pos = (snapshot.scroll_offset as f32
            / snapshot.scrollback_lines as f32).clamp(0.0, 1.0);
        let scrollable_h = track_h_ndc - thumb_h_ndc;
        let thumb_bottom_ndc = term_bottom + scroll_pos * scrollable_h;
        let thumb_top_ndc = thumb_bottom_ndc + thumb_h_ndc;
        let [r, g, b, _] = theme.separator_focused;
        let thumb_color = [r, g, b, 0.85_f32];
        verts.extend_from_slice(&quad_verts(sb_left, thumb_bottom_ndc, 1.0, thumb_top_ndc, thumb_color));
    }

    if size.width > 0 && size.height > 0 && cell_h_px > 0.0 {
        let edit_pane_h_px = (1.0 - snapshot.split_ratio) * size.height as f32;
        let visible_editor_rows = (edit_pane_h_px / cell_h_px).floor() as usize;
        if snapshot.editor_line_count > visible_editor_rows {
            let sb_w_ndc = 2.0 * SCROLLBAR_W_PX / size.width as f32;
            let sb_left = 1.0 - sb_w_ndc;
            let track_color = theme.separator;
            verts.extend_from_slice(&quad_verts(sb_left, edit_bottom, 1.0, edit_top_ndc, track_color));
            let track_h_ndc = edit_top_ndc - edit_bottom;
            let thumb_fraction = (visible_editor_rows as f32 / snapshot.editor_line_count as f32).clamp(0.05, 1.0);
            let thumb_h_ndc = thumb_fraction * track_h_ndc;
            let scrollable_range = snapshot.editor_line_count.saturating_sub(visible_editor_rows) as f32;
            let scroll_pos = if scrollable_range > 0.0 {
                (snapshot.editor_scroll_offset as f32 / scrollable_range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let scrollable_h = track_h_ndc - thumb_h_ndc;
            let thumb_top_ndc = edit_top_ndc - scroll_pos * scrollable_h;
            let thumb_bottom_ndc = thumb_top_ndc - thumb_h_ndc;
            let [r, g, b, _] = theme.separator_focused;
            let thumb_color = [r, g, b, 0.85_f32];
            verts.extend_from_slice(&quad_verts(sb_left, thumb_bottom_ndc, 1.0, thumb_top_ndc, thumb_color));
        }
    }

    if let Some(ref overlay_text) = snapshot.resize_overlay
        && size.width > 0 && size.height > 0 && cell_w_px > 0.0 && cell_h_px > 0.0 {
            let n_chars = overlay_text.chars().count() as f32;
            let padding_px = cell_w_px * 2.0;
            let box_w_px = n_chars * cell_w_px + padding_px * 2.0;
            let box_h_px = cell_h_px * 2.0;
            let win_w = size.width as f32;
            let win_h = size.height as f32;
            let cx_px = win_w / 2.0;
            let cy_px = snapshot.split_ratio * win_h / 2.0;
            let px_x = 2.0 / win_w;
            let px_y = 2.0 / win_h;
            let x0 = (cx_px - box_w_px / 2.0) * px_x - 1.0;
            let x1 = (cx_px + box_w_px / 2.0) * px_x - 1.0;
            let y0 = 1.0 - (cy_px + box_h_px / 2.0) * px_y;
            let y1 = 1.0 - (cy_px - box_h_px / 2.0) * px_y;
            verts.extend_from_slice(&quad_verts(x0, y0, x1, y1, [0.05, 0.08, 0.12, 0.90]));
    }

    let edit_top_px = (1.0 - edit_top_ndc) / 2.0 * size.height as f32 + 2.0;
    let editor_scroll = snapshot.editor_scroll_offset;

    // Editor selection highlight.
    if let Some((sel_start, sel_end)) = snapshot.editor_selection
        && size.width > 0 && size.height > 0 && cell_w_px > 0.0 && cell_h_px > 0.0 {
            let (s_start, s_end) = if sel_start <= sel_end { (sel_start, sel_end) } else { (sel_end, sel_start) };
            let px_x = 2.0 / size.width as f32;
            let px_y = 2.0 / size.height as f32;
            // Convert a byte offset in editor_text to a (row, visual_col) pair.
            let to_visual = |offset: usize| -> (usize, usize) {
                let clamped = offset.min(snapshot.editor_text.len());
                let before = &snapshot.editor_text[..clamped];
                let row = before.chars().filter(|&c| c == '\n').count();
                let col_in_text = match before.rfind('\n') {
                    Some(pos) => before[pos + 1..].chars().count(),
                    None => before.chars().count(),
                };
                (row, col_in_text + if row == 0 { EDITOR_PREFIX_COLS } else { 0 })
            };
            let (start_row, start_col) = to_visual(s_start);
            let (end_row, end_col)   = to_visual(s_end);
            let sel_color = [0.20_f32, 0.48, 1.0, 0.38];
            let max_cols = (size.width as f32 / cell_w_px).ceil() as usize + 1;
            for row in start_row..=end_row {
                let from_col = if row == start_row { start_col } else if row == 0 { EDITOR_PREFIX_COLS } else { 0 };
                let to_col   = if row == end_row   { end_col }   else { max_cols };
                if from_col >= to_col { continue; }
                if row < editor_scroll { continue; }
                let visible_row = row - editor_scroll;
                let x0 = (pad_h + from_col as f32 * cell_w_px) * px_x - 1.0;
                let x1 = (pad_h + to_col   as f32 * cell_w_px) * px_x - 1.0;
                let y_top_px = edit_top_px + pad_v + visible_row as f32 * cell_h_px;
                let y1 = 1.0 - y_top_px * px_y;
                let y0 = 1.0 - (y_top_px + cell_h_px) * px_y;
                verts.extend_from_slice(&quad_verts(x0, y0, x1, y1, sel_color));
            }
    }

    verts.extend_from_slice(&editor_caret_verts(
        &snapshot.editor_text,
        snapshot.editor_cursor_offset,
        edit_top_px,
        cell_w_px,
        cell_h_px,
        size,
        theme.cursor,
        editor_scroll,
        pad_h,
        pad_v,
    ));

    verts
}

/// Returns background geometry for the settings overlay only.
/// Must be drawn **after** all terminal/editor text so the panel sits on top.
/// Returns an empty `Vec` when there is no active overlay.
pub(crate) fn build_settings_overlay_bg_verts(
    size: PhysicalSize<u32>,
    snapshot: &RenderSnapshot,
    cell_w_px: f32,
    cell_h_px: f32,
) -> Vec<f32> {
    let mut verts = Vec::new();
    let Some(ref overlay) = snapshot.settings_overlay else { return verts; };
    if size.width == 0 || size.height == 0 || cell_w_px == 0.0 || cell_h_px == 0.0 {
        return verts;
    }

    // Settings overlay colours derived from the active theme.
    let add_c = |a: [f32; 4], d: f32| -> [f32; 4] {
        [(a[0]+d).clamp(0.0,1.0), (a[1]+d).clamp(0.0,1.0), (a[2]+d).clamp(0.0,1.0), a[3]]
    };
    let mix_c = |a: [f32; 4], b: [f32; 4], t: f32| -> [f32; 4] {
        [a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t, a[2]+(b[2]-a[2])*t, a[3]+(b[3]-a[3])*t]
    };
    let theme      = &snapshot.theme;
    let dim        = [0.0_f32, 0.0, 0.0, 0.68];
    let ov_bg      = add_c(theme.terminal_bg, 0.01);
    let ov_border  = theme.separator_focused;
    let ov_title   = add_c(theme.terminal_bg, -0.01);
    let ov_section = add_c(theme.terminal_bg, 0.04);
    let ov_select  = mix_c(add_c(theme.terminal_bg, 0.08), theme.separator_focused, 0.20);
    let ov_edit    = mix_c(add_c(theme.terminal_bg, 0.08), theme.separator_focused, 0.28);

    let px_x = 2.0 / size.width  as f32;
    let px_y = 2.0 / size.height as f32;
    let win_w = size.width  as f32;
    let win_h = size.height as f32;

    // Full-screen dim.
    verts.extend_from_slice(&quad_verts(-1.0, -1.0, 1.0, 1.0, dim));

    let title_h  = cell_h_px * 1.8;
    let row_h    = cell_h_px * 1.3;
    let footer_h = cell_h_px * 1.5;
    let edit_h   = if overlay.editing.is_some() { cell_h_px * 1.4 } else { 0.0 };
    let n_items  = overlay.items.len() as f32;
    let panel_h  = title_h + n_items * row_h + edit_h + footer_h;
    let panel_w  = (cell_w_px * 54.0).min(win_w * 0.88).max(cell_w_px * 30.0);

    let panel_x0 = (win_w - panel_w) / 2.0;
    let panel_y0 = (win_h - panel_h) / 2.0;

    let x0 = panel_x0 * px_x - 1.0;
    let x1 = (panel_x0 + panel_w) * px_x - 1.0;
    let y_top = 1.0 - panel_y0 * px_y;
    let y_bot = 1.0 - (panel_y0 + panel_h) * px_y;

    // 2-pixel border.
    verts.extend_from_slice(&quad_verts(
        x0 - 2.0 * px_x, y_bot - 2.0 * px_y,
        x1 + 2.0 * px_x, y_top + 2.0 * px_y,
        ov_border,
    ));
    // Main background.
    verts.extend_from_slice(&quad_verts(x0, y_bot, x1, y_top, ov_bg));
    // Title bar.
    let title_top_ndc = y_top;
    let title_bot_ndc = 1.0 - (panel_y0 + title_h) * px_y;
    verts.extend_from_slice(&quad_verts(x0, title_bot_ndc, x1, title_top_ndc, ov_title));

    // Per-item row highlights.
    let mut editable_idx = 0usize;
    for (i, item) in overlay.items.iter().enumerate() {
        let item_y_top_px = panel_y0 + title_h + i as f32 * row_h;
        let iy_top = 1.0 - item_y_top_px * px_y;
        let iy_bot = 1.0 - (item_y_top_px + row_h) * px_y;
        if item.is_header {
            verts.extend_from_slice(&quad_verts(x0, iy_bot, x1, iy_top, ov_section));
        } else {
            if editable_idx == overlay.cursor {
                let color = if overlay.editing.is_some() { ov_edit } else { ov_select };
                verts.extend_from_slice(&quad_verts(x0, iy_bot, x1, iy_top, color));
            }
            editable_idx += 1;
        }
    }

    // Edit-mode input row.
    if overlay.editing.is_some() {
        let edit_y_top_px = panel_y0 + title_h + n_items * row_h;
        let ey_top = 1.0 - edit_y_top_px * px_y;
        let ey_bot = 1.0 - (edit_y_top_px + edit_h) * px_y;
        verts.extend_from_slice(&quad_verts(x0, ey_bot, x1, ey_top, ov_edit));
    }

    verts
}

/// Returns background geometry for the suggestion-cycling dropdown that floats
/// just above the editor line.  Each candidate is rendered as one row; the
/// selected item is highlighted.  Returns an empty `Vec` when there is no
/// active dropdown.
pub(crate) fn build_suggestion_dropdown_bg_verts(
    size: PhysicalSize<u32>,
    snapshot: &RenderSnapshot,
    cell_w_px: f32,
    cell_h_px: f32,
    pad_h: f32,
) -> Vec<f32> {
    let Some(ref dd) = snapshot.suggestion_dropdown else { return Vec::new() };
    if dd.items.is_empty() || size.width == 0 || size.height == 0 { return Vec::new(); }

    let theme       = &snapshot.theme;
    let add_c = |a: [f32; 4], d: f32| -> [f32; 4] {
        [(a[0]+d).clamp(0.0,1.0), (a[1]+d).clamp(0.0,1.0), (a[2]+d).clamp(0.0,1.0), a[3]]
    };
    let mix_c = |a: [f32; 4], b: [f32; 4], t: f32| -> [f32; 4] {
        [a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t, a[2]+(b[2]-a[2])*t, 1.0]
    };
    let dd_bg      = add_c(theme.terminal_bg, 0.06);
    let dd_border  = theme.separator_focused;
    let dd_select  = mix_c(add_c(theme.terminal_bg, 0.10), theme.separator_focused, 0.25);

    let px_x = 2.0 / size.width  as f32;
    let px_y = 2.0 / size.height as f32;
    let win_h = size.height as f32;

    let tab_bar_h    = if !snapshot.tab_labels.is_empty() { cell_h_px } else { 0.0 };
    let available_h  = win_h - tab_bar_h;
    let edit_top_px  = (tab_bar_h + snapshot.split_ratio * available_h + 2.0).round();

    let row_h      = cell_h_px * 1.2;
    let n_visible  = dd.items.len().saturating_sub(dd.scroll_offset).min(8);
    let visible_end = dd.scroll_offset + n_visible;
    let visible_selected = dd.selected.saturating_sub(dd.scroll_offset);
    let max_chars  = dd.items[dd.scroll_offset..visible_end].iter().map(|s| s.chars().count()).max().unwrap_or(10);
    let panel_w    = (max_chars as f32 + 4.0) * cell_w_px;
    let panel_h    = n_visible as f32 * row_h;

    // Anchor the panel's left edge at the text-grid start and its bottom edge
    // flush with the top of the editor area.
    let panel_x0_px = pad_h;
    let panel_y_bot_px = edit_top_px;            // bottom of dropdown = top of editor
    let panel_y_top_px = edit_top_px - panel_h;  // top of dropdown (above editor)

    let x0    = panel_x0_px * px_x - 1.0;
    let x1    = (panel_x0_px + panel_w) * px_x - 1.0;
    let y_bot = 1.0 - panel_y_bot_px * px_y;
    let y_top = 1.0 - panel_y_top_px * px_y;

    let mut verts = Vec::new();

    // 1-pixel border.
    verts.extend_from_slice(&quad_verts(
        x0 - px_x, y_bot - px_y, x1 + px_x, y_top + px_y, dd_border,
    ));
    // Background.
    verts.extend_from_slice(&quad_verts(x0, y_bot, x1, y_top, dd_bg));

    // Per-row highlight for the selected item.
    for i in 0..n_visible {
        let row_px_top = panel_y_top_px + i as f32 * row_h;
        let ry_top = 1.0 - row_px_top * px_y;
        let ry_bot = 1.0 - (row_px_top + row_h) * px_y;
        if i == visible_selected {
            verts.extend_from_slice(&quad_verts(x0, ry_bot, x1, ry_top, dd_select));
        }
    }

    verts
}

pub(crate) fn editor_caret_verts(
    editor_text: &str,
    cursor_offset: usize,
    edit_top_px: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    size: PhysicalSize<u32>,
    color: [f32; 4],
    scroll_offset: usize,
    pad_h: f32,
    pad_v: f32,
) -> [f32; 36] {
    let win_w = size.width as f32;
    let win_h = size.height as f32;
    if win_w == 0.0 || win_h == 0.0 || cell_w_px == 0.0 || cell_h_px == 0.0 {
        return [0.0; 36];
    }
    let clamped = cursor_offset.min(editor_text.len());
    let before = &editor_text[..clamped];
    let row = before.chars().filter(|&c| c == '\n').count();
    // Caret is above the visible scroll window — don't draw it.
    if row < scroll_offset {
        return [0.0; 36];
    }
    let visible_row = row - scroll_offset;
    let col_in_editor = match before.rfind('\n') {
        Some(pos) => before[pos + 1..].chars().count(),
        None => before.chars().count(),
    };
    let col = col_in_editor + if row == 0 { EDITOR_PREFIX_COLS } else { 0 };
    let px_x = 2.0 / win_w;
    let px_y = 2.0 / win_h;
    let gx0 = pad_h + col as f32 * cell_w_px;
    let gy0 = edit_top_px + pad_v + visible_row as f32 * cell_h_px;
    let x0 = gx0 * px_x - 1.0;
    let x1 = (gx0 + 2.0) * px_x - 1.0;
    let y1 = 1.0 - gy0 * px_y;
    let y0 = 1.0 - (gy0 + cell_h_px) * px_y;
    quad_verts(x0, y0, x1, y1, color)
}

pub(crate) fn add_text_verts(
    text: &str,
    pane_top_px: f32,
    x_start_px: f32,
    default_color: [f32; 4],
    fg_colors: &[Option<[f32; 3]>],
    glyph_cache: &HashMap<char, CachedGlyph>,
    cell_w_px: f32,
    cell_h_px: f32,
    window_size: PhysicalSize<u32>,
    verts: &mut Vec<f32>,
    // Number of text rows to skip before rendering (editor scroll).
    skip_rows: usize,
) {
    let win_w = window_size.width as f32;
    let win_h = window_size.height as f32;
    if win_w == 0.0 || win_h == 0.0 {
        return;
    }
    let px_x = 2.0 / win_w;
    let px_y = 2.0 / win_h;
    let mut row = 0usize;
    let mut col = 0usize;
    for (char_idx, ch) in text.chars().enumerate() {
        if ch == '\n' {
            row += 1;
            col = 0;
            continue;
        }
        if row < skip_rows {
            col += 1;
            continue;
        }
        let visible_row = row - skip_rows;
        let [r, g, b, a] = match fg_colors.get(char_idx).copied().flatten() {
            Some([cr, cg, cb]) => [cr, cg, cb, default_color[3]],
            None => default_color,
        };
        if let Some(glyph) = glyph_cache.get(&ch)
            && glyph.width_px > 0.0 && glyph.height_px > 0.0 {
                let gx0 = x_start_px + col as f32 * cell_w_px + glyph.offset_x_px;
                let gy0 = pane_top_px + visible_row as f32 * cell_h_px + glyph.offset_y_px;
                let x0 = gx0 * px_x - 1.0;
                let x1 = (gx0 + glyph.width_px) * px_x - 1.0;
                let y1 = 1.0 - gy0 * px_y;
                let y0 = 1.0 - (gy0 + glyph.height_px) * px_y;
                let (u0, v0, u1, v1) = (glyph.u0, glyph.v0, glyph.u1, glyph.v1);
                verts.extend_from_slice(&[x0, y1, u0, v0, r, g, b, a]);
                verts.extend_from_slice(&[x1, y1, u1, v0, r, g, b, a]);
                verts.extend_from_slice(&[x1, y0, u1, v1, r, g, b, a]);
                verts.extend_from_slice(&[x0, y1, u0, v0, r, g, b, a]);
                verts.extend_from_slice(&[x1, y0, u1, v1, r, g, b, a]);
                verts.extend_from_slice(&[x0, y0, u0, v1, r, g, b, a]);
            }
        col += 1;
    }
}

pub fn snapshot_to_cell_quads(
    snapshot: &RenderSnapshot,
    damage: &DamageRegion,
    cols_hint: usize,
) -> Vec<CellQuad> {
    snapshot_to_cell_quads_in_bounds(&snapshot.terminal_text, damage, cols_hint, (1.0, -1.0))
}

pub fn snapshot_to_cell_quads_in_bounds(
    text: &str,
    damage: &DamageRegion,
    cols_hint: usize,
    y_bounds: (f32, f32),
) -> Vec<CellQuad> {
    let lines: Vec<&str> = text.lines().collect();
    let cols = cols_hint.max(1);
    let cell_w = 2.0f32 / cols as f32;
    let rows = lines.len().max(1);
    let cell_h = (y_bounds.0 - y_bounds.1) / rows as f32;

    let mut quads = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        if !damage.full_redraw && !damage.dirty_rows.contains(&row) {
            continue;
        }

        for (col, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }

            let x = -1.0 + col as f32 * cell_w;
            let y = y_bounds.0 - (row as f32 + 1.0) * cell_h;
            quads.push(CellQuad {
                x,
                y,
                w: cell_w,
                h: cell_h,
                glyph: ch,
            });
        }
    }
    quads
}

pub(crate) fn snapshot_to_text_quads_in_bounds(
    text: &str,
    cols_hint: usize,
    y_bounds: (f32, f32),
) -> Vec<CellQuad> {
    let full_redraw = DamageRegion {
        full_redraw: true,
        dirty_rows: Vec::new(),
    };
    snapshot_to_cell_quads_in_bounds(text, &full_redraw, cols_hint, y_bounds)
}

pub fn snapshot_to_ime_area(
    snapshot: &RenderSnapshot,
    window_size: PhysicalSize<u32>,
) -> (PhysicalPosition<f64>, PhysicalSize<f64>) {
    let layout = PaneLayout { split_ratio: snapshot.split_ratio };
    let (edit_y_top, edit_y_bottom) = layout.editor_bounds();

    let cols: usize = 80;
    let text = &snapshot.editor_text;
    let clamped = snapshot.editor_cursor_offset.min(text.len());
    let before = &text[..clamped];
    let row = before.chars().filter(|&c| c == '\n').count();
    let col = before.rfind('\n').map(|i| clamped - i - 1).unwrap_or(clamped);

    let lines = text.lines().count().max(1) as f64;
    let cell_w_ndc = 2.0_f64 / cols as f64;
    let cell_h_ndc = (edit_y_top - edit_y_bottom) as f64 / lines;

    let ndc_x = -1.0 + col as f64 * cell_w_ndc;
    let ndc_y = edit_y_top as f64 - (row as f64 + 1.0) * cell_h_ndc;

    let w = window_size.width as f64;
    let h = window_size.height as f64;
    let screen_x = (ndc_x + 1.0) / 2.0 * w;
    let screen_y = (1.0 - ndc_y) / 2.0 * h;
    let char_w = cell_w_ndc / 2.0 * w;
    let char_h = cell_h_ndc / 2.0 * h;

    (
        PhysicalPosition::new(screen_x, screen_y),
        PhysicalSize::new(char_w.max(1.0), char_h.max(1.0)),
    )
}

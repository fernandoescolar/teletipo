use std::time::Instant;

use crate::batch::{Batch, BatchBuilder, FramePacer};
use crate::geometry::{
    snapshot_to_cell_quads_in_bounds, snapshot_to_text_quads_in_bounds,
};
use crate::types::{
    DamageRegion, PaneLayout, PipelineStage, RenderConfig, RenderSnapshot, RenderStats,
};

pub trait Renderer {
    fn ingest_damage(&mut self, damage: DamageRegion);
    fn render(&mut self, snapshot: &RenderSnapshot);
}

pub struct WgpuRenderer {
    config: RenderConfig,
    pending_damage: DamageRegion,
    pacer: FramePacer,
    frames: usize,
    stats: RenderStats,
}

impl WgpuRenderer {
    pub fn new(config: RenderConfig) -> Self {
        Self {
            pacer: FramePacer::new(config.target_fps),
            config,
            pending_damage: DamageRegion {
                full_redraw: true,
                dirty_rows: Vec::new(),
            },
            frames: 0,
            stats: RenderStats::default(),
        }
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn config(&self) -> &RenderConfig {
        &self.config
    }

    pub fn atlas_len(&self) -> usize {
        0
    }

    pub fn stats(&self) -> &RenderStats {
        &self.stats
    }

    fn build_batches(&self, snapshot: &RenderSnapshot) -> Vec<Batch> {
        let layout = PaneLayout { split_ratio: snapshot.split_ratio };
        let terminal_quads = snapshot_to_cell_quads_in_bounds(
            &snapshot.terminal_text,
            &self.pending_damage,
            80,
            layout.terminal_bounds(),
        );
        let editor_quads = snapshot_to_text_quads_in_bounds(
            &snapshot.editor_text,
            80,
            layout.editor_bounds(),
        );
        let mut builder = BatchBuilder::default();
        builder.add(Batch {
            stage: PipelineStage::Background,
            vertex_count: 12,
            index_count: 12,
        });
        builder.add(Batch {
            stage: PipelineStage::Text,
            vertex_count: (terminal_quads.len() + editor_quads.len()) * 4,
            index_count: (terminal_quads.len() + editor_quads.len()) * 6,
        });
        builder.add(Batch {
            stage: PipelineStage::Overlay,
            vertex_count: 6,
            index_count: 6,
        });
        builder.build()
    }
}

impl Renderer for WgpuRenderer {
    fn ingest_damage(&mut self, damage: DamageRegion) {
        if damage.full_redraw {
            self.pending_damage.full_redraw = true;
        }
        self.pending_damage.dirty_rows.extend(damage.dirty_rows);
    }

    fn render(&mut self, snapshot: &RenderSnapshot) {
        if !self.pending_damage.full_redraw && self.pending_damage.dirty_rows.is_empty() {
            return;
        }
        if !self.pending_damage.full_redraw && !self.pacer.should_render() {
            return;
        }

        let start = Instant::now();
        let _batches = self.build_batches(snapshot);
        self.pending_damage.dirty_rows.clear();
        self.pending_damage.full_redraw = false;
        self.pacer.on_presented();
        self.frames += 1;
        self.stats.record(start.elapsed());
    }
}

#[derive(Default)]
pub struct NullRenderer {
    frames: usize,
}

impl NullRenderer {
    pub fn frames(&self) -> usize {
        self.frames
    }
}

impl Renderer for NullRenderer {
    fn ingest_damage(&mut self, _damage: DamageRegion) {}

    fn render(&mut self, _snapshot: &RenderSnapshot) {
        self.frames += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{NullRenderer, Renderer, WgpuRenderer};
    use crate::geometry::{
        build_panel_vertices, snapshot_to_cell_quads, snapshot_to_cell_quads_in_bounds,
    };
    use crate::types::{
        ColorTheme, DamageRegion, FontConfig, RenderConfig, RenderSnapshot, VsyncMode,
    };
    use winit::dpi::PhysicalSize;

    fn blank_snapshot() -> RenderSnapshot {
        RenderSnapshot {
            terminal_text: String::new(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: String::new(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        }
    }

    #[test]
    fn null_renderer_counts_frames() {
        let mut renderer = NullRenderer::default();
        renderer.ingest_damage(DamageRegion {
            full_redraw: true,
            dirty_rows: vec![0],
        });
        renderer.render(&RenderSnapshot {
            terminal_text: "t".to_string(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: "e".to_string(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        });
        assert_eq!(renderer.frames(), 1);
    }

    #[test]
    fn wgpu_renderer_respects_damage() {
        let mut renderer = WgpuRenderer::new(RenderConfig {
            vsync: VsyncMode::Off,
            target_fps: 1000,
            glyph_atlas_size: (1024, 1024),
            font: FontConfig::default(),
            theme: ColorTheme::default(),
            initial_size: None,
        });

        renderer.render(&RenderSnapshot {
            terminal_text: "noop".to_string(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: "noop".to_string(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        });
        assert_eq!(renderer.frames(), 1);
        assert_eq!(renderer.atlas_len(), 0);

        renderer.ingest_damage(DamageRegion {
            full_redraw: false,
            dirty_rows: vec![1],
        });
        renderer.render(&RenderSnapshot {
            terminal_text: "dirty".to_string(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: "editor".to_string(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        });
        assert!(renderer.frames() >= 1);
        assert!(renderer.stats().frame_count >= 1);
    }

    #[test]
    fn converts_snapshot_to_quads_with_damage_filter() {
        let snapshot = RenderSnapshot {
            terminal_text: "abc\nxyz".to_string(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: String::new(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        };

        let only_row_0 = snapshot_to_cell_quads(
            &snapshot,
            &DamageRegion {
                full_redraw: false,
                dirty_rows: vec![0],
            },
            3,
        );
        assert_eq!(only_row_0.len(), 3);
        assert_eq!(only_row_0[0].glyph, 'a');

        let full = snapshot_to_cell_quads(
            &snapshot,
            &DamageRegion {
                full_redraw: true,
                dirty_rows: vec![],
            },
            3,
        );
        assert_eq!(full.len(), 6);
    }

    #[test]
    fn separates_terminal_and_editor_quads_into_distinct_bands() {
        let terminal = snapshot_to_cell_quads_in_bounds(
            "term",
            &DamageRegion {
                full_redraw: true,
                dirty_rows: vec![],
            },
            4,
            (1.0, -0.4),
        );
        let editor = snapshot_to_cell_quads_in_bounds(
            "edit",
            &DamageRegion {
                full_redraw: true,
                dirty_rows: vec![],
            },
            4,
            (-0.4, -1.0),
        );

        assert_eq!(terminal.len(), 4);
        assert_eq!(editor.len(), 4);
        assert!(terminal.iter().all(|quad| quad.y > -0.4));
        assert!(editor.iter().all(|quad| quad.y <= -0.4));
    }

    #[test]
    fn panel_vertices_contain_cursor_quad() {
        let snapshot = RenderSnapshot {
            terminal_text: String::new(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: "abc".to_string(),
            editor_cursor_offset: 3,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: true,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        };
        let size = PhysicalSize::new(1280u32, 720u32);
        let verts = build_panel_vertices(size, &snapshot, 0.0, 8.4, 16.8, 0.0, 0.0);
        assert_eq!(verts.len(), 4 * 36);
    }

    #[test]
    fn ime_area_maps_to_editor_screen_region() {
        use crate::geometry::snapshot_to_ime_area;

        let snapshot = RenderSnapshot {
            terminal_text: String::new(),
            terminal_fg_colors: vec![],
            terminal_bg_colors: vec![],
            editor_text: "hello".to_string(),
            editor_cursor_offset: 5,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: true,
            split_ratio: 0.7,
            resize_overlay: None,
            editor_line_count: 1,
            editor_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            tab_labels: vec![],
            active_tab: 0,
            tab_context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: ColorTheme::default(),
            padding_h: 0,
            padding_v: 0,
            settings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
        };
        let (pos, size) = snapshot_to_ime_area(&snapshot, PhysicalSize::new(1280u32, 720u32));
        assert!(pos.y > 500.0, "IME y={:.1} should be in the editor half", pos.y);
        assert!(pos.x > 0.0, "IME x={:.1} should be right of origin", pos.x);
        assert!(size.width > 0.0 && size.height > 0.0, "IME size must be positive");
    }
}

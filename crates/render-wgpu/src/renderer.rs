use std::time::Instant;

use crate::batch::{Batch, BatchBuilder, FramePacer};
use crate::geometry::{snapshot_to_cell_quads_in_bounds, snapshot_to_text_quads_in_bounds};
use crate::types::{
    DamageRegion, PaneLayout, PipelineStage, RenderConfig, RenderSnapshot, RenderStats,
};

/// Public renderer interface used by the application layer.
///
/// Implementations consume a [`RenderSnapshot`] each frame and emit GPU
/// draw calls; [`Renderer::ingest_damage`] lets the caller hint which rows
/// changed so the renderer can skip clean cells.
pub trait Renderer {
    /// Provide the screen damage description for the upcoming frame.
    fn ingest_damage(&mut self, damage: DamageRegion);
    /// Render one frame from `snapshot`.
    fn render(&mut self, snapshot: &RenderSnapshot);
}

pub struct WgpuRenderer {
    config: RenderConfig,
    pending_damage: DamageRegion,
    pacer: FramePacer,
    frames: usize,
    stats: RenderStats,
    batch_builder: BatchBuilder,
}

impl WgpuRenderer {
    pub fn new(config: RenderConfig) -> Self {
        Self {
            pacer: FramePacer::new(config.target_fps),
            config,
            pending_damage: DamageRegion::default(),
            frames: 0,
            stats: RenderStats::default(),
            batch_builder: BatchBuilder::default(),
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

    fn build_batches(&mut self, snapshot: &RenderSnapshot) -> Vec<Batch> {
        let layout = PaneLayout {
            split_ratio: snapshot.split_ratio,
        };
        let terminal_quads = snapshot_to_cell_quads_in_bounds(
            &snapshot.terminal_text,
            &self.pending_damage,
            80,
            layout.terminal_bounds(),
        );
        let editor_quads =
            snapshot_to_text_quads_in_bounds(&snapshot.editor_text, 80, layout.editor_bounds());
        self.batch_builder.clear();
        self.batch_builder.add(Batch {
            stage: PipelineStage::Background,
            vertex_count: 12,
            index_count: 12,
        });
        self.batch_builder.add(Batch {
            stage: PipelineStage::Text,
            vertex_count: (terminal_quads.len() + editor_quads.len()) * 4,
            index_count: (terminal_quads.len() + editor_quads.len()) * 6,
        });
        self.batch_builder.add(Batch {
            stage: PipelineStage::Overlay,
            vertex_count: 6,
            index_count: 6,
        });
        self.batch_builder.build()
    }
}

impl Renderer for WgpuRenderer {
    fn ingest_damage(&mut self, damage: DamageRegion) {
        self.pending_damage.merge_from(&damage);
    }

    fn render(&mut self, snapshot: &RenderSnapshot) {
        if self.pending_damage.is_empty() {
            return;
        }
        if !self.pending_damage.full_redraw && !self.pacer.should_render() {
            return;
        }

        let start = Instant::now();
        let _batches = self.build_batches(snapshot);
        self.pending_damage.clear();
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
    use std::sync::Arc;

    use super::{NullRenderer, Renderer, WgpuRenderer};
    use crate::geometry::{
        build_panel_vertices, snapshot_to_cell_quads, snapshot_to_cell_quads_in_bounds,
    };
    use crate::types::{
        ColorTheme, DamageRegion, FontConfig, RenderConfig, RenderSnapshot, VsyncMode,
    };
    use winit::dpi::PhysicalSize;

    #[allow(dead_code)]
    fn blank_snapshot() -> RenderSnapshot {
        RenderSnapshot {
            terminal_rows: vec![],
            terminal_damage: Arc::new(DamageRegion::default()),
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
            search_highlights: vec![],
            search_current_highlight: None,
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
            search_panel: None,
            terminal_links: vec![],
            request_exit: false,
            cursor_shape: 0,
            bell_active: false,
            terminal_cursor_row: 0,
            terminal_cursor_col: 0,
            terminal_fullscreen: false,
            terminal_screen_version: 0,
            terminal_styles: vec![],
            cursor_blink_on: true,
            toast_stack: vec![],
            command_palette: None,
        }
    }

    fn damage(full_redraw: bool, dirty_rows: Vec<usize>) -> DamageRegion {
        DamageRegion {
            full_redraw,
            dirty_rows,
            cols: 0,
            dirty_cells: Vec::new(),
        }
    }

    #[test]
    fn null_renderer_counts_frames() {
        let mut renderer = NullRenderer::default();
        renderer.ingest_damage(damage(true, vec![0]));
        renderer.render(&RenderSnapshot {
            terminal_text: "t".to_string(),
            editor_text: "e".to_string(),
            ..blank_snapshot()
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
            initial_position: None,
        });

        renderer.render(&RenderSnapshot {
            terminal_text: "noop".to_string(),
            editor_text: "noop".to_string(),
            ..blank_snapshot()
        });
        assert_eq!(renderer.frames(), 1);
        assert_eq!(renderer.atlas_len(), 0);

        renderer.ingest_damage(damage(false, vec![1]));
        renderer.render(&RenderSnapshot {
            terminal_text: "dirty".to_string(),
            editor_text: "editor".to_string(),
            ..blank_snapshot()
        });
        assert!(renderer.frames() >= 1);
        assert!(renderer.stats().frame_count >= 1);
    }

    #[test]
    fn converts_snapshot_to_quads_with_damage_filter() {
        let snapshot = RenderSnapshot {
            terminal_text: "abc\nxyz".to_string(),
            ..blank_snapshot()
        };

        let only_row_0 = snapshot_to_cell_quads(
            &snapshot,
            &DamageRegion {
                full_redraw: false,
                dirty_rows: vec![0],
                cols: 0,
                dirty_cells: Vec::new(),
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
                cols: 0,
                dirty_cells: Vec::new(),
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
                cols: 0,
                dirty_cells: Vec::new(),
            },
            4,
            (1.0, -0.4),
        );
        let editor = snapshot_to_cell_quads_in_bounds(
            "edit",
            &DamageRegion {
                full_redraw: true,
                dirty_rows: vec![],
                cols: 0,
                dirty_cells: Vec::new(),
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
            editor_text: "abc".to_string(),
            editor_cursor_offset: 3,
            editor_focused: true,
            ..blank_snapshot()
        };
        let size = PhysicalSize::new(1280u32, 720u32);
        let verts = build_panel_vertices(size, &snapshot, 0.0, 8.4, 16.8, 0.0, 0.0);
        assert_eq!(verts.len(), 4 * 36); // 3 panel bg quads + editor caret (terminal cursor hidden when not fullscreen)
    }

    #[test]
    fn ime_area_maps_to_editor_screen_region() {
        use crate::geometry::snapshot_to_ime_area;

        let snapshot = RenderSnapshot {
            editor_text: "hello".to_string(),
            editor_cursor_offset: 5,
            editor_focused: true,
            ..blank_snapshot()
        };
        let (pos, size) = snapshot_to_ime_area(&snapshot, PhysicalSize::new(1280u32, 720u32));
        assert!(
            pos.y > 500.0,
            "IME y={:.1} should be in the editor half",
            pos.y
        );
        assert!(pos.x > 0.0, "IME x={:.1} should be right of origin", pos.x);
        assert!(
            size.width > 0.0 && size.height > 0.0,
            "IME size must be positive"
        );
    }
}

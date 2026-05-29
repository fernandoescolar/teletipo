use crate::batch::CellQuad;
use crate::types::{DamageRegion, RenderSnapshot};

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
            if ch == ' ' || ch == '\0' {
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

#[cfg(test)]
mod tests {
    use super::snapshot_to_cell_quads_in_bounds;
    use crate::types::DamageRegion;

    #[test]
    fn snapshot_to_cell_quads_in_bounds_matches_expected_layout() {
        let damage = DamageRegion {
            full_redraw: true,
            dirty_rows: Vec::new(),
        };

        let quads = snapshot_to_cell_quads_in_bounds("ab\nc", &damage, 4, (1.0, -1.0));

        assert_eq!(
            quads,
            vec![
                crate::batch::CellQuad {
                    x: -1.0,
                    y: 0.0,
                    w: 0.5,
                    h: 1.0,
                    glyph: 'a',
                },
                crate::batch::CellQuad {
                    x: -0.5,
                    y: 0.0,
                    w: 0.5,
                    h: 1.0,
                    glyph: 'b',
                },
                crate::batch::CellQuad {
                    x: -1.0,
                    y: -1.0,
                    w: 0.5,
                    h: 1.0,
                    glyph: 'c',
                },
            ]
        );
    }
}

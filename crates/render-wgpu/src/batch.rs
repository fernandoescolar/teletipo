use std::time::{Duration, Instant};

use crate::types::PipelineStage;

#[derive(Debug, Clone)]
pub struct Batch {
    pub stage: PipelineStage,
    pub vertex_count: usize,
    pub index_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub glyph: char,
}

#[derive(Debug, Default)]
pub struct BatchBuilder {
    batches: Vec<Batch>,
}

impl BatchBuilder {
    pub fn add(&mut self, batch: Batch) {
        self.batches.push(batch);
    }

    pub fn build(self) -> Vec<Batch> {
        self.batches
    }
}

#[derive(Debug)]
pub struct FramePacer {
    target_frame: Duration,
    last_frame: Instant,
    pub presented: u64,
}

impl FramePacer {
    pub fn new(target_fps: u32) -> Self {
        let fps = target_fps.max(1) as u64;
        Self {
            target_frame: Duration::from_micros(1_000_000 / fps),
            last_frame: Instant::now(),
            presented: 0,
        }
    }

    pub fn should_render(&self) -> bool {
        self.last_frame.elapsed() >= self.target_frame
    }

    pub fn on_presented(&mut self) {
        self.last_frame = Instant::now();
        self.presented = self.presented.saturating_add(1);
    }
}

use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct RenderStats {
    pub frame_count: u64,
    pub total_frame_time_us: u128,
    pub max_frame_time_us: u128,
}

impl RenderStats {
    pub fn record(&mut self, frame_time: Duration) {
        let micros = frame_time.as_micros();
        self.frame_count = self.frame_count.saturating_add(1);
        self.total_frame_time_us = self.total_frame_time_us.saturating_add(micros);
        self.max_frame_time_us = self.max_frame_time_us.max(micros);
    }

    pub fn avg_frame_time_us(&self) -> u128 {
        if self.frame_count == 0 {
            0
        } else {
            self.total_frame_time_us / self.frame_count as u128
        }
    }
}

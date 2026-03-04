use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

/// Quality scaler for adaptive bitrate video.
///
/// NOTE: This component is not currently connected to the media pipeline.
/// To use, integrate with the congestion controller to receive QP feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalingDecision {
    KeepResolution,
    ScaleDown,
    ScaleUp,
}

#[derive(Clone, Debug)]
pub struct QualityScalerConfig {
    pub low_qp_threshold: u32,
    pub high_qp_threshold: u32,
    pub min_frames_before_scale: u32,
    pub target_fps: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub scale_factor_num: u32,
    pub scale_factor_den: u32,
}

impl Default for QualityScalerConfig {
    fn default() -> Self {
        Self {
            low_qp_threshold: 20,
            high_qp_threshold: 37,
            min_frames_before_scale: 30,
            target_fps: 30,
            min_width: 180,
            min_height: 120,
            max_width: 1920,
            max_height: 1080,
            scale_factor_num: 3,
            scale_factor_den: 4,
        }
    }
}

pub struct QualityScaler {
    cfg: QualityScalerConfig,
    qp_sum: AtomicU64,
    frame_count: AtomicU32,
    current_width: AtomicU32,
    current_height: AtomicU32,
    scale_down_count: AtomicU32,
    scale_up_count: AtomicU32,
    consecutive_low_qp: AtomicI32,
    consecutive_high_qp: AtomicI32,
}

impl QualityScaler {
    pub fn new(cfg: QualityScalerConfig, width: u32, height: u32) -> Self {
        Self {
            cfg,
            qp_sum: AtomicU64::new(0),
            frame_count: AtomicU32::new(0),
            current_width: AtomicU32::new(width),
            current_height: AtomicU32::new(height),
            scale_down_count: AtomicU32::new(0),
            scale_up_count: AtomicU32::new(0),
            consecutive_low_qp: AtomicI32::new(0),
            consecutive_high_qp: AtomicI32::new(0),
        }
    }

    pub fn report_qp(&self, qp: u32) -> ScalingDecision {
        self.qp_sum.fetch_add(qp as u64, Ordering::Relaxed);
        let count = self.frame_count.fetch_add(1, Ordering::Relaxed) + 1;

        if qp < self.cfg.low_qp_threshold {
            self.consecutive_low_qp.fetch_add(1, Ordering::Relaxed);
            self.consecutive_high_qp.store(0, Ordering::Relaxed);
        } else if qp > self.cfg.high_qp_threshold {
            self.consecutive_high_qp.fetch_add(1, Ordering::Relaxed);
            self.consecutive_low_qp.store(0, Ordering::Relaxed);
        } else {
            self.consecutive_low_qp.store(0, Ordering::Relaxed);
            self.consecutive_high_qp.store(0, Ordering::Relaxed);
        }

        if count < self.cfg.min_frames_before_scale {
            return ScalingDecision::KeepResolution;
        }

        let low_count = self.consecutive_low_qp.load(Ordering::Relaxed);
        let high_count = self.consecutive_high_qp.load(Ordering::Relaxed);

        if high_count >= self.cfg.min_frames_before_scale as i32 {
            let w = self.current_width.load(Ordering::Relaxed);
            let h = self.current_height.load(Ordering::Relaxed);
            let new_w = (w * self.cfg.scale_factor_num / self.cfg.scale_factor_den).max(self.cfg.min_width);
            let new_h = (h * self.cfg.scale_factor_num / self.cfg.scale_factor_den).max(self.cfg.min_height);
            if new_w < w || new_h < h {
                self.current_width.store(new_w, Ordering::Relaxed);
                self.current_height.store(new_h, Ordering::Relaxed);
                self.scale_down_count.fetch_add(1, Ordering::Relaxed);
                self.consecutive_high_qp.store(0, Ordering::Relaxed);
                return ScalingDecision::ScaleDown;
            }
        } else if low_count >= self.cfg.min_frames_before_scale as i32 {
            let w = self.current_width.load(Ordering::Relaxed);
            let h = self.current_height.load(Ordering::Relaxed);
            let new_w = (w * self.cfg.scale_factor_den / self.cfg.scale_factor_num).min(self.cfg.max_width);
            let new_h = (h * self.cfg.scale_factor_den / self.cfg.scale_factor_num).min(self.cfg.max_height);
            if new_w > w || new_h > h {
                self.current_width.store(new_w, Ordering::Relaxed);
                self.current_height.store(new_h, Ordering::Relaxed);
                self.scale_up_count.fetch_add(1, Ordering::Relaxed);
                self.consecutive_low_qp.store(0, Ordering::Relaxed);
                return ScalingDecision::ScaleUp;
            }
        }
        ScalingDecision::KeepResolution
    }

    pub fn current_resolution(&self) -> (u32, u32) {
        (
            self.current_width.load(Ordering::Relaxed),
            self.current_height.load(Ordering::Relaxed),
        )
    }

    pub fn average_qp(&self) -> u32 {
        let count = self.frame_count.load(Ordering::Relaxed);
        if count == 0 { return 0; }
        (self.qp_sum.load(Ordering::Relaxed) / count as u64) as u32
    }

    pub fn scale_down_count(&self) -> u32 {
        self.scale_down_count.load(Ordering::Relaxed)
    }

    pub fn scale_up_count(&self) -> u32 {
        self.scale_up_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_down_on_high_qp() {
        let cfg = QualityScalerConfig {
            min_frames_before_scale: 5,
            ..Default::default()
        };
        let scaler = QualityScaler::new(cfg, 1280, 720);
        let mut decision = ScalingDecision::KeepResolution;
        for _ in 0..5 {
            decision = scaler.report_qp(50);
        }
        assert_eq!(decision, ScalingDecision::ScaleDown);
        let (w, h) = scaler.current_resolution();
        assert!(w < 1280 || h < 720);
    }

    #[test]
    fn scale_up_on_low_qp() {
        let cfg = QualityScalerConfig {
            min_frames_before_scale: 5,
            ..Default::default()
        };
        let scaler = QualityScaler::new(cfg, 640, 360);
        let mut decision = ScalingDecision::KeepResolution;
        for _ in 0..5 {
            decision = scaler.report_qp(10);
        }
        assert_eq!(decision, ScalingDecision::ScaleUp);
    }
}

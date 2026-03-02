use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use super::twcc_aggregator::TwccFeedback;

#[derive(Clone, Debug)]
pub struct AimdConfig {
    pub initial_bitrate_bps: u32,
    pub min_bitrate_bps: u32,
    pub max_bitrate_bps: u32,
    pub additive_increase_bps: u32,
    pub multiplicative_decrease: u32,
    pub loss_threshold_high: u32,
    pub loss_threshold_low: u32,
}

impl Default for AimdConfig {
    fn default() -> Self {
        Self {
            initial_bitrate_bps: 500_000,
            min_bitrate_bps: 30_000,
            max_bitrate_bps: 30_000_000,
            additive_increase_bps: 1_000,
            multiplicative_decrease: 85,
            loss_threshold_high: 10,
            loss_threshold_low: 2,
        }
    }
}

pub struct AimdController {
    cfg: AimdConfig,
    target_bps: AtomicU32,
    consecutive_acked: AtomicU32,
    total_updates: AtomicU64,
}

impl AimdController {
    pub fn new(cfg: AimdConfig) -> Self {
        let init = cfg.initial_bitrate_bps;
        Self {
            cfg,
            target_bps: AtomicU32::new(init),
            consecutive_acked: AtomicU32::new(0),
            total_updates: AtomicU64::new(0),
        }
    }

    pub fn on_feedback(&self, feedback: &TwccFeedback) {
        self.total_updates.fetch_add(1, Ordering::Relaxed);
        if feedback.sent_count == 0 {
            return;
        }
        let lost = feedback.sent_count.saturating_sub(feedback.received_count);
        let loss_pct = lost * 100 / feedback.sent_count;
        let cur = self.target_bps.load(Ordering::Acquire);
        let new_bps = if loss_pct > self.cfg.loss_threshold_high as u64 {
            self.consecutive_acked.store(0, Ordering::Relaxed);
            (cur as u64 * self.cfg.multiplicative_decrease as u64 / 100)
                .max(self.cfg.min_bitrate_bps as u64)
                .min(self.cfg.max_bitrate_bps as u64) as u32
        } else if loss_pct < self.cfg.loss_threshold_low as u64 {
            self.consecutive_acked.fetch_add(feedback.received_count as u32, Ordering::Relaxed);
            let increase = (self.cfg.additive_increase_bps as u64 *
                feedback.received_count).min(1_000_000) as u32;
            cur.saturating_add(increase).min(self.cfg.max_bitrate_bps)
        } else {
            cur
        };
        self.target_bps.store(new_bps, Ordering::Release);
    }

    pub fn target_bitrate_bps(&self) -> u32 {
        self.target_bps.load(Ordering::Relaxed)
    }

    pub fn force_set_bitrate(&self, bps: u32) {
        let clamped = bps.max(self.cfg.min_bitrate_bps).min(self.cfg.max_bitrate_bps);
        self.target_bps.store(clamped, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aimd_increase_on_no_loss() {
        let cfg = AimdConfig { additive_increase_bps: 10_000, ..Default::default() };
        let ctrl = AimdController::new(cfg);
        let fb = TwccFeedback { sent_count: 10, received_count: 10, ..Default::default() };
        ctrl.on_feedback(&fb);
        assert!(ctrl.target_bitrate_bps() > 500_000);
    }

    #[test]
    fn aimd_decrease_on_high_loss() {
        let cfg = AimdConfig::default();
        let ctrl = AimdController::new(cfg);
        let fb = TwccFeedback { sent_count: 10, received_count: 8, ..Default::default() };
        ctrl.on_feedback(&fb);
        assert!(ctrl.target_bitrate_bps() <= 500_000);
    }
}

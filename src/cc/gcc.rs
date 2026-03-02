use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use super::twcc_aggregator::TwccFeedback;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandwidthUsage {
    Underuse,
    Normal,
    Overuse,
}

impl Default for BandwidthUsage {
    fn default() -> Self {
        BandwidthUsage::Normal
    }
}

#[derive(Clone, Debug)]
pub struct GccConfig {
    pub initial_bitrate_bps: u32,
    pub min_bitrate_bps: u32,
    pub max_bitrate_bps: u32,
    pub overuse_time_threshold_ms: u64,
    pub overuse_gradient_threshold: f64,
    pub aimd_decrease_factor: u32,
    pub aimd_increase_rate_bps_per_s: u32,
}

impl Default for GccConfig {
    fn default() -> Self {
        Self {
            initial_bitrate_bps: 1_000_000,
            min_bitrate_bps: 50_000,
            max_bitrate_bps: 50_000_000,
            overuse_time_threshold_ms: 100,
            overuse_gradient_threshold: 12.5,
            aimd_decrease_factor: 85,
            aimd_increase_rate_bps_per_s: 8_000,
        }
    }
}

pub struct DelayBasedBwe {
    prev_group_delay_sum_ns: AtomicI64,
    prev_group_count: AtomicU32,
    delay_gradient: AtomicI64,
    overuse_start_ns: AtomicU64,
    threshold: AtomicI64,
    state: parking_lot::Mutex<BandwidthUsage>,
}

impl DelayBasedBwe {
    pub fn new() -> Self {
        Self {
            prev_group_delay_sum_ns: AtomicI64::new(0),
            prev_group_count: AtomicU32::new(0),
            delay_gradient: AtomicI64::new(0),
            overuse_start_ns: AtomicU64::new(0),
            threshold: AtomicI64::new(12_500_000),
            state: parking_lot::Mutex::new(BandwidthUsage::Normal),
        }
    }

    pub fn update(&self, inter_arrival_delta_ns: i64, inter_departure_delta_ns: i64, now_ns: u64) -> BandwidthUsage {
        let d = inter_arrival_delta_ns - inter_departure_delta_ns;
        let alpha = 8i64;
        let old = self.delay_gradient.load(Ordering::Acquire);
        let new_grad = old + (d - old) / alpha;
        self.delay_gradient.store(new_grad, Ordering::Release);

        let thresh = self.threshold.load(Ordering::Acquire);
        let usage = if new_grad > thresh {
            BandwidthUsage::Overuse
        } else if new_grad < -thresh {
            BandwidthUsage::Underuse
        } else {
            BandwidthUsage::Normal
        };

        if usage == BandwidthUsage::Overuse {
            let start = self.overuse_start_ns.load(Ordering::Acquire);
            if start == 0 {
                self.overuse_start_ns.store(now_ns, Ordering::Release);
            }
        } else {
            self.overuse_start_ns.store(0, Ordering::Release);
        }

        *self.state.lock() = usage;
        usage
    }

    pub fn current_usage(&self) -> BandwidthUsage {
        *self.state.lock()
    }

    pub fn adapt_threshold(&self, usage: BandwidthUsage) {
        let thresh = self.threshold.load(Ordering::Acquire);
        let new_thresh = match usage {
            BandwidthUsage::Overuse => (thresh * 105 / 100).min(600_000_000),
            BandwidthUsage::Underuse => (thresh * 95 / 100).max(6_000_000),
            BandwidthUsage::Normal => thresh,
        };
        self.threshold.store(new_thresh, Ordering::Release);
    }
}

pub struct GccController {
    cfg: GccConfig,
    delay_bwe: DelayBasedBwe,
    target_bps: AtomicU32,
    loss_fraction_q8: AtomicU32,
    last_update_ns: AtomicU64,
    total_sent_bytes: AtomicU64,
}

impl GccController {
    pub fn new(cfg: GccConfig) -> Self {
        let init = cfg.initial_bitrate_bps;
        Self {
            cfg,
            delay_bwe: DelayBasedBwe::new(),
            target_bps: AtomicU32::new(init),
            loss_fraction_q8: AtomicU32::new(0),
            last_update_ns: AtomicU64::new(0),
            total_sent_bytes: AtomicU64::new(0),
        }
    }

    pub fn on_packet_sent(&self, size_bytes: usize, _send_time_ns: u64) {
        self.total_sent_bytes.fetch_add(size_bytes as u64, Ordering::Relaxed);
    }

    pub fn on_feedback(&self, feedback: &TwccFeedback) {
        if feedback.received_count == 0 {
            return;
        }
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let usage = if feedback.inter_arrival_delta_ns != 0 || feedback.inter_departure_delta_ns != 0 {
            self.delay_bwe.update(
                feedback.inter_arrival_delta_ns,
                feedback.inter_departure_delta_ns,
                now_ns,
            )
        } else {
            self.delay_bwe.current_usage()
        };

        self.delay_bwe.adapt_threshold(usage);

        let lost = feedback.sent_count.saturating_sub(feedback.received_count);
        let fraction = if feedback.sent_count > 0 {
            (lost * 256 / feedback.sent_count).min(255) as u32
        } else {
            0
        };
        self.loss_fraction_q8.store(fraction, Ordering::Relaxed);

        let cur = self.target_bps.load(Ordering::Acquire);
        let new_bps = self.compute_target(cur, usage, fraction, now_ns);
        self.target_bps.store(new_bps, Ordering::Release);
        self.last_update_ns.store(now_ns, Ordering::Relaxed);
    }

    fn compute_target(&self, cur: u32, usage: BandwidthUsage, loss_q8: u32, now_ns: u64) -> u32 {
        let loss_based = if loss_q8 > 26 {
            (cur as u64 * self.cfg.aimd_decrease_factor as u64 / 100).min(cur as u64) as u32
        } else {
            let elapsed_s = {
                let last = self.last_update_ns.load(Ordering::Relaxed);
                if last == 0 { 0.0f64 }
                else { (now_ns.saturating_sub(last)) as f64 / 1e9 }
            };
            let increase = (self.cfg.aimd_increase_rate_bps_per_s as f64 * elapsed_s) as u32;
            cur.saturating_add(increase)
        };

        let delay_based = match usage {
            BandwidthUsage::Overuse => (cur as u64 * 85 / 100) as u32,
            BandwidthUsage::Normal => loss_based,
            BandwidthUsage::Underuse => loss_based,
        };

        delay_based
            .max(self.cfg.min_bitrate_bps)
            .min(self.cfg.max_bitrate_bps)
    }

    pub fn target_bitrate_bps(&self) -> u32 {
        self.target_bps.load(Ordering::Relaxed)
    }

    pub fn bandwidth_usage(&self) -> BandwidthUsage {
        self.delay_bwe.current_usage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::TwccFeedback;

    #[test]
    fn gcc_initial_bitrate() {
        let cfg = GccConfig { initial_bitrate_bps: 2_000_000, ..Default::default() };
        let gcc = GccController::new(cfg);
        assert_eq!(gcc.target_bitrate_bps(), 2_000_000);
    }

    #[test]
    fn gcc_overuse_decreases_target() {
        let cfg = GccConfig::default();
        let gcc = GccController::new(cfg);
        let fb = TwccFeedback {
            sent_count: 10,
            received_count: 10,
            inter_arrival_delta_ns: 100_000_000,
            inter_departure_delta_ns: 0,
        };
        gcc.on_feedback(&fb);
        gcc.on_feedback(&fb);
        gcc.on_feedback(&fb);
        let target = gcc.target_bitrate_bps();
        assert!(target < 1_000_000);
    }

    #[test]
    fn gcc_enforces_min_bitrate() {
        let cfg = GccConfig { min_bitrate_bps: 100_000, initial_bitrate_bps: 100_000, ..Default::default() };
        let gcc = GccController::new(cfg);
        let fb = TwccFeedback {
            sent_count: 10,
            received_count: 10,
            inter_arrival_delta_ns: 1_000_000_000,
            inter_departure_delta_ns: 0,
        };
        for _ in 0..100 {
            gcc.on_feedback(&fb);
        }
        assert!(gcc.target_bitrate_bps() >= 100_000);
    }
}

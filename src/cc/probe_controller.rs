use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeState {
    Inactive,
    Probing,
    Done,
}

#[derive(Clone, Copy, Debug)]
pub struct ProbeResult {
    pub estimated_bps: u32,
    pub success: bool,
}

#[derive(Clone, Debug)]
pub struct ProbeConfig {
    pub start_bitrate_bps: u32,
    pub max_probe_bitrate_bps: u32,
    pub probe_window_ms: u64,
    pub min_probe_packets: u32,
    pub probe_step_multiplier_num: u32,
    pub probe_step_multiplier_den: u32,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            start_bitrate_bps: 900_000,
            max_probe_bitrate_bps: 6_000_000,
            probe_window_ms: 500,
            min_probe_packets: 5,
            probe_step_multiplier_num: 2,
            probe_step_multiplier_den: 1,
        }
    }
}

pub struct ProbeController {
    cfg: ProbeConfig,
    probing: AtomicBool,
    probe_start_ns: AtomicU64,
    probe_bitrate_bps: AtomicU32,
    probe_bytes_sent: AtomicU64,
    probe_packets_sent: AtomicU32,
    probe_packets_received: AtomicU32,
    probe_count: AtomicU32,
}

impl ProbeController {
    pub fn new(cfg: ProbeConfig) -> Self {
        Self {
            cfg,
            probing: AtomicBool::new(false),
            probe_start_ns: AtomicU64::new(0),
            probe_bitrate_bps: AtomicU32::new(0),
            probe_bytes_sent: AtomicU64::new(0),
            probe_packets_sent: AtomicU32::new(0),
            probe_packets_received: AtomicU32::new(0),
            probe_count: AtomicU32::new(0),
        }
    }

    pub fn start_probe(&self, current_bps: u32, now_ns: u64) -> bool {
        if self.probing.load(Ordering::Acquire) {
            return false;
        }
        let probe_bps = (current_bps as u64 * self.cfg.probe_step_multiplier_num as u64 / self.cfg.probe_step_multiplier_den as u64)
            .min(self.cfg.max_probe_bitrate_bps as u64) as u32;
        if probe_bps <= current_bps {
            return false;
        }
        self.probe_bitrate_bps.store(probe_bps, Ordering::Relaxed);
        self.probe_start_ns.store(now_ns, Ordering::Release);
        self.probe_bytes_sent.store(0, Ordering::Relaxed);
        self.probe_packets_sent.store(0, Ordering::Relaxed);
        self.probe_packets_received.store(0, Ordering::Relaxed);
        self.probing.store(true, Ordering::Release);
        true
    }

    pub fn on_probe_packet_sent(&self, size_bytes: usize) {
        if !self.probing.load(Ordering::Acquire) {
            return;
        }
        self.probe_bytes_sent.fetch_add(size_bytes as u64, Ordering::Relaxed);
        self.probe_packets_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_probe_feedback(&self, received: u32) {
        if !self.probing.load(Ordering::Acquire) {
            return;
        }
        self.probe_packets_received.fetch_add(received, Ordering::Relaxed);
    }

    pub fn check_probe_complete(&self, now_ns: u64) -> Option<ProbeResult> {
        if !self.probing.load(Ordering::Acquire) {
            return None;
        }
        let start = self.probe_start_ns.load(Ordering::Acquire);
        let elapsed_ms = (now_ns.saturating_sub(start)) / 1_000_000;
        let sent = self.probe_packets_sent.load(Ordering::Relaxed);
        let received = self.probe_packets_received.load(Ordering::Relaxed);

        let window_done = elapsed_ms >= self.cfg.probe_window_ms;
        let packets_done = sent >= self.cfg.min_probe_packets;

        if !window_done && !packets_done {
            return None;
        }

        self.probing.store(false, Ordering::Release);
        self.probe_count.fetch_add(1, Ordering::Relaxed);

        let success = sent > 0 && received * 10 >= sent * 8;
        let estimated_bps = if success {
            self.probe_bitrate_bps.load(Ordering::Relaxed)
        } else {
            let bytes = self.probe_bytes_sent.load(Ordering::Relaxed);
            let elapsed_s = elapsed_ms.max(1) as f64 / 1000.0;
            (bytes as f64 * 8.0 / elapsed_s) as u32
        };

        Some(ProbeResult { estimated_bps, success })
    }

    pub fn is_probing(&self) -> bool {
        self.probing.load(Ordering::Acquire)
    }

    pub fn probe_target_bps(&self) -> u32 {
        self.probe_bitrate_bps.load(Ordering::Relaxed)
    }

    pub fn probe_count(&self) -> u32 {
        self.probe_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_start_and_complete() {
        let cfg = ProbeConfig {
            probe_window_ms: 10,
            min_probe_packets: 2,
            ..Default::default()
        };
        let ctrl = ProbeController::new(cfg);
        let now = 1_000_000_000u64;
        assert!(ctrl.start_probe(500_000, now));
        assert!(ctrl.is_probing());
        ctrl.on_probe_packet_sent(1200);
        ctrl.on_probe_packet_sent(1200);
        ctrl.on_probe_feedback(2);
        let result = ctrl.check_probe_complete(now + 20_000_000);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.success);
        assert!(!ctrl.is_probing());
    }

    #[test]
    fn probe_does_not_exceed_max() {
        let cfg = ProbeConfig { max_probe_bitrate_bps: 2_000_000, ..Default::default() };
        let ctrl = ProbeController::new(cfg);
        ctrl.start_probe(3_000_000, 0);
        assert!(ctrl.probe_target_bps() <= 2_000_000);
    }
}

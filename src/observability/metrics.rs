use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub packets_received: u64,
    pub packets_sent: u64,
    pub packets_dropped: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub jitter_buffer_ms: u64,
    pub rtt_ms: i64,
    pub loss_fraction: f32,
    pub target_bitrate_bps: u32,
    pub latency_p99_us: u64,
    pub nack_count: u64,
    pub pli_count: u64,
    pub fir_count: u64,
    pub keyframe_count: u64,
    pub freeze_count: u32,
    pub qp_avg: u32,
}

impl Default for MetricsSnapshot {
    fn default() -> Self {
        Self {
            packets_received: 0,
            packets_sent: 0,
            packets_dropped: 0,
            bytes_received: 0,
            bytes_sent: 0,
            jitter_buffer_ms: 0,
            rtt_ms: 0,
            loss_fraction: 0.0,
            target_bitrate_bps: 0,
            latency_p99_us: 0,
            nack_count: 0,
            pli_count: 0,
            fir_count: 0,
            keyframe_count: 0,
            freeze_count: 0,
            qp_avg: 0,
        }
    }
}

pub struct StreamMetrics {
    pub packets_received: AtomicU64,
    pub packets_sent: AtomicU64,
    pub packets_dropped: AtomicU64,
    pub bytes_received: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub jitter_buffer_ns: AtomicU64,
    pub rtt_ns: AtomicI64,
    pub nack_count: AtomicU64,
    pub pli_count: AtomicU64,
    pub fir_count: AtomicU64,
    pub keyframe_count: AtomicU64,
    pub freeze_count: AtomicU32,
    pub qp_sum: AtomicU64,
    pub qp_count: AtomicU64,
}

impl StreamMetrics {
    pub const fn new() -> Self {
        Self {
            packets_received: AtomicU64::new(0),
            packets_sent: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            jitter_buffer_ns: AtomicU64::new(0),
            rtt_ns: AtomicI64::new(0),
            nack_count: AtomicU64::new(0),
            pli_count: AtomicU64::new(0),
            fir_count: AtomicU64::new(0),
            keyframe_count: AtomicU64::new(0),
            freeze_count: AtomicU32::new(0),
            qp_sum: AtomicU64::new(0),
            qp_count: AtomicU64::new(0),
        }
    }

    pub fn record_received(&self, bytes: usize) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn record_sent(&self, bytes: usize) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn record_dropped(&self) {
        self.packets_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_nack(&self) {
        self.nack_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pli(&self) {
        self.pli_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_fir(&self) {
        self.fir_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_keyframe(&self) {
        self.keyframe_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_qp(&self, qp: u32) {
        self.qp_sum.fetch_add(qp as u64, Ordering::Relaxed);
        self.qp_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn update_rtt_ns(&self, rtt: i64) {
        self.rtt_ns.store(rtt, Ordering::Relaxed);
    }

    pub fn update_jitter_ns(&self, jitter: u64) {
        self.jitter_buffer_ns.store(jitter, Ordering::Relaxed);
    }

    pub fn snapshot(&self, target_bitrate_bps: u32, latency_p99_ns: u64) -> MetricsSnapshot {
        let qp_count = self.qp_count.load(Ordering::Relaxed);
        let qp_avg = if qp_count > 0 {
            (self.qp_sum.load(Ordering::Relaxed) / qp_count) as u32
        } else {
            0
        };
        let rx = self.packets_received.load(Ordering::Relaxed);
        let dropped = self.packets_dropped.load(Ordering::Relaxed);
        let total = rx + dropped;
        let loss_fraction = if total > 0 { dropped as f32 / total as f32 } else { 0.0 };
        MetricsSnapshot {
            packets_received: rx,
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            packets_dropped: dropped,
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            jitter_buffer_ms: self.jitter_buffer_ns.load(Ordering::Relaxed) / 1_000_000,
            rtt_ms: self.rtt_ns.load(Ordering::Relaxed) / 1_000_000,
            loss_fraction,
            target_bitrate_bps,
            latency_p99_us: latency_p99_ns / 1000,
            nack_count: self.nack_count.load(Ordering::Relaxed),
            pli_count: self.pli_count.load(Ordering::Relaxed),
            fir_count: self.fir_count.load(Ordering::Relaxed),
            keyframe_count: self.keyframe_count.load(Ordering::Relaxed),
            freeze_count: self.freeze_count.load(Ordering::Relaxed),
            qp_avg,
        }
    }
}

pub struct EngineMetrics {
    pub audio: Arc<StreamMetrics>,
    pub video: Arc<StreamMetrics>,
    pub data: Arc<StreamMetrics>,
}

impl EngineMetrics {
    pub fn new() -> Self {
        Self {
            audio: Arc::new(StreamMetrics::new()),
            video: Arc::new(StreamMetrics::new()),
            data: Arc::new(StreamMetrics::new()),
        }
    }

    pub fn total_pps(&self) -> u64 {
        self.audio.packets_received.load(Ordering::Relaxed)
            + self.video.packets_received.load(Ordering::Relaxed)
            + self.data.packets_received.load(Ordering::Relaxed)
    }

    pub fn total_bps_rx(&self) -> u64 {
        self.audio.bytes_received.load(Ordering::Relaxed)
            + self.video.bytes_received.load(Ordering::Relaxed)
            + self.data.bytes_received.load(Ordering::Relaxed)
    }
}

impl Default for EngineMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_metrics_basic() {
        let m = StreamMetrics::new();
        m.record_received(1200);
        m.record_received(1200);
        m.record_sent(1200);
        m.record_dropped();
        m.record_nack();
        let snap = m.snapshot(1_000_000, 500_000);
        assert_eq!(snap.packets_received, 2);
        assert_eq!(snap.packets_sent, 1);
        assert_eq!(snap.packets_dropped, 1);
        assert_eq!(snap.nack_count, 1);
        assert!(snap.loss_fraction > 0.0);
    }

    #[test]
    fn engine_metrics_totals() {
        let em = EngineMetrics::new();
        em.audio.record_received(160);
        em.video.record_received(1200);
        assert_eq!(em.total_pps(), 2);
    }
}

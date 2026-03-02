pub mod gcc;
pub mod aimd;
pub mod probe_controller;
pub mod twcc_aggregator;

pub use gcc::{GccController, GccConfig, BandwidthUsage};
pub use aimd::{AimdController, AimdConfig};
pub use probe_controller::{ProbeController, ProbeConfig, ProbeResult};
pub use twcc_aggregator::{TwccAggregator, PacketArrival, TwccFeedback};

use std::sync::atomic::{AtomicU32, Ordering};

pub trait CongestionController: Send + Sync {
    fn on_packet_sent(&self, size_bytes: usize, send_time_ns: u64);
    fn on_feedback(&self, feedback: &TwccFeedback);
    fn target_bitrate_bps(&self) -> u32;
    fn reset(&self);
}

pub struct CongestionStats {
    pub target_bps: AtomicU32,
    pub probe_bps: AtomicU32,
    pub loss_fraction: AtomicU32,
}

impl CongestionStats {
    pub const fn new() -> Self {
        Self {
            target_bps: AtomicU32::new(0),
            probe_bps: AtomicU32::new(0),
            loss_fraction: AtomicU32::new(0),
        }
    }

    pub fn target_mbps(&self) -> f32 {
        self.target_bps.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }
}

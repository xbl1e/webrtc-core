use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use super::frame::VideoResolution;

#[derive(Clone, Debug)]
pub struct SimulcastLayer {
    pub rid: [u8; 16],
    pub rid_len: usize,
    pub max_bitrate_bps: u32,
    pub max_framerate: u8,
    pub resolution: VideoResolution,
    pub ssrc: u32,
    pub rtx_ssrc: u32,
    pub active: bool,
}

impl SimulcastLayer {
    pub fn new(rid: &str, max_bitrate_bps: u32, width: u32, height: u32, ssrc: u32) -> Self {
        let bytes = rid.as_bytes();
        let len = bytes.len().min(16);
        let mut rid_arr = [0u8; 16];
        rid_arr[..len].copy_from_slice(&bytes[..len]);
        Self {
            rid: rid_arr,
            rid_len: len,
            max_bitrate_bps,
            max_framerate: 30,
            resolution: VideoResolution { width, height },
            ssrc,
            rtx_ssrc: 0,
            active: true,
        }
    }

    pub fn rid_str(&self) -> &str {
        std::str::from_utf8(&self.rid[..self.rid_len]).unwrap_or("")
    }
}

#[derive(Clone, Debug, Default)]
pub struct SimulcastConfig {
    pub layers: Vec<SimulcastLayer>,
}

impl SimulcastConfig {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    pub fn add_layer(&mut self, layer: SimulcastLayer) {
        self.layers.push(layer);
    }

    pub fn standard_3_layer(base_ssrc: u32) -> Self {
        let mut cfg = Self::new();
        cfg.add_layer(SimulcastLayer::new("f", 3_000_000, 1280, 720, base_ssrc));
        cfg.add_layer(SimulcastLayer::new("h", 800_000, 640, 360, base_ssrc + 2));
        cfg.add_layer(SimulcastLayer::new("q", 200_000, 320, 180, base_ssrc + 4));
        cfg
    }

    pub fn layer_by_ssrc(&self, ssrc: u32) -> Option<&SimulcastLayer> {
        self.layers.iter().find(|l| l.ssrc == ssrc)
    }

    pub fn layer_by_rid(&self, rid: &str) -> Option<&SimulcastLayer> {
        self.layers.iter().find(|l| l.rid_str() == rid)
    }
}

pub struct SimulcastStats {
    pub packets_forwarded: AtomicU64,
    pub packets_dropped: AtomicU64,
    pub bytes_forwarded: AtomicU64,
    pub active_layer: AtomicU8,
}

impl SimulcastStats {
    pub const fn new() -> Self {
        Self {
            packets_forwarded: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
            bytes_forwarded: AtomicU64::new(0),
            active_layer: AtomicU8::new(0xFF),
        }
    }
}

pub struct SimulcastSelector {
    config: SimulcastConfig,
    target_layer_idx: AtomicU8,
    available_bw_bps: AtomicU32,
    stats: SimulcastStats,
}

impl SimulcastSelector {
    pub fn new(config: SimulcastConfig) -> Self {
        Self {
            config,
            target_layer_idx: AtomicU8::new(0),
            available_bw_bps: AtomicU32::new(u32::MAX),
            stats: SimulcastStats::new(),
        }
    }

    pub fn update_bandwidth(&self, bps: u32) {
        self.available_bw_bps.store(bps, Ordering::Relaxed);
        self.reselect_layer(bps);
    }

    fn reselect_layer(&self, bps: u32) {
        let mut best_idx = 0u8;
        let mut best_bps = 0u32;
        for (i, layer) in self.config.layers.iter().enumerate() {
            if layer.active && layer.max_bitrate_bps <= bps && layer.max_bitrate_bps >= best_bps {
                best_bps = layer.max_bitrate_bps;
                best_idx = i as u8;
            }
        }
        self.target_layer_idx.store(best_idx, Ordering::Relaxed);
        self.stats.active_layer.store(best_idx, Ordering::Relaxed);
    }

    pub fn should_forward_ssrc(&self, ssrc: u32) -> bool {
        let target = self.target_layer_idx.load(Ordering::Relaxed) as usize;
        if target >= self.config.layers.len() {
            return false;
        }
        self.config.layers[target].ssrc == ssrc
    }

    pub fn forward_packet(&self, ssrc: u32, bytes: u64) -> bool {
        if self.should_forward_ssrc(ssrc) {
            self.stats.packets_forwarded.fetch_add(1, Ordering::Relaxed);
            self.stats.bytes_forwarded.fetch_add(bytes, Ordering::Relaxed);
            true
        } else {
            self.stats.packets_dropped.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    pub fn active_layer(&self) -> Option<&SimulcastLayer> {
        let idx = self.target_layer_idx.load(Ordering::Relaxed) as usize;
        self.config.layers.get(idx)
    }

    pub fn stats_snapshot(&self) -> (u64, u64) {
        (
            self.stats.packets_forwarded.load(Ordering::Relaxed),
            self.stats.packets_dropped.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulcast_config_lookup() {
        let cfg = SimulcastConfig::standard_3_layer(100);
        assert!(cfg.layer_by_rid("f").is_some());
        assert!(cfg.layer_by_rid("h").is_some());
        assert!(cfg.layer_by_rid("q").is_some());
        assert!(cfg.layer_by_rid("x").is_none());
        assert!(cfg.layer_by_ssrc(100).is_some());
        assert!(cfg.layer_by_ssrc(102).is_some());
    }

    #[test]
    fn simulcast_selector_bw_adaptation() {
        let cfg = SimulcastConfig::standard_3_layer(100);
        let sel = SimulcastSelector::new(cfg);
        sel.update_bandwidth(500_000);
        let layer = sel.active_layer().unwrap();
        assert!(layer.max_bitrate_bps <= 500_000);
    }

    #[test]
    fn simulcast_selector_high_bw() {
        let cfg = SimulcastConfig::standard_3_layer(100);
        let sel = SimulcastSelector::new(cfg);
        sel.update_bandwidth(5_000_000);
        let layer = sel.active_layer().unwrap();
        assert_eq!(layer.rid_str(), "f");
    }
}

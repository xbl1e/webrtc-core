use std::sync::atomic::{AtomicU8, Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvcMode {
    L1T1,
    L1T2,
    L1T3,
    L2T1,
    L2T2,
    L2T3,
    L3T1,
    L3T2,
    L3T3,
}

impl SvcMode {
    pub fn spatial_layers(&self) -> u8 {
        match self {
            SvcMode::L1T1 | SvcMode::L1T2 | SvcMode::L1T3 => 1,
            SvcMode::L2T1 | SvcMode::L2T2 | SvcMode::L2T3 => 2,
            SvcMode::L3T1 | SvcMode::L3T2 | SvcMode::L3T3 => 3,
        }
    }

    pub fn temporal_layers(&self) -> u8 {
        match self {
            SvcMode::L1T1 | SvcMode::L2T1 | SvcMode::L3T1 => 1,
            SvcMode::L1T2 | SvcMode::L2T2 | SvcMode::L3T2 => 2,
            SvcMode::L1T3 | SvcMode::L2T3 | SvcMode::L3T3 => 3,
        }
    }

    pub fn total_layers(&self) -> u8 {
        self.spatial_layers() * self.temporal_layers()
    }
}

impl Default for SvcMode {
    fn default() -> Self {
        SvcMode::L1T1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SvcLayer {
    pub spatial: u8,
    pub temporal: u8,
}

impl SvcLayer {
    pub fn new(spatial: u8, temporal: u8) -> Self {
        Self { spatial, temporal }
    }

    pub fn base() -> Self {
        Self { spatial: 0, temporal: 0 }
    }

    pub fn is_base(&self) -> bool {
        self.spatial == 0 && self.temporal == 0
    }

    pub fn dependency_descriptor_tl_idx(&self) -> u8 {
        self.temporal
    }
}

pub struct SvcLayerSelector {
    mode: SvcMode,
    target_spatial: AtomicU8,
    target_temporal: AtomicU8,
    congestion_spatial: AtomicU8,
    congestion_temporal: AtomicU8,
    is_congested: std::sync::atomic::AtomicBool,
}

impl SvcLayerSelector {
    pub fn new(mode: SvcMode) -> Self {
        let s = mode.spatial_layers() - 1;
        let t = mode.temporal_layers() - 1;
        Self {
            mode,
            target_spatial: AtomicU8::new(s),
            target_temporal: AtomicU8::new(t),
            congestion_spatial: AtomicU8::new(0),
            congestion_temporal: AtomicU8::new(0),
            is_congested: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn set_target(&self, spatial: u8, temporal: u8) {
        let max_s = self.mode.spatial_layers() - 1;
        let max_t = self.mode.temporal_layers() - 1;
        self.target_spatial.store(spatial.min(max_s), Ordering::Relaxed);
        self.target_temporal.store(temporal.min(max_t), Ordering::Relaxed);
    }

    pub fn set_congestion_target(&self, spatial: u8, temporal: u8) {
        self.congestion_spatial.store(spatial, Ordering::Relaxed);
        self.congestion_temporal.store(temporal, Ordering::Relaxed);
    }

    pub fn set_congested(&self, congested: bool) {
        self.is_congested.store(congested, Ordering::Release);
    }

    pub fn should_forward(&self, layer: SvcLayer) -> bool {
        let congested = self.is_congested.load(Ordering::Acquire);
        let (max_s, max_t) = if congested {
            (
                self.congestion_spatial.load(Ordering::Relaxed),
                self.congestion_temporal.load(Ordering::Relaxed),
            )
        } else {
            (
                self.target_spatial.load(Ordering::Relaxed),
                self.target_temporal.load(Ordering::Relaxed),
            )
        };
        layer.spatial <= max_s && layer.temporal <= max_t
    }

    pub fn active_mode(&self) -> SvcMode {
        self.mode
    }

    pub fn effective_target(&self) -> SvcLayer {
        let congested = self.is_congested.load(Ordering::Acquire);
        if congested {
            SvcLayer::new(
                self.congestion_spatial.load(Ordering::Relaxed),
                self.congestion_temporal.load(Ordering::Relaxed),
            )
        } else {
            SvcLayer::new(
                self.target_spatial.load(Ordering::Relaxed),
                self.target_temporal.load(Ordering::Relaxed),
            )
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TemporalLayerPattern {
    pub pattern: [u8; 4],
    pub period: u8,
}

impl TemporalLayerPattern {
    pub fn for_mode(mode: SvcMode) -> Self {
        match mode.temporal_layers() {
            1 => Self { pattern: [0, 0, 0, 0], period: 1 },
            2 => Self { pattern: [0, 1, 0, 1], period: 2 },
            _ => Self { pattern: [0, 2, 1, 2], period: 4 },
        }
    }

    pub fn temporal_layer_for_frame(&self, frame_idx: u64) -> u8 {
        self.pattern[(frame_idx % self.period as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svc_mode_layers() {
        assert_eq!(SvcMode::L3T3.spatial_layers(), 3);
        assert_eq!(SvcMode::L3T3.temporal_layers(), 3);
        assert_eq!(SvcMode::L1T3.spatial_layers(), 1);
        assert_eq!(SvcMode::L2T1.temporal_layers(), 1);
    }

    #[test]
    fn layer_selector_congestion() {
        let sel = SvcLayerSelector::new(SvcMode::L3T3);
        sel.set_congestion_target(0, 0);
        sel.set_congested(true);
        assert!(sel.should_forward(SvcLayer::new(0, 0)));
        assert!(!sel.should_forward(SvcLayer::new(1, 0)));
        assert!(!sel.should_forward(SvcLayer::new(0, 1)));
        sel.set_congested(false);
        assert!(sel.should_forward(SvcLayer::new(2, 2)));
    }

    #[test]
    fn temporal_pattern_t3() {
        let pat = TemporalLayerPattern::for_mode(SvcMode::L1T3);
        assert_eq!(pat.temporal_layer_for_frame(0), 0);
        assert_eq!(pat.temporal_layer_for_frame(2), 1);
    }
}

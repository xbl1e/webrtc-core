use std::sync::atomic::{AtomicI64, Ordering};
pub struct ClockDriftEstimator {
    offset_fp: AtomicI64,
}

impl ClockDriftEstimator {
    pub const fn new() -> Self {
        Self {
            offset_fp: AtomicI64::new(0),
        }
    }

    pub fn update(&self, rtp_ts: u32, sample_rate: u32, arrival_ns: u64) {
        let ts_ns = (rtp_ts as i128) * 1_000_000_000i128 / (sample_rate as i128);
        let sample_offset = (arrival_ns as i128) - ts_ns;
        let alpha_num: i128 = 1;
        let alpha_den: i128 = 16;
        let sample_fp = sample_offset as i128;
        loop {
            let old = self.offset_fp.load(Ordering::Acquire) as i128;
            let new = ((old * (alpha_den - alpha_num)) + (sample_fp * alpha_num)) / alpha_den;
            if self
                .offset_fp
                .compare_exchange(old as i64, new as i64, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
    }

    pub fn offset_ns(&self) -> i64 {
        self.offset_fp.load(Ordering::Relaxed)
    }
}

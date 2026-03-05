use crate::{
    index_ring::IndexRing,
    jitter_buffer::AudioJitterBuffer,
    latency_ring::LatencyRing,
    rtcp_queue::RtcpSendQueue,
    session::SessionState,
    slab::{SlabAllocator, SlabKey},

};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
};
use std::thread;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct EngineStats {
    pub pps: AtomicU64,
    pub dropped_packets: AtomicU64,
    pub latency_p99: AtomicU64,
    pub dropped_layer0: AtomicU64,
    pub dropped_layer1: AtomicU64,
    pub dropped_layer2: AtomicU64,
}

impl EngineStats {
    pub const fn new() -> Self {
        Self {
            pps: AtomicU64::new(0),
            dropped_packets: AtomicU64::new(0),
            latency_p99: AtomicU64::new(0),
            dropped_layer0: AtomicU64::new(0),
            dropped_layer1: AtomicU64::new(0),
            dropped_layer2: AtomicU64::new(0),
        }
    }
}

pub struct EngineShard {
    slab: Arc<SlabAllocator>,
    idx: Arc<IndexRing>,
    jitter: Mutex<AudioJitterBuffer>,
    latency: Arc<LatencyRing>,
    rtcp: Arc<RtcpSendQueue>,
    session: Arc<RwLock<SessionState>>,
    stats: Arc<EngineStats>,
    stop: Arc<AtomicBool>,
    congestion: Arc<AtomicBool>,
}

impl EngineShard {
    pub fn new(
        slab: Arc<SlabAllocator>,
        idx: Arc<IndexRing>,
        jitter_capacity: usize,
        rtcp: Arc<RtcpSendQueue>,
        session: Arc<RwLock<SessionState>>,
    ) -> Self {
        Self {
            slab,
            idx,
            jitter: Mutex::new(AudioJitterBuffer::new(jitter_capacity)),
            latency: Arc::new(LatencyRing::new(8192)),
            rtcp,
            session,
            stats: Arc::new(EngineStats::new()),

            stop: Arc::new(AtomicBool::new(false)),
            congestion: Arc::new(AtomicBool::new(false)),
        }
    }

    fn now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64
    }

    pub fn start(self) -> Arc<Self> {
        let _stop = self.stop.clone();
        let s = Arc::new(self);
        let s_thread = s.clone();
        let _ = thread::spawn(move || {
            while !s_thread.stop.load(Ordering::Acquire) {
                while let Some(slot) = s_thread.idx.pop() {
                    let arrival = Self::now_ns();
                    s_thread.handle_incoming_index(slot, arrival);
                }
                s_thread.poll_step();
                thread::sleep(Duration::from_millis(5));
            }
            loop {
                if s_thread.idx.pop().is_none() {
                    break;
                }
            }
        });
        s
    }

    pub fn enqueue_index(&self, idx: SlabKey) -> bool {
        self.idx.push(idx.index())
    }

    pub fn set_congestion(&self, v: bool) {
        self.congestion.store(v, Ordering::Release);
    }

    pub fn stats_snapshot(&self) -> (u64, u64, u64) {
        (
            self.stats.pps.load(Ordering::Relaxed),
            self.stats.latency_p99.load(Ordering::Relaxed),
            self.stats.dropped_packets.load(Ordering::Relaxed),
        )
    }

    pub fn get_ewma_delay_ms(&self) -> u64 {
        let jb = self.jitter.lock().ok();
        if let Some(jb) = jb {
            jb.get_ewma_delay_ms()
        } else {
            0
        }
    }

    pub fn is_congested(&self) -> bool {
        self.congestion.load(Ordering::Acquire)
    }

    pub fn get_dropped_layers(&self) -> u64 {
        self.stats.dropped_layer1.load(Ordering::Relaxed) + self.stats.dropped_layer2.load(Ordering::Relaxed)
    }

    fn handle_incoming_index(&self, slot_key: SlabKey, arrival: u64) {
        self.stats.pps.fetch_add(1, Ordering::Relaxed);
        let jb = self.jitter.lock().ok();
        if let Some(jb) = jb {
            let enq = jb.push_index_with_seq(slot_key, arrival, &self.slab);
            if !enq {
                self.stats.dropped_packets.fetch_add(1, Ordering::Relaxed);
                self.slab.free(slot_key);
            }
        } else {
            self.slab.free(slot_key);
        }
    }

    fn poll_step(&self) {
        loop {
            let maybe = {
                let jb = self.jitter.lock().ok();
                jb.and_then(|j| j.pop_index())
            };
            match maybe {
                Some(slot_key) => {
                    let pkt = if let Some(p) = self.slab.get_mut(&slot_key) { p } else { continue };
                    let now_ns = Self::now_ns();
                    let latency = now_ns.saturating_sub(pkt.timestamp);
                    let _ = self.latency.push(latency);
                    let cur_delay_ms = {
                        let jb = self.jitter.lock().ok();
                        jb.map(|j| j.get_ewma_delay_ms()).unwrap_or(0)
                    };
                    if cur_delay_ms > 50 {
                        self.congestion.store(true, Ordering::Release);
                    } else {
                        self.congestion.store(false, Ordering::Release);
                    }
                    if self.congestion.load(Ordering::Acquire) && pkt.layer > 0 {
                        self.slab.free(slot_key);
                        self.stats.dropped_packets.fetch_add(1, Ordering::Relaxed);
                        match pkt.layer {
                            0 => { self.stats.dropped_layer0.fetch_add(1, Ordering::Relaxed); }
                            1 => { self.stats.dropped_layer1.fetch_add(1, Ordering::Relaxed); }
                            2 => { self.stats.dropped_layer2.fetch_add(1, Ordering::Relaxed); }
                            _ => { self.stats.dropped_layer2.fetch_add(1, Ordering::Relaxed); }
                        }
                        continue;
                    }
                    let maybe_srtp = {
                        let s = self.session.read().ok();
                        s.and_then(|session| session.srtp())
                    };
                    if let Some(srtp) = maybe_srtp {
                        let nonce = [0u8;12];
                        let _ = srtp.protect_index_inplace(&self.slab, &slot_key, &nonce, &[]);
                    }
                    self.slab.free(slot_key);
                }
                None => break,
            }
        }
        self.emit_nack_if_needed();
        self.update_latency_p99();
        self.maybe_resize_jitter();
    }

    fn maybe_resize_jitter(&self) {
        let jitter_ms = {
            let jb = self.jitter.lock().ok();
            jb.map(|j| j.get_ewma_delay_ms() as usize).unwrap_or(0)
        };
        let target = jitter_ms.saturating_mul(2).max(16);
        let mut jb = self.jitter.lock().ok();
        if let Some(jb) = jb.as_mut() {
            let cur = jb.capacity();
            if target != cur && target > 0 && target < 65536 {
                *jb = AudioJitterBuffer::new(target);
            }
        }
    }

    fn emit_nack_if_needed(&self) {
        let now_ns = Self::now_ns();
        let jb_snapshot = {
            let jb = self.jitter.lock().ok();
            jb.map(|j| j.check_and_emit_nack(now_ns)).unwrap_or(false)
        };
        if jb_snapshot {
            let mut buf = [0u8;512];
            let n = {
                let jb = self.jitter.lock().ok();
                jb.and_then(|j| crate::rtcp::RtcpFeedback::write_nack_into(&j, &self.slab, &mut buf))
                    .unwrap_or(0)
            };
            if n > 0 { let _ = self.rtcp.push_drop_oldest(&buf[..n]); }
        }
    }

    fn update_latency_p99(&self) {
        let mut buf = vec![0u64; 8192];
        let n = self.latency.pop_batch(&mut buf);
        if n == 0 { return }
        buf.truncate(n);
        buf.sort_unstable();
        let idx = ((n as f64) * 0.99).ceil() as usize;
        let ix = idx.saturating_sub(1).min(n.saturating_sub(1));
        let p99 = buf[ix];
        self.stats.latency_p99.store(p99, Ordering::Relaxed);
    }
}

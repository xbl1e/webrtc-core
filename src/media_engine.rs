use crate::index_ring::IndexRing;
use crate::jitter_buffer::AudioJitterBuffer;
use crate::latency_ring::LatencyRing;
use crate::rtcp_queue::RtcpSendQueue;
use crate::session::SessionState;
use crate::slab::SlabAllocator;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;

pub struct Metrics {
    pub packets_received: AtomicU64,
    pub packets_dropped: AtomicU64,
    pub jitter_samples: AtomicU64,
}

impl Metrics {
    pub const fn new() -> Self {
        Self {
            packets_received: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
            jitter_samples: AtomicU64::new(0),
        }
    }
}

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("invalid state transition")]
    InvalidState,
}

pub enum Idle {}
pub enum Running {}
pub enum Closed {}

pub struct MediaEngine<S> {
    pub metrics: Arc<Metrics>,
    jitter: AudioJitterBuffer,
    slab: Arc<SlabAllocator>,
    idx_ring: Arc<IndexRing>,
    rtcp_queue: Arc<RtcpSendQueue>,
    latency_ring: Arc<LatencyRing>,
    session: Arc<std::sync::RwLock<SessionState>>,
    state_marker: std::marker::PhantomData<S>,
}

impl MediaEngine<Idle> {
    pub fn new(jitter_capacity: usize) -> Self {
        Self {
            metrics: Arc::new(Metrics::new()),
            jitter: AudioJitterBuffer::new(jitter_capacity),
            slab: Arc::new(SlabAllocator::new(4096)),
            idx_ring: Arc::new(IndexRing::new(4096)),
            rtcp_queue: Arc::new(RtcpSendQueue::new(1024)),
            latency_ring: Arc::new(LatencyRing::new(8192)),
            session: Arc::new(std::sync::RwLock::new(SessionState::new())),
            state_marker: std::marker::PhantomData,
        }
    }

    pub fn from_components(
        metrics: Arc<Metrics>,
        jitter: AudioJitterBuffer,
        slab: Arc<SlabAllocator>,
        idx_ring: Arc<IndexRing>,
        rtcp_queue: Arc<RtcpSendQueue>,
        latency_ring: Arc<LatencyRing>,
        session: Arc<std::sync::RwLock<SessionState>>,
    ) -> Self {
        Self {
            metrics,
            jitter,
            slab,
            idx_ring,
            rtcp_queue,
            latency_ring,
            session,
            state_marker: std::marker::PhantomData,
        }
    }

    pub fn start(self) -> MediaEngine<Running> {
        MediaEngine {
            metrics: self.metrics,
            jitter: self.jitter,
            slab: self.slab,
            idx_ring: self.idx_ring,
            rtcp_queue: self.rtcp_queue,
            latency_ring: self.latency_ring,
            session: self.session,
            state_marker: std::marker::PhantomData,
        }
    }
}

impl MediaEngine<Running> {
    pub fn poll_step(&mut self) {
        loop {
            match self.jitter.pop_index() {
                Some(slot_idx) => {
                    let pkt = unsafe { self.slab.get_mut(slot_idx) };
                    self.metrics
                        .packets_received
                        .fetch_add(1, Ordering::Relaxed);
                    let now_ns = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64;
                    let latency = now_ns.saturating_sub(pkt.timestamp);
                    let _ = self.latency_ring.push(latency);
                    let maybe_srtp = {
                        let r = self.session.read().unwrap();
                        r.srtp()
                    };
                    if let Some(srtp) = maybe_srtp {
                        let nonce = [0u8; 12];
                        let aad = &[] as &[u8];
                        let _ = srtp.protect_index_inplace(&self.slab, slot_idx, &nonce, aad);
                    }
                    self.slab.free(slot_idx);
                }
                None => break,
            }
        }
        let now_u = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        if self.jitter.check_and_emit_nack(now_u) {
            let mut buf = [0u8; 512];
            let n = crate::rtcp::RtcpFeedback::write_nack_into(&self.jitter, &self.slab, &mut buf);
            if n > 0 {
                let _ = self.rtcp_queue.push_drop_oldest(&buf[..n]);
            }
        }
    }

    pub fn handle_incoming_index(&self, slot_idx: usize, arrival: u64) {
        self.metrics
            .packets_received
            .fetch_add(1, Ordering::Relaxed);
        let enq = self
            .jitter
            .push_index_with_seq(slot_idx, arrival, &self.slab);
        if !enq {
            self.metrics.packets_dropped.fetch_add(1, Ordering::Relaxed);
            self.slab.free(slot_idx);
        }
    }

    pub fn stop(self) -> MediaEngine<Closed> {
        MediaEngine {
            metrics: self.metrics,
            jitter: self.jitter,
            slab: self.slab,
            idx_ring: self.idx_ring,
            rtcp_queue: self.rtcp_queue,
            latency_ring: self.latency_ring,
            session: self.session,
            state_marker: std::marker::PhantomData,
        }
    }
}

impl MediaEngine<Closed> {}

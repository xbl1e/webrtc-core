use crate::{
    index_ring::IndexRing, rtcp_queue::RtcpSendQueue, session::SessionState, slab::{SlabAllocator, SlabKey},
    engine_shard::EngineShard,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct EngineBuilder {
    jitter_capacity: usize,
    slab_capacity: usize,
    index_capacity: usize,
    rtcp_capacity: usize,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            jitter_capacity: 1024,
            slab_capacity: 4096,
            index_capacity: 4096,
            rtcp_capacity: 256,
        }
    }
    pub fn jitter_capacity(mut self, v: usize) -> Self {
        self.jitter_capacity = v;
        self
    }
    pub fn slab_capacity(mut self, v: usize) -> Self {
        self.slab_capacity = v;
        self
    }
    pub fn index_capacity(mut self, v: usize) -> Self {
        self.index_capacity = v;
        self
    }
    pub fn rtcp_capacity(mut self, v: usize) -> Self {
        self.rtcp_capacity = v;
        self
    }
    pub fn build(self) -> EngineHandle {
        let slab = Arc::new(SlabAllocator::new(self.slab_capacity));
        let idx = Arc::new(IndexRing::new(self.index_capacity));
        let rtcp = Arc::new(RtcpSendQueue::new(self.rtcp_capacity));
        let latency = Arc::new(crate::latency_ring::LatencyRing::new(8192));
        let session = Arc::new(std::sync::RwLock::new(SessionState::new()));
        let _shard = EngineShard::new(slab.clone(), idx.clone(), self.jitter_capacity, rtcp.clone(), session.clone()).start();
        EngineHandle {
            slab,
            idx_ring: idx,
            rtcp_queue: rtcp,
            session,
            media_handle: None,
            stop: Arc::new(AtomicBool::new(false)),
            latency_ring: latency,
        }
    }
}

pub struct EngineHandle {
    slab: Arc<SlabAllocator>,
    idx_ring: Arc<IndexRing>,
    rtcp_queue: Arc<RtcpSendQueue>,
    session: Arc<std::sync::RwLock<SessionState>>,
    media_handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    latency_ring: Arc<crate::latency_ring::LatencyRing>,
}

impl EngineHandle {
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    pub fn feed_packet(&self, payload: &[u8], seq: u16, ssrc: u32) -> Result<(), ()> {
        if let Some(guard) = SlabAllocator::allocate_guard(&self.slab) {
            if let Some(p) = guard.get_mut() {
                let len = payload.len().min(p.data.len());
                p.data[..len].copy_from_slice(&payload[..len]);
                p.len = len;
                p.seq = seq;
                p.timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_nanos() as u64;
                p.ssrc = ssrc;
            }
            let key = guard.into_key();
            if !self.idx_ring.push(key.index()) {
                self.slab.free(key);
                return Err(());
            }
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn provide_keying_material(&self, key: [u8; 32]) {
        if let Ok(mut s) = self.session.write() {
            let old = std::mem::replace(&mut *s, SessionState::new());
            *s = old.protect(&key);
        }
    }

    pub fn start_rtcp_sender(
        &self,
        socket: std::sync::Arc<tokio::net::UdpSocket>,
        peer: std::net::SocketAddr,
    ) -> tokio::task::JoinHandle<()> {
        let q = self.rtcp_queue.clone();
        let sock = socket.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            loop {
                if let Some(n) = q.pop(&mut buf) {
                    let send = sock.send_to(&buf[..n], peer);
                    let t = tokio::time::timeout(Duration::from_millis(10), send).await;
                    let _ = t;
                } else {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        })
    }

    pub fn start_latency_monitor(&self) -> std::thread::JoinHandle<()> {
        let ring = self.latency_ring.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u64; 65536];
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let n = ring.pop_batch(&mut buf);
                if n == 0 {
                    continue;
                }
                let mut s = buf[..n].to_vec();
                s.sort_unstable();
                let idx = ((n as f64) * 0.99).ceil() as usize;
                let idx = idx.saturating_sub(1).min(n.saturating_sub(1));
                let p99 = s[idx];
            }
        })
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.media_handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_basic_feed() {
        let handle = EngineHandle::builder().build();
        let payload = vec![0u8; 160];
        assert!(handle.feed_packet(&payload, 1u16, 0x1234).is_ok());
    }

    #[test]
    fn engine_custom_capacity() {
        let handle = EngineHandle::builder()
            .jitter_capacity(2048)
            .slab_capacity(8192)
            .index_capacity(8192)
            .rtcp_capacity(512)
            .build();
        let payload = vec![0u8; 160];
        assert!(handle.feed_packet(&payload, 1u16, 0x1234).is_ok());
    }
}

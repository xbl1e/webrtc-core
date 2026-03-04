//! Pacer (packet pacing) for rate-limited RTP transmission.
//!
//! Implements a token bucket-based pacer to smooth packet transmission
//! and prevent bursts that can cause network congestion.

use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::{Mutex, RwLock};
use std::collections::VecDeque;
#[derive(Clone)]
pub struct PacedPacket {
    pub data: Arc<Vec<u8>>,
    pub destination: std::net::SocketAddr,
    pub enqueue_time: Instant,
    pub priority: PacketPriority,
    pub ssrc: u32,
    pub seq: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PacketPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}
#[derive(Clone, Debug)]
pub struct PacerConfig {
    pub target_bitrate_bps: u32,
    pub max_burst_bytes: u32,
    pub queue_size: usize,
    pub wakeup_interval_ms: u32,
}

impl Default for PacerConfig {
    fn default() -> Self {
        Self {
            target_bitrate_bps: 3_000_000, // 3 Mbps default
            max_burst_bytes: 64 * 1024,    // 64KB burst
            queue_size: 2000,
            wakeup_interval_ms: 5,
        }
    }
}
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            max_tokens: capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self, bytes: u32) -> bool {
        self.refill();
        let needed = bytes as f64;
        if self.tokens >= needed {
            self.tokens -= needed;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    fn set_rate(&mut self, bps: u32) {
        self.refill_rate = bps as f64 / 8.0; // Convert to bytes/sec
    }

    fn available(&self) -> u32 {
        self.tokens as u32
    }
}
pub struct Pacer {
    config: PacerConfig,
    bucket: Mutex<TokenBucket>,
    queue: RwLock<VecDeque<PacedPacket>>,
    stats: PacerStats,
}

#[derive(Clone, Debug, Default)]
pub struct PacerStats {
    pub packets_queued: u64,
    pub packets_sent: u64,
    pub packets_dropped: u64,
    pub current_queue_size: usize,
    pub token_available: u32,
}

impl Pacer {
    pub fn new(config: PacerConfig) -> Self {
        let max_tokens = config.max_burst_bytes as f64;
        let refill_rate = config.target_bitrate_bps as f64 / 8.0;

        Self {
            config: config.clone(),
            bucket: Mutex::new(TokenBucket::new(max_tokens, refill_rate)),
            queue: RwLock::new(VecDeque::new()),
            stats: PacerStats::default(),
        }
    }

    pub fn enqueue(&self, packet: PacedPacket) -> bool {
        let mut queue = self.queue.write();

        // Check queue size
        if queue.len() >= self.config.queue_size {
            self.stats.packets_dropped += 1;
            return false;
        }

        // Insert in priority order
        let inserted = queue.iter()
            .position(|p| p.priority < packet.priority)
            .map(|pos| {
                queue.insert(pos, packet.clone());
                true
            })
            .unwrap_or_else(|| {
                queue.push_back(packet.clone());
                true
            });

        if inserted {
            self.stats.packets_queued += 1;
            self.stats.current_queue_size = queue.len();
        }

        inserted
    }

    pub fn try_send(&self) -> Option<PacedPacket> {
        let mut queue = self.queue.write();

        // Find a packet we can send
        let packet = queue.pop_front()?;

        let mut bucket = self.bucket.lock();
        let packet_len = packet.data.len() as u32;

        if bucket.try_consume(packet_len) {
            self.stats.packets_sent += 1;
            self.stats.current_queue_size = queue.len();
            self.stats.token_available = bucket.available();
            Some(packet)
        } else {
            // Put it back at the front (highest priority)
            queue.push_front(packet);
            None
        }
    }

    pub fn set_target_bitrate(&self, bps: u32) {
        let mut bucket = self.bucket.lock();
        bucket.set_rate(bps);
    }

    pub fn stats(&self) -> PacerStats {
        self.stats.clone()
    }

    pub fn queue_size(&self) -> usize {
        self.queue.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacer_basic_enqueue() {
        let pacer = Pacer::default();

        let packet = PacedPacket {
            data: Arc::new(vec![0u8; 100]),
            destination: "127.0.0.1:5000".parse().unwrap(),
            enqueue_time: Instant::now(),
            priority: PacketPriority::Normal,
            ssrc: 1,
            seq: 100,
        };

        assert!(pacer.enqueue(packet));
        assert_eq!(pacer.queue_size(), 1);
    }

    #[test]
    fn pacer_priority_ordering() {
        let pacer = Pacer::default();

        // Enqueue low priority first
        let low = PacedPacket {
            data: Arc::new(vec![0u8; 100]),
            destination: "127.0.0.1:5000".parse().unwrap(),
            enqueue_time: Instant::now(),
            priority: PacketPriority::Low,
            ssrc: 1,
            seq: 100,
        };

        // Then high priority
        let high = PacedPacket {
            data: Arc::new(vec![0u8; 100]),
            destination: "127.0.0.1:5000".parse().unwrap(),
            enqueue_time: Instant::now(),
            priority: PacketPriority::High,
            ssrc: 1,
            seq: 101,
        };

        pacer.enqueue(low);
        pacer.enqueue(high);

        // High should come first
        if let Some(p) = pacer.try_send() {
            assert_eq!(p.priority, PacketPriority::High);
        }
    }
}

use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};

const TWCC_WINDOW: usize = 256;

#[derive(Clone, Copy, Debug, Default)]
pub struct PacketArrival {
    pub transport_seq: u16,
    pub send_time_ns: u64,
    pub recv_time_ns: u64,
    pub size_bytes: u16,
    pub received: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TwccFeedback {
    pub sent_count: u64,
    pub received_count: u64,
    pub inter_arrival_delta_ns: i64,
    pub inter_departure_delta_ns: i64,
}

struct ArrivalSlot {
    arrival: UnsafeCell<PacketArrival>,
    valid: std::sync::atomic::AtomicBool,
}

unsafe impl Send for ArrivalSlot {}
unsafe impl Sync for ArrivalSlot {}

impl ArrivalSlot {
    fn new() -> Self {
        Self {
            arrival: UnsafeCell::new(PacketArrival::default()),
            valid: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

pub struct TwccAggregator {
    slots: Box<[ArrivalSlot]>,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    seq: CachePadded<AtomicU16>,
    last_sent_ns: AtomicU64,
    last_recv_ns: AtomicU64,
    total_sent: AtomicU64,
    total_received: AtomicU64,
}

unsafe impl Send for TwccAggregator {}
unsafe impl Sync for TwccAggregator {}

impl TwccAggregator {
    pub fn new() -> Self {
        let cap = TWCC_WINDOW;
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(ArrivalSlot::new());
        }
        Self {
            slots: slots.into_boxed_slice(),
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            seq: CachePadded::new(AtomicU16::new(0)),
            last_sent_ns: AtomicU64::new(0),
            last_recv_ns: AtomicU64::new(0),
            total_sent: AtomicU64::new(0),
            total_received: AtomicU64::new(0),
        }
    }

    pub fn next_transport_seq(&self) -> u16 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    pub fn on_packet_sent(&self, transport_seq: u16, send_time_ns: u64, size_bytes: usize) {
        let idx = (transport_seq as usize) & self.mask;
        let slot = &self.slots[idx];
        let arrival = PacketArrival {
            transport_seq,
            send_time_ns,
            recv_time_ns: 0,
            size_bytes: size_bytes.min(65535) as u16,
            received: false,
        };
        unsafe { *slot.arrival.get() = arrival; }
        slot.valid.store(true, Ordering::Release);
        self.total_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_packet_received(&self, transport_seq: u16, recv_time_ns: u64) {
        let idx = (transport_seq as usize) & self.mask;
        let slot = &self.slots[idx];
        if slot.valid.load(Ordering::Acquire) {
            unsafe {
                let a = &mut *slot.arrival.get();
                if a.transport_seq == transport_seq {
                    a.recv_time_ns = recv_time_ns;
                    a.received = true;
                }
            }
            self.total_received.fetch_add(1, Ordering::Relaxed);
            self.last_recv_ns.store(recv_time_ns, Ordering::Relaxed);
        }
    }

    pub fn compute_feedback(&self) -> TwccFeedback {
        let sent = self.total_sent.load(Ordering::Relaxed);
        let received = self.total_received.load(Ordering::Relaxed);

        let mut last_send = 0u64;
        let mut prev_send = 0u64;
        let mut last_recv = 0u64;
        let mut prev_recv = 0u64;
        let mut found_last = false;
        let mut found_prev = false;

        let head = self.head.load(Ordering::Relaxed);
        for i in 0..TWCC_WINDOW {
            let idx = (head + i) & self.mask;
            let slot = &self.slots[idx];
            if !slot.valid.load(Ordering::Acquire) {
                continue;
            }
            let a = unsafe { &*slot.arrival.get() };
            if a.received {
                if !found_last {
                    last_send = a.send_time_ns;
                    last_recv = a.recv_time_ns;
                    found_last = true;
                } else if !found_prev {
                    prev_send = a.send_time_ns;
                    prev_recv = a.recv_time_ns;
                    found_prev = true;
                }
            }
        }

        let inter_departure = if found_prev {
            last_send.wrapping_sub(prev_send) as i64
        } else {
            0
        };
        let inter_arrival = if found_prev {
            last_recv.wrapping_sub(prev_recv) as i64
        } else {
            0
        };

        TwccFeedback {
            sent_count: sent,
            received_count: received,
            inter_arrival_delta_ns: inter_arrival,
            inter_departure_delta_ns: inter_departure,
        }
    }

    pub fn write_feedback_into(&self, sender_ssrc: u32, media_ssrc: u32, out: &mut [u8]) -> usize {
        let fb = self.compute_feedback();
        if out.len() < 24 {
            return 0;
        }
        out[0] = 0x8f;
        out[1] = 0xcd;
        out[2..4].copy_from_slice(&5u16.to_be_bytes());
        out[4..8].copy_from_slice(&sender_ssrc.to_be_bytes());
        out[8..12].copy_from_slice(&media_ssrc.to_be_bytes());
        out[12..16].copy_from_slice(&(fb.received_count as u32).to_be_bytes());
        out[16..20].copy_from_slice(&(fb.sent_count as u32).to_be_bytes());
        out[20..24].copy_from_slice(&(fb.inter_arrival_delta_ns as i32 as u32).to_be_bytes());
        24
    }

    pub fn total_sent(&self) -> u64 {
        self.total_sent.load(Ordering::Relaxed)
    }

    pub fn total_received(&self) -> u64 {
        self.total_received.load(Ordering::Relaxed)
    }

    pub fn loss_fraction(&self) -> f32 {
        let s = self.total_sent.load(Ordering::Relaxed);
        let r = self.total_received.load(Ordering::Relaxed);
        if s == 0 { return 0.0; }
        let lost = s.saturating_sub(r);
        lost as f32 / s as f32
    }
}

impl Default for TwccAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twcc_sequence_numbers() {
        let agg = TwccAggregator::new();
        let s1 = agg.next_transport_seq();
        let s2 = agg.next_transport_seq();
        assert_eq!(s2, s1.wrapping_add(1));
    }

    #[test]
    fn twcc_loss_fraction() {
        let agg = TwccAggregator::new();
        for i in 0..10u16 {
            agg.on_packet_sent(i, 1_000_000 * i as u64, 1200);
        }
        for i in 0..8u16 {
            agg.on_packet_received(i, 2_000_000 * i as u64);
        }
        let loss = agg.loss_fraction();
        assert!((loss - 0.2).abs() < 0.05);
    }

    #[test]
    fn twcc_feedback_counts() {
        let agg = TwccAggregator::new();
        agg.on_packet_sent(0, 1000, 1200);
        agg.on_packet_sent(1, 2000, 1200);
        agg.on_packet_received(0, 1500);
        agg.on_packet_received(1, 2500);
        let fb = agg.compute_feedback();
        assert_eq!(fb.sent_count, 2);
        assert_eq!(fb.received_count, 2);
    }
}

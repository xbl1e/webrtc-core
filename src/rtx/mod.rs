//! RTX (Retransmission) handling for RTP.
//!
//! Implements RTX as per RFC 4588 for packet retransmission.
//! Handles NACK-to-RTX conversion and RTX packet creation.

use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::RwLock;
#[derive(Clone, Copy, Debug)]
pub struct RtxNackEntry {
    pub seq: u16,
    pub send_time: std::time::Instant,
    pub retransmit_count: u8,
}
pub struct PacketHistory {
    entries: RwLock<VecDeque<RtxNackEntry>>,
    max_size: usize,
    seq_to_ssrc: RwLock<std::collections::HashMap<u16, (u32, u16)>>,
}

impl PacketHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: RwLock::new(VecDeque::with_capacity(max_size)),
            max_size,
            seq_to_ssrc: RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn store_packet(&self, seq: u16, ssrc: u16) {
        let mut entries = self.entries.write();

        // Remove old entries if at capacity
        while entries.len() >= self.max_size {
            if let Some(old) = entries.pop_front() {
                let mut map = self.seq_to_ssrc.write();
                map.remove(&old.seq);
            }
        }

        entries.push_back(RtxNackEntry {
            seq,
            send_time: std::time::Instant::now(),
            retransmit_count: 0,
        });

        let mut map = self.seq_to_ssrc.write();
        map.insert(seq, (ssrc as u32, seq));
    }

    pub fn get_packet(&self, seq: u16) -> Option<RtxNackEntry> {
        let entries = self.entries.read();
        entries.iter().find(|e| e.seq == seq).cloned()
    }

    pub fn increment_retransmit(&self, seq: u16) -> bool {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.iter_mut().find(|e| e.seq == seq) {
            entry.retransmit_count += 1;
            return entry.retransmit_count < 3; // Max 3 retransmits
        }
        false
    }

    pub fn get_missing(&self, received: &[u16]) -> Vec<u16> {
        let entries = self.entries.read();
        let mut missing = Vec::new();

        for entry in entries.iter() {
            let is_missing = received.iter().all(|&r| r != entry.seq);
            if is_missing && entry.retransmit_count < 3 {
                missing.push(entry.seq);
            }
        }

        missing
    }
}
#[derive(Clone, Debug)]
pub struct RtxConfig {
    pub rtx_ssrc: u32,
    pub rtx_payload_type: u8,
    pub original_ssrc: u32,
    pub max_retransmits: u8,
    pub rtx_time_ms: u32,
}

impl Default for RtxConfig {
    fn default() -> Self {
        Self {
            rtx_ssrc: 0,
            rtx_payload_type: 0,
            original_ssrc: 0,
            max_retransmits: 3,
            rtx_time_ms: 1000, // RTX time in ms (per RFC 4588)
        }
    }
}
pub struct RtxSender {
    config: RtxConfig,
    packet_history: Arc<PacketHistory>,
}

impl RtxSender {
    pub fn new(config: RtxConfig) -> Self {
        Self {
            config,
            packet_history: Arc::new(PacketHistory::new(1000)),
        }
    }

    pub fn on_rtp_sent(&self, seq: u16, ssrc: u16) {
        self.packet_history.store_packet(seq, ssrc);
    }

    pub fn on_nack(&self, nack_list: &[u16]) -> Vec<PacketToRetransmit> {
        let mut to_retransmit = Vec::new();

        for &seq in nack_list {
            if let Some(entry) = self.packet_history.get_packet(seq) {
                if self.packet_history.increment_retransmit(seq) {
                    to_retransmit.push(PacketToRetransmit {
                        seq,
                        original_ssrc: self.config.original_ssrc,
                        retransmit_count: entry.retransmit_count,
                    });
                }
            }
        }

        to_retransmit
    }

    pub fn packet_history(&self) -> &Arc<PacketHistory> {
        &self.packet_history
    }
}
#[derive(Clone, Debug)]
pub struct PacketToRetransmit {
    pub seq: u16,
    pub original_ssrc: u32,
    pub retransmit_count: u8,
}
pub struct RtxReceiver {
    config: RtxConfig,
    rtcp_feedback: RwLock<Vec<NackFeedback>>,
}

impl RtxReceiver {
    pub fn new(config: RtxConfig) -> Self {
        Self {
            config,
            rtcp_feedback: RwLock::new(Vec::new()),
        }
    }

    pub fn receive_rtx_packet(&self, seq: u16, rtx_payload: &[u8]) -> Option<RtxRecoveredPacket> {
        // Recover the original RTP packet from RTX
        // In real implementation, would reconstruct from RTX payload
        Some(RtxRecoveredPacket {
            seq,
            original_ssrc: self.config.original_ssrc,
            payload: rtx_payload.to_vec(),
        })
    }

    pub fn record_feedback(&self, feedback: NackFeedback) {
        self.rtcp_feedback.write().push(feedback);
    }

    pub fn take_feedback(&self) -> Vec<NackFeedback> {
        std::mem::take(&mut *self.rtcp_feedback.write())
    }
}
#[derive(Clone, Debug)]
pub struct NackFeedback {
    pub seq: u16,
    pub send_time: std::time::Instant,
}
#[derive(Clone, Debug)]
pub struct RtxRecoveredPacket {
    pub seq: u16,
    pub original_ssrc: u32,
    pub payload: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_history_basic() {
        let history = PacketHistory::new(10);

        history.store_packet(100, 1);
        history.store_packet(101, 1);

        let entry = history.get_packet(100);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().seq, 100);
    }

    #[test]
    fn rtx_sender_nack() {
        let config = RtxConfig {
            original_ssrc: 0x12345678,
            ..Default::default()
        };
        let sender = RtxSender::new(config);

        // Record some sent packets
        sender.on_rtp_sent(100, 1);
        sender.on_rtp_sent(101, 1);

        // Get NACK for missing packets
        let to_retransmit = sender.on_nack(&[100, 101]);
        assert_eq!(to_retransmit.len(), 2);
    }
}

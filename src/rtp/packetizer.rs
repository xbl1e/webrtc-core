use super::header::{RtpHeader, CsrcList};
use super::packet::{MediaPacket, PacketType};

#[derive(Clone, Debug)]
pub struct PacketizerConfig {
    pub payload_type: u8,
    pub ssrc: u32,
    pub clock_rate: u32,
    pub mtu: usize,
    pub initial_seq: u16,
    pub initial_timestamp: u32,
}

impl Default for PacketizerConfig {
    fn default() -> Self {
        Self {
            payload_type: 111,
            ssrc: 0,
            clock_rate: 90000,
            mtu: 1200,
            initial_seq: 0,
            initial_timestamp: 0,
        }
    }
}

pub struct Packetizer {
    cfg: PacketizerConfig,
    seq: u16,
    timestamp: u32,
}

impl Packetizer {
    pub fn new(cfg: PacketizerConfig) -> Self {
        let seq = cfg.initial_seq;
        let timestamp = cfg.initial_timestamp;
        Self { cfg, seq, timestamp }
    }

    pub fn packetize<'a>(
        &mut self,
        payload: &[u8],
        duration_samples: u32,
        kind: PacketType,
        out: &'a mut [MediaPacket],
    ) -> usize {
        let max_payload = self.cfg.mtu.saturating_sub(12);
        let chunks = (payload.len() + max_payload - 1).max(1) / max_payload;
        let mut written = 0usize;
        let mut offset = 0usize;

        for chunk_idx in 0..chunks {
            if written >= out.len() {
                break;
            }
            let end = (offset + max_payload).min(payload.len());
            let chunk = &payload[offset..end];
            let is_last = chunk_idx == chunks - 1;

            let hdr = RtpHeader {
                version: 2,
                padding: false,
                extension: false,
                csrc_count: 0,
                marker: is_last,
                payload_type: self.cfg.payload_type,
                sequence_number: self.seq,
                timestamp: self.timestamp,
                ssrc: self.cfg.ssrc,
                csrc: CsrcList::new(),
                ext: None,
                header_size: 12,
            };

            let pkt = &mut out[written];
            if let Some(hdr_len) = hdr.write_into(&mut pkt.data) {
                let payload_end = hdr_len + chunk.len();
                if payload_end <= pkt.data.len() {
                    pkt.data[hdr_len..payload_end].copy_from_slice(chunk);
                    pkt.len = payload_end;
                    pkt.seq = self.seq;
                    pkt.timestamp = self.timestamp as u64;
                    pkt.ssrc = self.cfg.ssrc;
                    pkt.kind = kind;
                    written += 1;
                }
            }

            self.seq = self.seq.wrapping_add(1);
            offset = end;
        }

        if written > 0 {
            self.timestamp = self.timestamp.wrapping_add(duration_samples);
        }
        written
    }

    pub fn advance_timestamp(&mut self, samples: u32) {
        self.timestamp = self.timestamp.wrapping_add(samples);
    }

    pub fn current_seq(&self) -> u16 {
        self.seq
    }

    pub fn current_timestamp(&self) -> u32 {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packetize_small_payload() {
        let cfg = PacketizerConfig {
            payload_type: 96,
            ssrc: 0x1234,
            clock_rate: 90000,
            mtu: 1200,
            initial_seq: 100,
            initial_timestamp: 0,
        };
        let mut p = Packetizer::new(cfg);
        let payload = vec![0xAAu8; 100];
        let mut out = (0..4).map(|_| MediaPacket::default()).collect::<Vec<_>>();
        let n = p.packetize(&payload, 3000, PacketType::Audio, &mut out);
        assert_eq!(n, 1);
        assert_eq!(out[0].seq, 100);
        assert_eq!(p.current_seq(), 101);
    }

    #[test]
    fn packetize_large_payload_fragments() {
        let cfg = PacketizerConfig {
            payload_type: 96,
            ssrc: 0x5678,
            clock_rate: 90000,
            mtu: 200,
            initial_seq: 0,
            initial_timestamp: 0,
        };
        let mut p = Packetizer::new(cfg);
        let payload = vec![0xBBu8; 500];
        let mut out = (0..8).map(|_| MediaPacket::default()).collect::<Vec<_>>();
        let n = p.packetize(&payload, 0, PacketType::Video, &mut out);
        assert!(n > 1);
        let last_marker = out[n-1].parse_header().map(|h| h.marker).unwrap_or(false);
        assert!(last_marker);
    }
}

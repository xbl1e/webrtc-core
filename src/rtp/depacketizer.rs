use thiserror::Error;
use super::header::RtpHeader;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum DepacketizerError {
    #[error("packet too short")]
    TooShort,
    #[error("invalid RTP version")]
    InvalidVersion,
    #[error("buffer too small")]
    BufferTooSmall,
    #[error("reorder limit exceeded")]
    ReorderExceeded,
}

pub struct Depacketizer {
    last_seq: Option<u16>,
    reorder_window: u16,
    total_received: u64,
    total_out_of_order: u64,
}

impl Depacketizer {
    pub fn new(reorder_window: u16) -> Self {
        Self {
            last_seq: None,
            reorder_window,
            total_received: 0,
            total_out_of_order: 0,
        }
    }

    pub fn process<'a>(
        &mut self,
        pkt: &[u8],
        payload_out: &'a mut [u8],
    ) -> Result<(RtpHeader, usize), DepacketizerError> {
        let hdr = RtpHeader::parse(pkt).ok_or(DepacketizerError::TooShort)?;
        if hdr.version != 2 {
            return Err(DepacketizerError::InvalidVersion);
        }

        let seq = hdr.sequence_number;
        if let Some(last) = self.last_seq {
            let delta = seq.wrapping_sub(last) as i16;
            if delta < 0 {
                let abs_delta = (-delta) as u16;
                if abs_delta > self.reorder_window {
                    return Err(DepacketizerError::ReorderExceeded);
                }
                self.total_out_of_order += 1;
            }
        }
        self.last_seq = Some(seq);
        self.total_received += 1;

        let payload_offset = hdr.header_size;
        if payload_offset > pkt.len() {
            return Err(DepacketizerError::TooShort);
        }

        let mut payload_end = pkt.len();
        if hdr.padding && payload_end > payload_offset {
            let pad_len = pkt[payload_end - 1] as usize;
            if pad_len == 0 || pad_len > payload_end - payload_offset {
                return Err(DepacketizerError::TooShort);
            }
            payload_end -= pad_len;
        }

        let payload = &pkt[payload_offset..payload_end];
        if payload.len() > payload_out.len() {
            return Err(DepacketizerError::BufferTooSmall);
        }
        payload_out[..payload.len()].copy_from_slice(payload);
        Ok((hdr, payload.len()))
    }

    pub fn out_of_order_count(&self) -> u64 {
        self.total_out_of_order
    }

    pub fn total_received(&self) -> u64 {
        self.total_received
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtp::packetizer::{Packetizer, PacketizerConfig};
    use crate::rtp::packet::{MediaPacket, PacketType};

    #[test]
    fn depacketize_basic() {
        let cfg = PacketizerConfig {
            payload_type: 111,
            ssrc: 1,
            clock_rate: 8000,
            mtu: 1200,
            initial_seq: 1,
            initial_timestamp: 0,
        };
        let mut pktizer = Packetizer::new(cfg);
        let payload = vec![0x01u8; 160];
        let mut pkts = (0..2).map(|_| MediaPacket::default()).collect::<Vec<_>>();
        let n = pktizer.packetize(&payload, 160, PacketType::Audio, &mut pkts);
        assert_eq!(n, 1);

        let raw = &pkts[0].data[..pkts[0].len];
        let mut dpkt = Depacketizer::new(64);
        let mut out_buf = vec![0u8; 1500];
        let (hdr, plen) = dpkt.process(raw, &mut out_buf).unwrap();
        assert_eq!(hdr.sequence_number, 1);
        assert_eq!(plen, 160);
        assert_eq!(&out_buf[..plen], &payload[..]);
    }

    #[test]
    fn depacketize_padding() {
        let mut raw = vec![0x80u8, 0x61, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        raw.push(0xAA);
        raw[0] |= 0x20;
        raw.push(0x01);
        let mut dpkt = Depacketizer::new(64);
        let mut out = vec![0u8; 1500];
        let res = dpkt.process(&raw, &mut out);
        assert!(res.is_ok());
        let (_, plen) = res.unwrap();
        assert_eq!(plen, 1);
    }
}

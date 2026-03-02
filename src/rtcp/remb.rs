use thiserror::Error;

#[derive(Error, Debug)]
pub enum RembError {
    #[error("buffer too small")]
    BufferTooSmall,
    #[error("invalid REMB packet")]
    Invalid,
}

pub struct RembPacket {
    pub sender_ssrc: u32,
    pub media_ssrcs: Vec<u32>,
    pub bitrate_bps: u64,
}

impl RembPacket {
    pub fn new(sender_ssrc: u32, bitrate_bps: u64) -> Self {
        Self { sender_ssrc, media_ssrcs: Vec::new(), bitrate_bps }
    }

    pub fn add_ssrc(&mut self, ssrc: u32) {
        self.media_ssrcs.push(ssrc);
    }

    pub fn write_into(&self, out: &mut [u8]) -> Result<usize, RembError> {
        let num_ssrc = self.media_ssrcs.len().min(255);
        let fci_words = 2 + (num_ssrc + 1) / 1;
        let length_words = 1 + 1 + fci_words;
        let total = 4 + length_words * 4;
        if out.len() < total {
            return Err(RembError::BufferTooSmall);
        }
        out[0] = 0x8f;
        out[1] = 0xce;
        out[2..4].copy_from_slice(&(length_words as u16).to_be_bytes());
        out[4..8].copy_from_slice(&self.sender_ssrc.to_be_bytes());
        out[8..12].copy_from_slice(&0u32.to_be_bytes());
        out[12..16].copy_from_slice(b"REMB");
        out[16] = num_ssrc as u8;

        let (exp, mantissa) = Self::encode_bitrate(self.bitrate_bps);
        out[17] = (exp << 2) | ((mantissa >> 16) as u8 & 0x3);
        out[18] = ((mantissa >> 8) & 0xFF) as u8;
        out[19] = (mantissa & 0xFF) as u8;

        let mut off = 20usize;
        for &ssrc in self.media_ssrcs.iter().take(num_ssrc) {
            if off + 4 > out.len() {
                return Err(RembError::BufferTooSmall);
            }
            out[off..off+4].copy_from_slice(&ssrc.to_be_bytes());
            off += 4;
        }
        Ok(off)
    }

    pub fn parse(buf: &[u8]) -> Result<Self, RembError> {
        if buf.len() < 20 {
            return Err(RembError::Invalid);
        }
        if &buf[12..16] != b"REMB" {
            return Err(RembError::Invalid);
        }
        let sender_ssrc = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let num_ssrc = buf[16] as usize;
        let exp = (buf[17] >> 2) & 0x3F;
        let mantissa = ((buf[17] as u32 & 0x3) << 16) | ((buf[18] as u32) << 8) | (buf[19] as u32);
        let bitrate_bps = (mantissa as u64) << exp;
        let mut media_ssrcs = Vec::with_capacity(num_ssrc);
        let mut off = 20usize;
        for _ in 0..num_ssrc {
            if off + 4 > buf.len() {
                return Err(RembError::Invalid);
            }
            let ssrc = u32::from_be_bytes([buf[off], buf[off+1], buf[off+2], buf[off+3]]);
            media_ssrcs.push(ssrc);
            off += 4;
        }
        Ok(Self { sender_ssrc, media_ssrcs, bitrate_bps })
    }

    fn encode_bitrate(bps: u64) -> (u8, u32) {
        if bps == 0 {
            return (0, 0);
        }
        let mut exp = 0u8;
        let mut m = bps;
        while m > 0x3_FFFF {
            m >>= 1;
            exp += 1;
        }
        (exp, m as u32 & 0x3_FFFF)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remb_roundtrip() {
        let mut pkt = RembPacket::new(0x1234, 2_500_000);
        pkt.add_ssrc(0xAABB);
        pkt.add_ssrc(0xCCDD);
        let mut buf = [0u8; 64];
        let n = pkt.write_into(&mut buf).unwrap();
        assert!(n > 0);
        let parsed = RembPacket::parse(&buf[..n]).unwrap();
        assert_eq!(parsed.sender_ssrc, 0x1234);
        assert_eq!(parsed.media_ssrcs.len(), 2);
        assert!(parsed.bitrate_bps > 0);
    }
}

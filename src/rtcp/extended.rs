use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct ReceptionReport {
    pub ssrc: u32,
    pub fraction_lost: u8,
    pub cumulative_lost: u32,
    pub extended_highest_seq: u32,
    pub jitter: u32,
    pub last_sr: u32,
    pub delay_since_last_sr: u32,
}

impl ReceptionReport {
    pub fn write_into(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() < 24 {
            return None;
        }
        out[0..4].copy_from_slice(&self.ssrc.to_be_bytes());
        out[4] = self.fraction_lost;
        let cum = self.cumulative_lost & 0x00FF_FFFF;
        out[5] = ((cum >> 16) & 0xFF) as u8;
        out[6] = ((cum >> 8) & 0xFF) as u8;
        out[7] = (cum & 0xFF) as u8;
        out[8..12].copy_from_slice(&self.extended_highest_seq.to_be_bytes());
        out[12..16].copy_from_slice(&self.jitter.to_be_bytes());
        out[16..20].copy_from_slice(&self.last_sr.to_be_bytes());
        out[20..24].copy_from_slice(&self.delay_since_last_sr.to_be_bytes());
        Some(24)
    }

    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 24 {
            return None;
        }
        let ssrc = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let fraction_lost = buf[4];
        let cumulative_lost = ((buf[5] as u32) << 16) | ((buf[6] as u32) << 8) | (buf[7] as u32);
        let extended_highest_seq = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let jitter = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        let last_sr = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let delay_since_last_sr = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
        Some(Self {
            ssrc, fraction_lost, cumulative_lost, extended_highest_seq,
            jitter, last_sr, delay_since_last_sr,
        })
    }
}

#[derive(Debug)]
pub struct SenderInfo {
    pub ntp_timestamp: u64,
    pub rtp_timestamp: u32,
    pub packet_count: u32,
    pub octet_count: u32,
}

impl SenderInfo {
    pub fn write_into(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() < 20 {
            return None;
        }
        out[0..8].copy_from_slice(&self.ntp_timestamp.to_be_bytes());
        out[8..12].copy_from_slice(&self.rtp_timestamp.to_be_bytes());
        out[12..16].copy_from_slice(&self.packet_count.to_be_bytes());
        out[16..20].copy_from_slice(&self.octet_count.to_be_bytes());
        Some(20)
    }
}

pub struct RtcpSr {
    pub ssrc: u32,
    pub sender_info: SenderInfo,
    pub reports: Vec<ReceptionReport>,
}

impl RtcpSr {
    pub fn write_into(&self, out: &mut [u8]) -> Option<usize> {
        let rc = self.reports.len().min(31);
        let length_words = 1 + 5 + (rc * 6);
        let total = 4 + length_words * 4;
        if out.len() < total {
            return None;
        }
        out[0] = 0x80 | (rc as u8);
        out[1] = 0xC8;
        out[2..4].copy_from_slice(&(length_words as u16).to_be_bytes());
        out[4..8].copy_from_slice(&self.ssrc.to_be_bytes());
        let mut off = 8usize;
        self.sender_info.write_into(&mut out[off..])?;
        off += 20;
        for rr in self.reports.iter().take(rc) {
            rr.write_into(&mut out[off..])?;
            off += 24;
        }
        Some(off)
    }
}

pub struct RtcpRr {
    pub ssrc: u32,
    pub reports: Vec<ReceptionReport>,
}

impl RtcpRr {
    pub fn write_into(&self, out: &mut [u8]) -> Option<usize> {
        let rc = self.reports.len().min(31);
        let length_words = 1 + (rc * 6);
        let total = 4 + length_words * 4;
        if out.len() < total {
            return None;
        }
        out[0] = 0x80 | (rc as u8);
        out[1] = 0xC9;
        out[2..4].copy_from_slice(&(length_words as u16).to_be_bytes());
        out[4..8].copy_from_slice(&self.ssrc.to_be_bytes());
        let mut off = 8usize;
        for rr in self.reports.iter().take(rc) {
            rr.write_into(&mut out[off..])?;
            off += 24;
        }
        Some(off)
    }
}

pub struct LossStats {
    pub packets_received: AtomicU32,
    pub packets_expected: AtomicU32,
    pub jitter_acc: AtomicU64,
}

impl LossStats {
    pub const fn new() -> Self {
        Self {
            packets_received: AtomicU32::new(0),
            packets_expected: AtomicU32::new(0),
            jitter_acc: AtomicU64::new(0),
        }
    }

    pub fn record_received(&self) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_expected(&self, n: u32) {
        self.packets_expected.fetch_add(n, Ordering::Relaxed);
    }

    pub fn fraction_lost_u8(&self) -> u8 {
        let rx = self.packets_received.load(Ordering::Relaxed) as i64;
        let exp = self.packets_expected.load(Ordering::Relaxed) as i64;
        if exp <= 0 {
            return 0;
        }
        let lost = (exp - rx).max(0);
        ((lost * 256) / exp).min(255) as u8
    }
}

pub struct RtcpXr {
    pub ssrc: u32,
    pub loss_rle_ssrc: u32,
    pub begin_seq: u16,
    pub end_seq: u16,
    pub chunks: Vec<u16>,
}

impl RtcpXr {
    pub fn write_loss_rle_into(&self, out: &mut [u8]) -> Option<usize> {
        let chunk_count = self.chunks.len().min(64);
        let block_len_words = 2 + chunk_count;
        let length_words = 1 + 1 + block_len_words;
        let total = 4 + length_words * 4;
        if out.len() < total {
            return None;
        }
        out[0] = 0x80;
        out[1] = 0xcf;
        out[2..4].copy_from_slice(&(length_words as u16).to_be_bytes());
        out[4..8].copy_from_slice(&self.ssrc.to_be_bytes());
        out[8] = 1;
        out[9] = 0;
        out[10..12].copy_from_slice(&(block_len_words as u16).to_be_bytes());
        out[12..16].copy_from_slice(&self.loss_rle_ssrc.to_be_bytes());
        out[16..18].copy_from_slice(&self.begin_seq.to_be_bytes());
        out[18..20].copy_from_slice(&self.end_seq.to_be_bytes());
        let mut off = 20usize;
        for &c in self.chunks.iter().take(chunk_count) {
            out[off..off+2].copy_from_slice(&c.to_be_bytes());
            off += 2;
        }
        while off % 4 != 0 {
            out[off] = 0;
            off += 1;
        }
        Some(off)
    }
}

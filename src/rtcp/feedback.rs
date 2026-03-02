use crate::jitter_buffer::AudioJitterBuffer;
use crate::slab::SlabAllocator;

pub struct RtcpFeedback;

impl RtcpFeedback {
    pub fn write_nack_into(jb: &AudioJitterBuffer, slab: &SlabAllocator, out: &mut [u8]) -> usize {
        let mut w = 0usize;
        if out.len() < 12 {
            return 0;
        }
        out[0..2].copy_from_slice(&[0x81, 0x01]);
        out[2..4].copy_from_slice(&0u16.to_be_bytes());
        w += 4;
        let mut _missing_written = 0usize;
        let mut tmp = [0u16; 256];
        let found = jb.collect_missing(slab, &mut tmp);
        for &seq in tmp[..found].iter() {
            if w + 2 > out.len() {
                break;
            }
            out[w..w + 2].copy_from_slice(&seq.to_be_bytes());
            w += 2;
            _missing_written += 1;
        }
        w
    }

    pub fn write_twcc_into(jb: &AudioJitterBuffer, slab: &SlabAllocator, out: &mut [u8]) -> usize {
        if out.len() < 12 {
            return 0;
        }
        let mut w = 0usize;
        out[w..w + 2].copy_from_slice(&[0x80, 0x8f]);
        w += 2;
        out[w..w + 4].copy_from_slice(&0u32.to_be_bytes());
        w += 4;
        let (largest, bitmask) = jb.twcc_summary(slab);
        if w + 4 + 8 > out.len() {
            return 0;
        }
        out[w..w + 2].copy_from_slice(&((largest as u16).to_be_bytes()));
        w += 2;
        out[w..w + 2].copy_from_slice(&0u16.to_be_bytes());
        w += 2;
        out[w..w + 8].copy_from_slice(&bitmask.to_be_bytes());
        w += 8;
        w
    }

    pub fn write_pli_into(media_ssrc: u32, sender_ssrc: u32, out: &mut [u8]) -> usize {
        if out.len() < 12 {
            return 0;
        }
        out[0] = 0x81;
        out[1] = 0xce;
        out[2..4].copy_from_slice(&2u16.to_be_bytes());
        out[4..8].copy_from_slice(&sender_ssrc.to_be_bytes());
        out[8..12].copy_from_slice(&media_ssrc.to_be_bytes());
        12
    }

    pub fn write_fir_into(media_ssrc: u32, sender_ssrc: u32, seq: u8, out: &mut [u8]) -> usize {
        if out.len() < 20 {
            return 0;
        }
        out[0] = 0x84;
        out[1] = 0xce;
        out[2..4].copy_from_slice(&4u16.to_be_bytes());
        out[4..8].copy_from_slice(&sender_ssrc.to_be_bytes());
        out[8..12].copy_from_slice(&0u32.to_be_bytes());
        out[12..16].copy_from_slice(&media_ssrc.to_be_bytes());
        out[16] = seq;
        out[17] = 0;
        out[18] = 0;
        out[19] = 0;
        20
    }
}

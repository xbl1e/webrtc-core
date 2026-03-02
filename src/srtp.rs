use crate::packet::AudioPacket;
use aes_gcm::{
    aead::{AeadInPlace, KeyInit, Tag},
    Aes256Gcm, Nonce,
};
pub struct SrtpContext {
    cipher: Aes256Gcm,
}

impl SrtpContext {
    pub fn new(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(key).expect("invalid key length");
        Self { cipher }
    }

    pub fn protect_inplace(
        &self,
        pkt: &mut AudioPacket,
        nonce12: &[u8; 12],
        aad: &[u8],
    ) -> Result<usize, ()> {
        if !pkt.reserve_tail(16) {
            return Err(());
        }
        let n = Nonce::from_slice(nonce12);
        let plaintext = &mut pkt.data[..pkt.len];
        let tag = self
            .cipher
            .encrypt_in_place_detached(n, aad, plaintext)
            .map_err(|_| ())?;
        let tag_bytes = tag.as_slice();
        let start = pkt.len;
        pkt.data[start..start + tag_bytes.len()].copy_from_slice(tag_bytes);
        pkt.len += tag_bytes.len();
        Ok(pkt.len)
    }

    pub fn unprotect_inplace(
        &self,
        pkt: &mut AudioPacket,
        nonce12: &[u8; 12],
        aad: &[u8],
    ) -> Result<usize, ()> {
        if pkt.len < 16 {
            return Err(());
        }
        let tag_offset = pkt.len - 16;
        let (pt, tag) = pkt.data.split_at_mut(tag_offset);
        let n = Nonce::from_slice(nonce12);
        let mut tag_arr = [0u8; 16];
        tag_arr.copy_from_slice(tag);
        let tag_obj = <Tag<Aes256Gcm>>::from_slice(&tag_arr);
        self.cipher
            .decrypt_in_place_detached(n, aad, pt, tag_obj)
            .map_err(|_| ())?;
        pkt.len = tag_offset;
        Ok(pkt.len)
    }

    pub fn protect_index_inplace(
        &self,
        slab: &crate::slab::SlabAllocator,
        idx: usize,
        nonce12: &[u8; 12],
        aad: &[u8],
    ) -> Result<usize, ()> {
        let pkt = unsafe { slab.get_mut(idx) };
        self.protect_inplace(pkt, nonce12, aad)
    }

    pub fn unprotect_index_inplace(
        &self,
        slab: &crate::slab::SlabAllocator,
        idx: usize,
        nonce12: &[u8; 12],
        aad: &[u8],
    ) -> Result<usize, ()> {
        let pkt = unsafe { slab.get_mut(idx) };
        self.unprotect_inplace(pkt, nonce12, aad)
    }
}

use crate::packet::AudioPacket;
use crate::slab::SlabKey;
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
        key: &SlabKey,
        nonce12: &[u8; 12],
        aad: &[u8],
    ) -> Result<usize, ()> {
        let pkt = slab.get_mut(key).ok_or(())?;
        self.protect_inplace(pkt, nonce12, aad)
    }

    pub fn unprotect_index_inplace(
        &self,
        slab: &crate::slab::SlabAllocator,
        key: &SlabKey,
        nonce12: &[u8; 12],
        aad: &[u8],
    ) -> Result<usize, ()> {
        let pkt = slab.get_mut(key).ok_or(())?;
        self.unprotect_inplace(pkt, nonce12, aad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srtp_protect_unprotect() {
        let key = [0u8; 32];
        let nonce = [0u8; 12];
        let ctx = SrtpContext::new(&key);

        let mut pkt = AudioPacket::from_slice(&[1, 2, 3, 4, 5]);
        let original_len = pkt.len;

        assert!(ctx.protect_inplace(&mut pkt, &nonce, &[]).is_ok());
        assert!(pkt.len > original_len);

        assert!(ctx.unprotect_inplace(&mut pkt, &nonce, &[]).is_ok());
        assert_eq!(pkt.len, original_len);
        assert_eq!(&pkt.data[..original_len], &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn srtp_with_slab() {
        use crate::slab::SlabAllocator;
        let slab = SlabAllocator::new(16);
        let key = [0u8; 32];
        let nonce = [0u8; 12];
        let ctx = SrtpContext::new(&key);

        let slab_key = slab.allocate().unwrap();
        if let Some(pkt) = slab.get_mut(&slab_key) {
            pkt.len = 5;
            pkt.data[..5].copy_from_slice(&[1, 2, 3, 4, 5]);
        }

        assert!(ctx.protect_index_inplace(&slab, &slab_key, &nonce, &[]).is_ok());
        assert!(ctx.unprotect_index_inplace(&slab, &slab_key, &nonce, &[]).is_ok());

        if let Some(pkt) = slab.get_mut(&slab_key) {
            assert_eq!(&pkt.data[..5], &[1, 2, 3, 4, 5]);
        }
    }
}

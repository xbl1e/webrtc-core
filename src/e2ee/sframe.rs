use aes_gcm::{
    aead::{AeadInPlace, KeyInit},
    Aes256Gcm, Nonce,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

const TAG_LEN: usize = 16;

#[derive(Error, Debug)]
pub enum SFrameError {
    #[error("key not found for kid {0}")]
    KeyNotFound(u64),
    #[error("authentication failed")]
    AuthFailed,
    #[error("buffer too small")]
    BufferTooSmall,
    #[error("invalid frame header")]
    InvalidHeader,
    #[error("counter overflow")]
    CounterOverflow,
}

#[derive(Clone, Debug)]
pub struct SFrameConfig {
    pub ctr_bits: u8,
    pub key_id_bits: u8,
}

impl Default for SFrameConfig {
    fn default() -> Self {
        Self {
            ctr_bits: 32,
            key_id_bits: 3,
        }
    }
}

#[derive(Clone)]
struct SFrameKey {
    cipher: Arc<Aes256Gcm>,
    salt: [u8; 12],
}

impl SFrameKey {
    fn new(key_bytes: &[u8; 32], salt: [u8; 12]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(key_bytes).expect("invalid key");
        Self { cipher: Arc::new(cipher), salt }
    }

    fn build_nonce(&self, counter: u64) -> [u8; 12] {
        let mut n = self.salt;
        let c_bytes = counter.to_be_bytes();
        for i in 0..8 {
            n[4 + i] ^= c_bytes[i];
        }
        n
    }
}

pub struct KeyStore {
    keys: RwLock<HashMap<u64, SFrameKey>>,
}

impl KeyStore {
    pub fn new() -> Self {
        Self { keys: RwLock::new(HashMap::new()) }
    }

    pub fn add_key(&self, kid: u64, key_bytes: &[u8; 32], salt: [u8; 12]) {
        let mut map = self.keys.write().unwrap();
        map.insert(kid, SFrameKey::new(key_bytes, salt));
    }

    pub fn remove_key(&self, kid: u64) {
        let mut map = self.keys.write().unwrap();
        map.remove(&kid);
    }

    pub fn key_count(&self) -> usize {
        self.keys.read().unwrap().len()
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SFrameContext {
    cfg: SFrameConfig,
    keys: Arc<KeyStore>,
    sender_kid: u64,
    send_counter: std::sync::atomic::AtomicU64,
}

impl SFrameContext {
    pub fn new(cfg: SFrameConfig, keys: Arc<KeyStore>, sender_kid: u64) -> Self {
        Self {
            cfg,
            keys,
            sender_kid,
            send_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn sframe_header_len(&self, kid: u64) -> usize {
        let ctr_bytes = (self.cfg.ctr_bits as usize + 7) / 8;
        if kid < 8 { 1 + ctr_bytes } else { 1 + 8 + ctr_bytes }
    }

    fn encode_header(&self, kid: u64, ctr: u64, out: &mut [u8]) -> Option<usize> {
        let ctr_bytes = (self.cfg.ctr_bits as usize + 7) / 8;
        let kid_inline = kid < 8;
        let header_len = if kid_inline { 1 + ctr_bytes } else { 1 + 8 + ctr_bytes };
        if out.len() < header_len {
            return None;
        }
        let kid_flag = if kid_inline { 0u8 } else { 0x80u8 };
        let ctr_len_field = (ctr_bytes as u8).saturating_sub(1) & 0x7;
        out[0] = kid_flag | ((kid as u8 & 0x7) << 3) | ctr_len_field;
        let mut off = 1;
        if !kid_inline {
            out[off..off+8].copy_from_slice(&kid.to_be_bytes());
            off += 8;
        }
        let ctr_arr = ctr.to_be_bytes();
        let ctr_start = 8 - ctr_bytes;
        out[off..off + ctr_bytes].copy_from_slice(&ctr_arr[ctr_start..]);
        off += ctr_bytes;
        Some(off)
    }

    fn decode_header(&self, buf: &[u8]) -> Option<(u64, u64, usize)> {
        if buf.is_empty() {
            return None;
        }
        let ext_kid = (buf[0] & 0x80) != 0;
        let kid_inline = (buf[0] >> 3) & 0x7;
        let ctr_bytes = (buf[0] & 0x7) as usize + 1;
        let mut off = 1usize;
        let kid = if ext_kid {
            if off + 8 > buf.len() { return None; }
            let k = u64::from_be_bytes(buf[off..off+8].try_into().ok()?);
            off += 8;
            k
        } else {
            kid_inline as u64
        };
        if off + ctr_bytes > buf.len() {
            return None;
        }
        let mut ctr_arr = [0u8; 8];
        let ctr_start = 8 - ctr_bytes;
        ctr_arr[ctr_start..].copy_from_slice(&buf[off..off + ctr_bytes]);
        let ctr = u64::from_be_bytes(ctr_arr);
        off += ctr_bytes;
        Some((kid, ctr, off))
    }

    pub fn encrypt_frame(
        &self,
        rtp_header: &[u8],
        payload: &[u8],
        out: &mut [u8],
    ) -> Result<usize, SFrameError> {
        let ctr = self.send_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let kid = self.sender_kid;
        let hdr_len = self.sframe_header_len(kid);
        let total_len = rtp_header.len() + hdr_len + payload.len() + TAG_LEN;
        if out.len() < total_len {
            return Err(SFrameError::BufferTooSmall);
        }

        out[..rtp_header.len()].copy_from_slice(rtp_header);
        let hdr_written = self.encode_header(kid, ctr, &mut out[rtp_header.len()..])
            .ok_or(SFrameError::BufferTooSmall)?;
        let payload_start = rtp_header.len() + hdr_written;
        out[payload_start..payload_start + payload.len()].copy_from_slice(payload);

        let aad: Vec<u8> = out[..payload_start].to_vec();
        let keys = self.keys.keys.read().unwrap();
        let key = keys.get(&kid).ok_or(SFrameError::KeyNotFound(kid))?;
        let nonce_arr = key.build_nonce(ctr);
        let nonce = Nonce::from_slice(&nonce_arr);

        let plaintext = &mut out[payload_start..payload_start + payload.len()];
        let tag = key.cipher
            .encrypt_in_place_detached(nonce, &aad, plaintext)
            .map_err(|_| SFrameError::AuthFailed)?;

        let tag_start = payload_start + payload.len();
        out[tag_start..tag_start + TAG_LEN].copy_from_slice(tag.as_slice());
        Ok(total_len)
    }

    pub fn decrypt_frame(
        &self,
        buf: &mut [u8],
        rtp_header_len: usize,
    ) -> Result<usize, SFrameError> {
        let sframe_start = rtp_header_len;
        if sframe_start >= buf.len() {
            return Err(SFrameError::InvalidHeader);
        }
        let (kid, ctr, hdr_len) = self.decode_header(&buf[sframe_start..])
            .ok_or(SFrameError::InvalidHeader)?;
        let payload_start = sframe_start + hdr_len;
        if buf.len() < payload_start + TAG_LEN {
            return Err(SFrameError::InvalidHeader);
        }
        let total = buf.len();
        let tag_start = total - TAG_LEN;
        let mut tag_bytes = [0u8; TAG_LEN];
        tag_bytes.copy_from_slice(&buf[tag_start..tag_start + TAG_LEN]);

        let aad: Vec<u8> = buf[..payload_start].to_vec();
        let keys = self.keys.keys.read().unwrap();
        let key = keys.get(&kid).ok_or(SFrameError::KeyNotFound(kid))?;
        let nonce_arr = key.build_nonce(ctr);
        let nonce = Nonce::from_slice(&nonce_arr);
        let tag_obj = aes_gcm::aead::Tag::<Aes256Gcm>::from_slice(&tag_bytes);
        let ciphertext = &mut buf[payload_start..tag_start];
        key.cipher
            .decrypt_in_place_detached(nonce, &aad, ciphertext, tag_obj)
            .map_err(|_| SFrameError::AuthFailed)?;
        Ok(ciphertext.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> (SFrameContext, Arc<KeyStore>) {
        let store = Arc::new(KeyStore::new());
        let key = [0x42u8; 32];
        let salt = [0x01u8; 12];
        store.add_key(0, &key, salt);
        let ctx = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);
        (ctx, store)
    }

    #[test]
    fn sframe_encrypt_decrypt_roundtrip() {
        let (ctx, _store) = make_context();
        let rtp_hdr = [0x80u8, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let payload = b"Hello, SFrame!";
        let mut encrypted = vec![0u8; 256];
        let enc_len = ctx.encrypt_frame(&rtp_hdr, payload, &mut encrypted).unwrap();
        assert!(enc_len > rtp_hdr.len() + payload.len());

        let mut to_decrypt = encrypted[..enc_len].to_vec();
        let plain_len = ctx.decrypt_frame(&mut to_decrypt, rtp_hdr.len()).unwrap();
        assert_eq!(plain_len, payload.len());
        let ctr_bytes = (SFrameConfig::default().ctr_bits as usize + 7) / 8;
        let sframe_hdr_len = 1 + ctr_bytes;
        let decrypted_payload_start = rtp_hdr.len() + sframe_hdr_len;
        assert_eq!(&to_decrypt[decrypted_payload_start..decrypted_payload_start + plain_len], payload);
    }

    #[test]
    fn sframe_wrong_key_fails() {
        let (ctx, store) = make_context();
        let rtp_hdr = [0x80u8, 0x60, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let payload = b"Secret";
        let mut encrypted = vec![0u8; 256];
        let enc_len = ctx.encrypt_frame(&rtp_hdr, payload, &mut encrypted).unwrap();
        store.add_key(0, &[0x99u8; 32], [0x02u8; 12]);
        let mut to_decrypt = encrypted[..enc_len].to_vec();
        let result = ctx.decrypt_frame(&mut to_decrypt, rtp_hdr.len());
        assert!(result.is_err());
    }

    #[test]
    fn sframe_key_not_found() {
        let store = Arc::new(KeyStore::new());
        let ctx = SFrameContext::new(SFrameConfig::default(), store, 5);
        let rtp_hdr = [0x80u8; 12];
        let payload = b"data";
        let mut out = vec![0u8; 256];
        let result = ctx.encrypt_frame(&rtp_hdr, payload, &mut out);
        assert!(matches!(result, Err(SFrameError::KeyNotFound(5))));
    }
}

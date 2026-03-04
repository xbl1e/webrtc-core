pub mod handshake;

use aes_gcm::{
    aead::{AeadInPlace, KeyInit},
    Aes128Gcm, Nonce,
};
use ring::rand::SecureRandom;
use ring::rand::SystemRandom;
use ring::hkdf::{Salt, HKDF_SHA256};
use thiserror::Error;

pub const DTLS_VERSION_1_2: u16 = 0xfefd;
pub const DTLS_VERSION_1_0: u16 = 0xfeff;

pub const CONTENT_TYPE_CHANGE_CIPHER_SPEC: u8 = 20;
pub const CONTENT_TYPE_ALERT: u8 = 21;
pub const CONTENT_TYPE_HANDSHAKE: u8 = 22;
pub const CONTENT_TYPE_APPLICATION_DATA: u8 = 23;

pub const HANDSHAKE_TYPE_CLIENT_HELLO: u8 = 1;
pub const HANDSHAKE_TYPE_SERVER_HELLO: u8 = 2;
pub const HANDSHAKE_TYPE_HELLO_VERIFY_REQUEST: u8 = 3;
pub const HANDSHAKE_TYPE_CERTIFICATE: u8 = 11;
pub const HANDSHAKE_TYPE_SERVER_KEY_EXCHANGE: u8 = 12;
pub const HANDSHAKE_TYPE_CERTIFICATE_REQUEST: u8 = 13;
pub const HANDSHAKE_TYPE_SERVER_HELLO_DONE: u8 = 14;
pub const HANDSHAKE_TYPE_CERTIFICATE_VERIFY: u8 = 15;
pub const HANDSHAKE_TYPE_CLIENT_KEY_EXCHANGE: u8 = 16;
pub const HANDSHAKE_TYPE_FINISHED: u8 = 20;

#[derive(Error, Debug)]
pub enum DtlsError {
    #[error("invalid record")]
    InvalidRecord,
    #[error("invalid version")]
    InvalidVersion,
    #[error("invalid content type")]
    InvalidContentType,
    #[error("handshake failed")]
    HandshakeFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("invalid state")]
    InvalidState,
    #[error("MAC verification failed")]
    MacFailed,
    #[error("key derivation failed")]
    KeyDerivation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtlsRole {
    Client,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtlsState {
    Closed,
    Connecting,
    Connected,
    Failed,
}

impl Default for DtlsState {
    fn default() -> Self {
        DtlsState::Closed
    }
}

#[derive(Clone, Debug)]
pub struct DtlsRecordHeader {
    pub content_type: u8,
    pub version: u16,
    pub epoch: u16,
    pub sequence_number: u64,
    pub length: u16,
}

impl DtlsRecordHeader {
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 13 {
            return None;
        }
        Some(Self {
            content_type: buf[0],
            version: u16::from_be_bytes([buf[1], buf[2]]),
            epoch: u16::from_be_bytes([buf[3], buf[4]]),
            sequence_number: u64::from_be_bytes([
                0, 0, buf[5], buf[6], buf[7], buf[8], buf[9], buf[10],
            ]),
            length: u16::from_be_bytes([buf[11], buf[12]]),
        })
    }

    pub fn write_into(&self, buf: &mut [u8]) -> Option<usize> {
        if buf.len() < 13 {
            return None;
        }
        buf[0] = self.content_type;
        buf[1..3].copy_from_slice(&self.version.to_be_bytes());
        buf[3..5].copy_from_slice(&self.epoch.to_be_bytes());
        buf[5..11].copy_from_slice(&self.sequence_number.to_be_bytes()[2..]);
        buf[11..13].copy_from_slice(&self.length.to_be_bytes());
        Some(13)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DtlsCipherSuite {
    pub id: u16,
    pub name: String,
    pub key_len: usize,
    pub iv_len: usize,
    pub mac_len: usize,
}

impl DtlsCipherSuite {
    pub fn aes_128_gcm_sha256() -> Self {
        Self {
            id: 0xc02b,
            name: "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".to_string(),
            key_len: 16,
            iv_len: 12,
            mac_len: 0,
        }
    }

    pub fn aes_256_gcm_sha384() -> Self {
        Self {
            id: 0xc02c,
            name: "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384".to_string(),
            key_len: 32,
            iv_len: 12,
            mac_len: 0,
        }
    }
}

pub struct DtlsSecurityParameters {
    _cipher_suite: DtlsCipherSuite,
    client_write_key: Vec<u8>,
    server_write_key: Vec<u8>,
    client_write_iv: Vec<u8>,
    server_write_iv: Vec<u8>,
    master_secret: [u8; 48],
}

impl DtlsSecurityParameters {
    pub fn new(
        cipher_suite: DtlsCipherSuite,
        master_secret: [u8; 48],
        client_random: &[u8; 32],
        server_random: &[u8; 32],
    ) -> Self {
        let mut seed = Vec::with_capacity(client_random.len() + server_random.len());
        seed.extend_from_slice(server_random);
        seed.extend_from_slice(client_random);
        let key_block = Self::prf(
            &master_secret,
            b"key expansion",
            &seed,
            cipher_suite.key_len * 2 + cipher_suite.iv_len * 2,
        );

        let client_write_key = key_block[0..cipher_suite.key_len].to_vec();
        let server_write_key = key_block[cipher_suite.key_len..cipher_suite.key_len * 2].to_vec();
        let offset = cipher_suite.key_len * 2;
        let client_write_iv = key_block[offset..offset + cipher_suite.iv_len].to_vec();
        let server_write_iv = key_block[offset + cipher_suite.iv_len..].to_vec();

        Self {
            _cipher_suite: cipher_suite,
            client_write_key,
            server_write_key,
            client_write_iv,
            server_write_iv,
            master_secret,
        }
    }

    fn prf(secret: &[u8; 48], label: &[u8], seed: &[u8], length: usize) -> Vec<u8> {
        let salt = Salt::new(HKDF_SHA256, secret);
        let info: Vec<u8> = label.iter().chain(seed.iter()).cloned().collect();
        let prk = salt.extract(&[]);
        let mut out = vec![0u8; length];
        if let Ok(okm) = prk.expand(&[&info[..]], HKDF_SHA256) {
            let _ = okm.fill(&mut out);
        }
        out
    }

    pub fn srtp_master_key_and_salt(&self) -> Result<([u8; 16], [u8; 14]), DtlsError> {
        let key_block = Self::prf(
            &self.master_secret,
            b"EXTRACTOR-dtls_srtp",
            &[],
            60,
        );
        let mut key = [0u8; 16];
        let mut salt = [0u8; 14];
        key.copy_from_slice(&key_block[0..16]);
        salt.copy_from_slice(&key_block[16..30]);
        Ok((key, salt))
    }
}

pub struct DtlsContext {
    role: DtlsRole,
    state: DtlsState,
    epoch: u16,
    send_seq: u64,
    recv_seq: u64,
    cipher: Option<Aes128Gcm>,
    security_params: Option<DtlsSecurityParameters>,
}

impl DtlsContext {
    pub fn new(role: DtlsRole) -> Self {
        Self {
            role,
            state: DtlsState::Closed,
            epoch: 0,
            send_seq: 0,
            recv_seq: 0,
            cipher: None,
            security_params: None,
        }
    }

    pub fn state(&self) -> DtlsState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state == DtlsState::Connected
    }

    pub fn role(&self) -> DtlsRole {
        self.role
    }

    pub fn epoch(&self) -> u16 {
        self.epoch
    }

    pub fn set_security_parameters(&mut self, params: DtlsSecurityParameters) {
        let key = if self.role == DtlsRole::Client {
            &params.client_write_key
        } else {
            &params.server_write_key
        };
        if key.len() == 16 {
            self.cipher = Some(Aes128Gcm::new_from_slice(key).expect("invalid key"));
        }
        self.security_params = Some(params);
        self.epoch = 1;
    }

    pub fn encrypt_record(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, DtlsError> {
        let cipher = self.cipher.as_ref().ok_or(DtlsError::InvalidState)?;
        let params = self.security_params.as_ref().ok_or(DtlsError::InvalidState)?;

        if out.len() < plaintext.len() + 8 + 16 + 13 {
            return Err(DtlsError::EncryptionFailed);
        }

        let iv = if self.role == DtlsRole::Client {
            &params.client_write_iv
        } else {
            &params.server_write_iv
        };

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..iv.len()].copy_from_slice(iv);
        let seq_bytes = self.send_seq.to_be_bytes();
        for (i, b) in nonce_bytes.iter_mut().enumerate() {
            *b ^= seq_bytes[8 - iv.len() + i];
        }
        let nonce = Nonce::from_slice(&nonce_bytes);

        let hdr = DtlsRecordHeader {
            content_type: CONTENT_TYPE_APPLICATION_DATA,
            version: DTLS_VERSION_1_2,
            epoch: self.epoch,
            sequence_number: self.send_seq,
            length: (plaintext.len() + 8 + 16) as u16,
        };

        let hdr_len = hdr.write_into(out).ok_or(DtlsError::EncryptionFailed)?;

        let mut explicit_nonce = [0u8; 8];
        explicit_nonce.copy_from_slice(&self.send_seq.to_be_bytes()[..8]);
        out[hdr_len..hdr_len + 8].copy_from_slice(&explicit_nonce);

        out[hdr_len + 8..hdr_len + 8 + plaintext.len()].copy_from_slice(plaintext);

        let ct = &mut out[hdr_len + 8..hdr_len + 8 + plaintext.len()];
        let tag = cipher
            .encrypt_in_place_detached(nonce, &[], ct)
            .map_err(|_| DtlsError::EncryptionFailed)?;

        let tag_start = hdr_len + 8 + plaintext.len();
        out[tag_start..tag_start + 16].copy_from_slice(tag.as_slice());

        self.send_seq = self.send_seq.wrapping_add(1);
        Ok(hdr_len + 8 + plaintext.len() + 16)
    }

    pub fn decrypt_record(&mut self, record: &[u8], out: &mut [u8]) -> Result<usize, DtlsError> {
        let cipher = self.cipher.as_ref().ok_or(DtlsError::InvalidState)?;
        let params = self.security_params.as_ref().ok_or(DtlsError::InvalidState)?;

        let hdr = DtlsRecordHeader::parse(record).ok_or(DtlsError::InvalidRecord)?;

        if hdr.content_type != CONTENT_TYPE_APPLICATION_DATA {
            return Err(DtlsError::InvalidContentType);
        }

        let payload = &record[13..];
        if payload.len() < 8 + 16 {
            return Err(DtlsError::InvalidRecord);
        }

        let iv = if self.role == DtlsRole::Client {
            &params.server_write_iv
        } else {
            &params.client_write_iv
        };

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[..iv.len()].copy_from_slice(iv);
        let seq_bytes = hdr.sequence_number.to_be_bytes();
        for (i, b) in nonce_bytes.iter_mut().enumerate() {
            *b ^= seq_bytes[8 - iv.len() + i];
        }

        let explicit_nonce = &payload[..8];
        for (i, b) in nonce_bytes.iter_mut().enumerate() {
            *b ^= explicit_nonce[i];
        }

        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = &payload[8..];
        let (ct, tag) = ciphertext.split_at(ciphertext.len() - 16);

        if out.len() < ct.len() {
            return Err(DtlsError::DecryptionFailed);
        }

        out[..ct.len()].copy_from_slice(ct);
        let pt = &mut out[..ct.len()];

        let tag_arr = aes_gcm::aead::Tag::<Aes128Gcm>::from_slice(tag);
        cipher
            .decrypt_in_place_detached(nonce, &[], pt, tag_arr)
            .map_err(|_| DtlsError::DecryptionFailed)?;

        self.recv_seq = hdr.sequence_number.wrapping_add(1);
        Ok(ct.len())
    }

    pub fn srtp_keying_material(&self) -> Option<([u8; 16], [u8; 14])> {
        self.security_params.as_ref()?.srtp_master_key_and_salt().ok()
    }
}

pub struct DtlsEndpoint {
    context: DtlsContext,
    local_random: [u8; 32],
    remote_random: Option<[u8; 32]>,
    _pending_epoch: u16,
}

impl DtlsEndpoint {
    pub fn new(role: DtlsRole) -> Self {
        let mut local_random = [0u8; 32];
        let rng = SystemRandom::new();
        rng.fill(&mut local_random).unwrap();

        Self {
            context: DtlsContext::new(role),
            local_random,
            remote_random: None,
            _pending_epoch: 0,
        }
    }

    pub fn state(&self) -> DtlsState {
        self.context.state()
    }

    pub fn is_connected(&self) -> bool {
        self.context.is_connected()
    }

    pub fn role(&self) -> DtlsRole {
        self.context.role()
    }

    pub fn local_random(&self) -> &[u8; 32] {
        &self.local_random
    }

    pub fn set_remote_random(&mut self, random: [u8; 32]) {
        self.remote_random = Some(random);
    }

    pub fn compute_master_secret(&mut self, premaster: [u8; 48]) -> [u8; 48] {
        let mut master = [0u8; 48];
        let remote = self.remote_random.unwrap_or([0u8; 32]);
        let seed: Vec<u8> = self.local_random.iter()
            .chain(remote.iter())
            .cloned()
            .collect();

        let salt = Salt::new(HKDF_SHA256, &premaster);
        let prk = salt.extract(&[]);
        if let Ok(okm) = prk.expand(&[&seed[..]], HKDF_SHA256) {
            let _ = okm.fill(&mut master);
        }
        master
    }

    pub fn handshake_complete(&mut self, master_secret: [u8; 48]) {
        let remote = self.remote_random.unwrap_or([0u8; 32]);
        let params = DtlsSecurityParameters::new(
            DtlsCipherSuite::aes_128_gcm_sha256(),
            master_secret,
            &self.local_random,
            &remote,
        );
        self.context.set_security_parameters(params);
        self.context.state = DtlsState::Connected;
    }

    pub fn encrypt(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, DtlsError> {
        self.context.encrypt_record(plaintext, out)
    }

    pub fn decrypt(&mut self, record: &[u8], out: &mut [u8]) -> Result<usize, DtlsError> {
        self.context.decrypt_record(record, out)
    }

    pub fn srtp_keying_material(&self) -> Option<([u8; 16], [u8; 14])> {
        self.context.srtp_keying_material()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtls_record_header_roundtrip() {
        let hdr = DtlsRecordHeader {
            content_type: CONTENT_TYPE_APPLICATION_DATA,
            version: DTLS_VERSION_1_2,
            epoch: 1,
            sequence_number: 12345,
            length: 256,
        };
        let mut buf = [0u8; 32];
        hdr.write_into(&mut buf).unwrap();
        let parsed = DtlsRecordHeader::parse(&buf).unwrap();
        assert_eq!(parsed.content_type, hdr.content_type);
        assert_eq!(parsed.version, hdr.version);
        assert_eq!(parsed.epoch, hdr.epoch);
        assert_eq!(parsed.sequence_number, hdr.sequence_number);
        assert_eq!(parsed.length, hdr.length);
    }

    #[test]
    fn dtls_context_creation() {
        let ctx = DtlsContext::new(DtlsRole::Client);
        assert_eq!(ctx.role(), DtlsRole::Client);
        assert_eq!(ctx.state(), DtlsState::Closed);
        assert!(!ctx.is_connected());
    }

    #[test]
    fn dtls_endpoint_creation() {
        let endpoint = DtlsEndpoint::new(DtlsRole::Server);
        assert_eq!(endpoint.role(), DtlsRole::Server);
        assert_eq!(endpoint.local_random().len(), 32);
    }

    #[test]
    fn dtls_security_params_srtp_keys() {
        let cipher = DtlsCipherSuite::aes_128_gcm_sha256();
        let master = [0x42u8; 48];
        let client_random = [0x01u8; 32];
        let server_random = [0x02u8; 32];
        let params = DtlsSecurityParameters::new(cipher, master, &client_random, &server_random);
        let (key, salt) = params.srtp_master_key_and_salt().unwrap();
        assert_eq!(key.len(), 16);
        assert_eq!(salt.len(), 14);
    }

    #[test]
    fn dtls_encrypt_decrypt_roundtrip() {
        let mut client = DtlsContext::new(DtlsRole::Client);
        let mut server = DtlsContext::new(DtlsRole::Server);

        let master = [0x42u8; 48];
        let client_random = [0x01u8; 32];
        let server_random = [0x02u8; 32];

        let client_params = DtlsSecurityParameters::new(
            DtlsCipherSuite::aes_128_gcm_sha256(),
            master,
            &client_random,
            &server_random,
        );
        let server_params = DtlsSecurityParameters::new(
            DtlsCipherSuite::aes_128_gcm_sha256(),
            master,
            &client_random,
            &server_random,
        );

        client.set_security_parameters(client_params);
        server.set_security_parameters(server_params);

        let plaintext = b"Hello, DTLS!";
        let mut encrypted = vec![0u8; 256];
        let _enc_len = client.encrypt_record(plaintext, &mut encrypted).unwrap();

        let hdr = DtlsRecordHeader::parse(&encrypted).unwrap();
        let record = &encrypted[..13 + hdr.length as usize];

        let mut decrypted = vec![0u8; 256];
        let dec_len = server.decrypt_record(record, &mut decrypted).unwrap();

        assert_eq!(&decrypted[..dec_len], plaintext);
    }
}

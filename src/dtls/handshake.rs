use ring::rand::SecureRandom;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING, ECDSA_P256_SHA256_FIXED};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HandshakeError {
    #[error("invalid handshake message")]
    InvalidMessage,
    #[error("invalid state for operation")]
    InvalidState,
    #[error("certificate error")]
    CertificateError,
    #[error("key derivation failed")]
    KeyDerivationFailed,
    #[error("signature verification failed")]
    SignatureVerificationFailed,
    #[randomness]
    RandomError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeState {
    Initial,
    ClientHelloSent,
    ServerHelloReceived,
    CertificateSent,
    CertificateVerified,
    KeyExchangeSent,
    FinishedSent,
    Complete,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ClientHello {
    pub client_random: [u8; 32],
    pub session_id: Vec<u8>,
    pub cipher_suites: Vec<u16>,
    pub compression_methods: Vec<u8>,
    pub extensions: Vec<u8>,
}

impl ClientHello {
    pub fn new() -> Self {
        let rng = SystemRandom::new();
        let mut client_random = [0u8; 32];
        rng.fill(&mut client_random).unwrap();
        
        Self {
            client_random,
            session_id: Vec::new(),
            cipher_suites: vec![0xc02b, 0xc02c, 0xcca9, 0xcca8],
            compression_methods: vec![0x00],
            extensions: Vec::new(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        buf.extend_from_slice(&[0x01]);
        buf.extend_from_slice(&0x0303u16.to_be_bytes());
        buf.extend_from_slice(&self.client_random);
        
        buf.push(self.session_id.len() as u8);
        buf.extend_from_slice(&self.session_id);
        
        buf.extend_from_slice(&(self.cipher_suites.len() as u16).to_be_bytes());
        for cs in &self.cipher_suites {
            buf.extend_from_slice(&cs.to_be_bytes());
        }
        
        buf.push(self.compression_methods.len() as u8);
        buf.extend_from_slice(&self.compression_methods);
        
        let ext_len_pos = buf.len();
        buf.extend_from_slice(&0u16.to_be_bytes());
        
        self.encode_extensions(&mut buf);
        
        let total_len = buf.len() - ext_len_pos - 2;
        buf[ext_len_pos..ext_len_pos+2].copy_from_slice(&(total_len as u16).to_be_bytes());
        
        let total = buf.len();
        vec![0x16, 0xfe, 0xfd]
    }

    fn encode_extensions(&self, buf: &mut Vec<u8>) {
        let mut ext_data = Vec::new();
        
        ext_data.extend_from_slice(&0x002au16.to_be_bytes());
        let supported_versions_len = 2 + 2;
        ext_data.extend_from_slice(&(supported_versions_len as u16).to_be_bytes());
        ext_data.extend_from_slice(&0x03u8.to_be_bytes());
        ext_data.extend_from_slice(&3u8.to_be_bytes());
        ext_data.extend_from_slice(&0x03u8.to_be_bytes());
        ext_data.extend_from_slice(&0xfdu8.to_be_bytes());
        
        ext_data.extend_from_slice(&0x000du16.to_be_bytes());
        let groups_len = 2 + 4;
        ext_data.extend_from_slice(&(groups_len as u16).to_be_bytes());
        ext_data.extend_from_slice(&2u16.to_be_bytes());
        ext_data.extend_from_slice(&0x0017u16.to_be_bytes());
        ext_data.extend_from_slice(&0x001du16.to_be_bytes());
        
        ext_data.extend_from_slice(&0x000du16.to_be_bytes());
        let sig_algs_len = 2 + 4;
        ext_data.extend_from_slice(&(sig_algs_len as u16).to_be_bytes());
        ext_data.extend_from_slice(&4u16.to_be_bytes());
        ext_data.extend_from_slice(&0x0403u16.to_be_bytes());
        ext_data.extend_from_slice(&0x0804u16.to_be_bytes());
        
        ext_data.extend_from_slice(&0x0010u16.to_be_bytes());
        let srtp_len = 2 + 4;
        ext_data.extend_from_slice(&(srtp_len as u16).to_be_bytes());
        ext_data.extend_from_slice(&1u16.to_be_bytes());
        ext_data.extend_from_slice(&0xc02bu16.to_be_bytes());
        ext_data.extend_from_slice(&0u16.to_be_bytes());
        
        ext_data.extend_from_slice(&0xff01u16.to_be_bytes());
        ext_data.extend_from_slice(&0u16.to_be_bytes());
        
        let pos = buf.len();
        buf.extend_from_slice(&(ext_data.len() as u16).to_be_bytes());
        buf.splice(pos..pos, ext_data);
    }
}

pub struct ServerHello {
    pub server_random: [u8; 32],
    pub session_id: Vec<u8>,
    pub cipher_suite: u16,
    pub compression_method: u8,
    pub extensions: Vec<u8>,
}

impl ServerHello {
    pub fn decode(data: &[u8]) -> Result<Self, HandshakeError> {
        if data.len() < 38 {
            return Err(HandshakeError::InvalidMessage);
        }
        
        let mut server_random = [0u8; 32];
        server_random.copy_from_slice(&data[2..34]);
        
        let session_id_len = data[34] as usize;
        let session_id = data[35..35+session_id_len].to_vec();
        
        let cs_offset = 35 + session_id_len + 1;
        let cipher_suite = u16::from_be_bytes([data[cs_offset], data[cs_offset+1]]);
        
        let cm_offset = cs_offset + 2;
        let compression_method = data[cm_offset];
        
        Ok(Self {
            server_random,
            session_id,
            cipher_suite,
            compression_method,
            extensions: Vec::new(),
        })
    }
}

pub struct Certificate {
    pub certificates: Vec<Vec<u8>>,
}

impl Certificate {
    pub fn generate_self_signed() -> Result<Self, HandshakeError> {
        let rng = SystemRandom::new();
        
        let pkcs8_bytes = include_bytes!("../../testdata/ecdsa_p256_pkcs8.der");
        let key_pair = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8_bytes,
            &rng,
        ).map_err(|_| HandshakeError::CertificateError)?;
        
        let cert_der = include_bytes!("../../testdata/certificate.der");
        
        Ok(Self {
            certificates: vec![cert_der.to_vec()],
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        let certs_len: u32 = self.certificates.iter().map(|c| c.len() as u32 + 3).sum();
        buf.extend_from_slice(&certs_len.to_be_bytes());
        
        for cert in &self.certificates {
            buf.extend_from_slice(&(cert.len() as u32).to_be_bytes());
            buf.extend_from_slice(cert);
        }
        
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, HandshakeError> {
        if data.len() < 3 {
            return Err(HandshakeError::InvalidMessage);
        }
        
        let certs_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + certs_len {
            return Err(HandshakeError::InvalidMessage);
        }
        
        let mut certificates = Vec::new();
        let mut pos = 4;
        
        while pos + 3 < data.len() {
            let cert_len = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
            pos += 4;
            if pos + cert_len > data.len() {
                break;
            }
            certificates.push(data[pos..pos+cert_len].to_vec());
            pos += cert_len;
        }
        
        Ok(Self { certificates })
    }
}

pub struct ServerKeyExchange {
    pub curve_type: u8,
    pub named_curve: u16,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl ServerKeyExchange {
    pub fn decode(data: &[u8]) -> Result<Self, HandshakeError> {
        if data.len() < 5 {
            return Err(HandshakeError::InvalidMessage);
        }
        
        let curve_type = data[0];
        if curve_type != 3 {
            return Err(HandshakeError::InvalidMessage);
        }
        
        let named_curve = u16::from_be_bytes([data[1], data[2]]);
        
        let pk_len = data[3] as usize;
        if data.len() < 4 + pk_len {
            return Err(HandshakeError::InvalidMessage);
        }
        let public_key = data[4..4+pk_len].to_vec();
        
        let sig_offset = 4 + pk_len;
        if data.len() < sig_offset + 2 {
            return Err(HandshakeError::InvalidMessage);
        }
        let sig_len = u16::from_be_bytes([data[sig_offset], data[sig_offset+1]]) as usize;
        if data.len() < sig_offset + 2 + sig_len {
            return Err(HandshakeError::InvalidMessage);
        }
        let signature = data[sig_offset+2..sig_offset+2+sig_len].to_vec();
        
        Ok(Self {
            curve_type,
            named_curve,
            public_key,
            signature,
        })
    }
}

pub struct ClientKeyExchange {
    pub client_public_key: Vec<u8>,
}

impl ClientKeyExchange {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 1 + self.client_public_key.len() as u8];
        buf[0] = self.client_public_key.len() as u8;
        buf[1..].copy_from_slice(&self.client_public_key);
        buf
    }
}

pub struct CertificateVerify {
    pub signature: Vec<u8>,
}

impl CertificateVerify {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 2 + self.signature.len()];
        buf[0..2].copy_from_slice(&(self.signature.len() as u16).to_be_bytes());
        buf[2..].copy_from_slice(&self.signature);
        buf
    }
}

pub struct Finished {
    pub verify_data: Vec<u8>,
}

impl Finished {
    pub fn decode(data: &[u8]) -> Result<Self, HandshakeError> {
        if data.len() < 12 {
            return Err(HandshakeError::InvalidMessage);
        }
        Ok(Self {
            verify_data: data.to_vec(),
        })
    }
}

pub struct DtlsHandshake {
    pub state: HandshakeState,
    pub role: crate::dtls::DtlsRole,
    pub local_random: [u8; 32],
    pub remote_random: Option<[u8; 32]>,
    pub cipher_suite: u16,
    pub master_secret: Option<[u8; 48]>,
    pub local_keypair: Option<EcdsaKeyPair>,
    pub remote_public_key: Option<Vec<u8>>,
    pub srtp_profiles: Vec<u16>,
}

impl DtlsHandshake {
    pub fn new_client() -> Result<Self, HandshakeError> {
        let rng = SystemRandom::new();
        let mut local_random = [0u8; 32];
        rng.fill(&mut local_random).unwrap();
        
        let pkcs8_bytes = include_bytes!("../../testdata/ecdsa_p256_pkcs8.der");
        let keypair = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8_bytes,
            &rng,
        ).map_err(|_| HandshakeError::CertificateError)?;
        
        Ok(Self {
            state: HandshakeState::Initial,
            role: crate::dtls::DtlsRole::Client,
            local_random,
            remote_random: None,
            cipher_suite: 0xc02b,
            master_secret: None,
            local_keypair: Some(keypair),
            remote_public_key: None,
            srtp_profiles: vec![0xc02b],
        })
    }

    pub fn new_server() -> Result<Self, HandshakeError> {
        let rng = SystemRandom::new();
        let mut local_random = [0u8; 32];
        rng.fill(&mut local_random).unwrap();
        
        Ok(Self {
            state: HandshakeState::Initial,
            role: crate::dtls::DtlsRole::Server,
            local_random,
            remote_random: None,
            cipher_suite: 0xc02b,
            master_secret: None,
            local_keypair: None,
            remote_public_key: None,
            srtp_profiles: vec![0xc02b],
        })
    }

    pub fn create_client_hello(&mut self) -> Result<Vec<u8>, HandshakeError> {
        let hello = ClientHello::new();
        self.local_random = hello.client_random;
        
        let mut record = hello.encode();
        
        let handshake = self.encode_handshake(1, &record);
        
        self.state = HandshakeState::ClientHelloSent;
        
        Ok(handshake)
    }

    pub fn handle_server_hello(&mut self, data: &[u8]) -> Result<Vec<u8>, HandshakeError> {
        let hello = ServerHello::decode(data)?;
        self.remote_random = Some(hello.server_random);
        self.cipher_suite = hello.cipher_suite;
        
        self.state = HandshakeState::ServerHelloReceived;
        
        Ok(Vec::new())
    }

    pub fn handle_certificate(&mut self, data: &[u8]) -> Result<(), HandshakeError> {
        let _cert = Certificate::decode(data)?;
        self.state = HandshakeState::CertificateVerified;
        Ok(())
    }

    pub fn handle_server_key_exchange(&mut self, data: &[u8]) -> Result<(), HandshakeError> {
        let _ske = ServerKeyExchange::decode(data)?;
        self.state = HandshakeState::KeyExchangeSent;
        Ok(())
    }

    pub fn create_client_key_exchange(&mut self) -> Result<Vec<u8>, HandshakeError> {
        let keypair = self.local_keypair.as_ref().ok_or(HandshakeError::InvalidState)?;
        let public_key = keypair.public_key().as_ref().to_vec();
        
        let ckex = ClientKeyExchange { client_public_key: public_key };
        let encoded = ckex.encode();
        
        let handshake = self.encode_handshake(16, &encoded);
        
        Ok(handshake)
    }

    fn encode_handshake(&self, msg_type: u8, data: &[u8]) -> Vec<u8> {
        let mut record = Vec::new();
        
        record.extend_from_slice(&[msg_type]);
        record.extend_from_slice(&0x0303u16.to_be_bytes());
        record.extend_from_slice(&[0, 0]);
        
        let len_pos = record.len();
        record.extend_from_slice(&(data.len() as u32).to_be_bytes());
        
        let mut msg = vec![0u8; 4];
        msg[0] = msg_type;
        msg[1..3].copy_from_slice(&(data.len() as u16).to_be_bytes());
        
        let mut tid = [0u8; 3];
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u32;
        tid[0] = (t >> 16) as u8;
        tid[1] = (t >> 8) as u8;
        tid[2] = t as u8;
        msg[3] = tid[0];
        msg.extend_from_slice(&data);
        
        let msg_len = msg.len() - 4;
        msg[1..3].copy_from_slice(&(msg_len as u16).to_be_bytes());
        
        record.extend_from_slice(&msg);
        
        let data_len = record.len() - len_pos - 4;
        record[len_pos..len_pos+4].copy_from_slice(&(data_len as u32).to_be_bytes());
        
        record
    }

    pub fn is_complete(&self) -> bool {
        self.state == HandshakeState::Complete
    }

    pub fn derive_master_secret(&mut self, premaster: &[u8]) -> Result<[u8; 48], HandshakeError> {
        let remote = self.remote_random.ok_or(HandshakeError::InvalidState)?;
        let mut master = [0u8; 48];
        
        let seed: Vec<u8> = remote.iter()
            .chain(self.local_random.iter())
            .cloned()
            .collect();
        
        use ring::hkdf::{Salt, HKDF_SHA256};
        let salt = Salt::new(HKDF_SHA256, premaster);
        let prk = salt.extract(&[]);
        if let Ok(okm) = prk.expand(&[&seed[..]], HKDF_SHA256) {
            let _ = okm.fill(&mut master);
        }
        
        self.master_secret = Some(master);
        Ok(master)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_hello_generation() {
        let mut handshake = DtlsHandshake::new_client().unwrap();
        let hello = handshake.create_client_hello().unwrap();
        assert!(hello.len() > 0);
    }

    #[test]
    fn server_hello_parsing() {
        let data = vec![
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x20, 0xc0, 0x2b, 0x00, 0x00,
        ];
        let hello = ServerHello::decode(&data).unwrap();
        assert_eq!(hello.cipher_suite, 0xc02b);
    }
}

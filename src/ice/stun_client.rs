//! STUN client for ICE connectivity checks.
//!
//! Implements STUN (RFC 8489) binding requests for ICE.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StunClientError {
    #[error("STUN parsing error: {0}")]
    ParseError(String),
    #[error("STUN encoding error: {0}")]
    EncodeError(String),
    #[error("timeout waiting for response")]
    Timeout,
    #[error("transaction failed: {0}")]
    TransactionFailed(String),
}
#[derive(Clone, Debug)]
pub struct TransactionId(pub [u8; 12]);

impl TransactionId {
    pub fn new() -> Self {
        let mut id = [0u8; 12];
        // Use random bytes - in production use proper RNG
        for (i, b) in id.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(17).wrapping_add(42);
        }
        Self(id)
    }

    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.0
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Copy, Debug)]
pub enum StunClass {
    Request,
    SuccessResponse,
    ErrorResponse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StunMethod {
    Binding,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StunAttributeType {
    MappedAddress,
    SourceAddress,
    ChangedAddress,
    Username,
    Password,
    MessageIntegrity,
    Fingerprint,
    XorMappedAddress,
    UseCandidate,
    Priority,
    IceControlled,
    IceControlling,
}
pub struct StunMessage {
    pub class: StunClass,
    pub method: StunMethod,
    pub transaction_id: TransactionId,
    pub attributes: Vec<StunAttribute>,
}

#[derive(Clone, Debug)]
pub enum StunAttribute {
    MappedAddress(SocketAddr),
    SourceAddress(SocketAddr),
    ChangedAddress(SocketAddr),
    XorMappedAddress(SocketAddr),
    Username(Vec<u8>),
    Password(Vec<u8>),
    MessageIntegrity(Vec<u8>),
    Fingerprint(u32),
    Priority(u32),
    UseCandidate,
    IceControlled(u64),
    IceControlling(u64),
}

impl StunMessage {
    pub fn new_binding_request() -> Self {
        Self {
            class: StunClass::Request,
            method: StunMethod::Binding,
            transaction_id: TransactionId::new(),
            attributes: Vec::new(),
        }
    }
    pub fn with_username(mut self, username: &str) -> Self {
        self.attributes.push(StunAttribute::Username(username.as_bytes().to_vec()));
        self
    }
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.attributes.push(StunAttribute::Priority(priority));
        self
    }
    pub fn with_use_candidate(mut self) -> Self {
        self.attributes.push(StunAttribute::UseCandidate);
        self
    }
    pub fn with_ice_controlling(mut self, tie_breaker: u64) -> Self {
        self.attributes.push(StunAttribute::IceControlling(tie_breaker));
        self
    }
    pub fn with_ice_controlled(mut self, tie_breaker: u64) -> Self {
        self.attributes.push(StunAttribute::IceControlled(tie_breaker));
        self
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // STUN header
        // First 2 bytes: message type (0x0001 for Binding Request)
        let msg_type: u16 = match (self.class, self.method) {
            (StunClass::Request, StunMethod::Binding) => 0x0001,
            (StunClass::SuccessResponse, StunMethod::Binding) => 0x0101,
            (StunClass::ErrorResponse, StunMethod::Binding) => 0x0111,
            _ => 0x0001,
        };
        buf.extend_from_slice(&msg_type.to_be_bytes());

        // Message length (will be updated)
        let length_pos = buf.len();
        buf.extend_from_slice(&0u16.to_be_bytes());

        // Magic cookie (0x2112A442)
        buf.extend_from_slice(&0x2112A442u32.to_be_bytes());

        // Transaction ID
        buf.extend_from_slice(self.transaction_id.as_bytes());

        // Attributes
        for attr in &self.attributes {
            self.encode_attribute(attr, &mut buf);
        }

        // Update length
        let length = (buf.len() - 20) as u16;
        buf[length_pos..length_pos + 2].copy_from_slice(&length.to_be_bytes());

        // Add FINGERPRINT if not present
        if !self.attributes.iter().any(|a| matches!(a, StunAttribute::Fingerprint(_))) {
            // CRC32 of message (excluding fingerprint)
            let crc = self.calculate_fingerprint(&buf);
            self.encode_attribute(&StunAttribute::Fingerprint(crc), &mut buf);
        }

        buf
    }

    fn encode_attribute(&self, attr: &StunAttribute, buf: &mut Vec<u8>) {
        match attr {
            StunAttribute::MappedAddress(addr) => {
                self.encode_xor_address(0x0001, *addr, buf);
            }
            StunAttribute::SourceAddress(addr) => {
                self.encode_xor_address(0x0004, *addr, buf);
            }
            StunAttribute::ChangedAddress(addr) => {
                self.encode_xor_address(0x0005, *addr, buf);
            }
            StunAttribute::XorMappedAddress(addr) => {
                self.encode_xor_address(0x0020, *addr, buf);
            }
            StunAttribute::Username(data) => {
                self.encode_tlv(0x0006, data, buf);
            }
            StunAttribute::Password(data) => {
                self.encode_tlv(0x0007, data, buf);
            }
            StunAttribute::MessageIntegrity(data) => {
                self.encode_tlv(0x0008, data, buf);
            }
            StunAttribute::Fingerprint(crc) => {
                self.encode_tlv(0x8028, &crc.to_be_bytes(), buf);
            }
            StunAttribute::Priority(priority) => {
                self.encode_tlv(0x0024, &priority.to_be_bytes(), buf);
            }
            StunAttribute::UseCandidate => {
                // 4-byte value with padding
                self.encode_tlv(0x0025, &[0, 0, 0, 0], buf);
            }
            StunAttribute::IceControlled(tie) => {
                self.encode_tlv(0x8029, &tie.to_be_bytes(), buf);
            }
            StunAttribute::IceControlling(tie) => {
                self.encode_tlv(0x802A, &tie.to_be_bytes(), buf);
            }
        }
    }

    fn encode_xor_address(&self, attr_type: u16, addr: SocketAddr, buf: &mut Vec<u8>) {
        let mut value = vec![0u8, 0]; // reserved + family
        let xor_ip = match addr.ip() {
            std::net::IpAddr::V4(ip) => {
                let mut bytes = ip.octets();
                let magic = 0x2112A442u32.to_be_bytes();
                for (i, b) in bytes.iter_mut().enumerate() {
                    *b ^= magic[i];
                }
                value.push(1); // IPv4
                bytes.to_vec()
            }
            std::net::IpAddr::V6(ip) => {
                let mut bytes = ip.octets();
                let magic = 0x2112A442u32.to_be_bytes();
                for (i, b) in bytes.iter_mut().enumerate() {
                    *b ^= magic[i % 4];
                }
                value.push(2); // IPv6
                bytes.to_vec()
            }
        };
        value.extend(xor_ip);

        // XOR port
        let xor_port = addr.port() ^ (0x2112 >> 16) as u16;
        value[2..4].copy_from_slice(&xor_port.to_be_bytes());

        self.encode_tlv(attr_type, &value, buf);
    }

    fn encode_tlv(&self, attr_type: u16, value: &[u8], buf: &mut Vec<u8>) {
        // Pad to 4 bytes
        let padding = (4 - (value.len() % 4)) % 4;
        let total_len = value.len() + padding;

        buf.extend_from_slice(&attr_type.to_be_bytes());
        buf.extend_from_slice(&(total_len as u16).to_be_bytes());
        buf.extend_from_slice(value);
        buf.extend(vec![0u8; padding]);
    }

    fn calculate_fingerprint(&self, msg: &[u8]) -> u32 {
        // Simplified CRC32 - in production use proper implementation
        let mut crc: u32 = 0xFFFFFFFF;
        for byte in msg {
            crc ^= *byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB88320 & !(crc & 1));
            }
        }
        !crc
    }
    pub fn decode(data: &[u8]) -> Result<Self, StunClientError> {
        if data.len() < 20 {
            return Err(StunClientError::ParseError("Message too short".into()));
        }

        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        let length = u16::from_be_bytes([data[2], data[3]]);
        let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

        if magic != 0x2112A442 {
            return Err(StunClientError::ParseError("Invalid magic cookie".into()));
        }

        let transaction_id = TransactionId(data[8..20].try_into().unwrap());

        let (class, method) = match msg_type {
            0x0001 => (StunClass::Request, StunMethod::Binding),
            0x0101 => (StunClass::SuccessResponse, StunMethod::Binding),
            0x0111 => (StunClass::ErrorResponse, StunMethod::Binding),
            _ => return Err(StunClientError::ParseError(format!("Unknown message type: {:04x}", msg_type))),
        };

        let mut attributes = Vec::new();
        let mut offset = 20;

        while offset < data.len() - 4 {
            let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let attr_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            offset += 4;

            if offset + attr_len as usize > data.len() {
                break;
            }

            let attr_value = &data[offset..offset + attr_len as usize];

            match attr_type {
                0x0001 | 0x0004 | 0x0005 => {
                    // MappedAddress, SourceAddress, ChangedAddress
                    if attr_value.len() >= 4 {
                        let port = u16::from_be_bytes([attr_value[2], attr_value[3]]);
                        let ip = if attr_value[1] == 1 {
                            std::net::Ipv4Addr::new(attr_value[4], attr_value[5], attr_value[6], attr_value[7])
                        } else {
                            std::net::Ipv6Addr::new(
                                u16::from_be_bytes([attr_value[4], attr_value[5]]),
                                u16::from_be_bytes([attr_value[6], attr_value[7]]),
                                u16::from_be_bytes([attr_value[8], attr_value[9]]),
                                u16::from_be_bytes([attr_value[10], attr_value[11]]),
                                u16::from_be_bytes([attr_value[12], attr_value[13]]),
                                u16::from_be_bytes([attr_value[14], attr_value[15]]),
                                u16::from_be_bytes([attr_value[16], attr_value[17]]),
                                u16::from_be_bytes([attr_value[18], attr_value[19]]),
                            )
                        };
                        let addr = SocketAddr::new(std::net::IpAddr::V6(ip), port);
                        match attr_type {
                            0x0001 => attributes.push(StunAttribute::MappedAddress(addr)),
                            0x0004 => attributes.push(StunAttribute::SourceAddress(addr)),
                            0x0005 => attributes.push(StunAttribute::ChangedAddress(addr)),
                            _ => {}
                        }
                    }
                }
                0x0020 => {
                    // XorMappedAddress
                    if attr_value.len() >= 4 {
                        let xor_port = u16::from_be_bytes([attr_value[2], attr_value[3]]);
                        let port = xor_port ^ (0x2112 >> 16) as u16;
                        let ip = if attr_value[1] == 1 {
                            let mut bytes = [attr_value[4], attr_value[5], attr_value[6], attr_value[7]];
                            let magic = 0x2112A442u32.to_be_bytes();
                            for (i, b) in bytes.iter_mut().enumerate() {
                                *b ^= magic[i];
                            }
                            std::net::Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])
                        } else {
                            std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)
                        };
                        let addr = SocketAddr::new(std::net::IpAddr::V6(ip), port);
                        attributes.push(StunAttribute::XorMappedAddress(addr));
                    }
                }
                0x0024 => {
                    // Priority
                    if attr_value.len() >= 4 {
                        let priority = u32::from_be_bytes(attr_value[0..4].try_into().unwrap());
                        attributes.push(StunAttribute::Priority(priority));
                    }
                }
                0x0025 => {
                    // UseCandidate
                    attributes.push(StunAttribute::UseCandidate);
                }
                _ => {}
            }

            // Align to 4 bytes
            let padding = (4 - (attr_len % 4)) % 4;
            offset += attr_len as usize + padding;
        }

        Ok(Self {
            class,
            method,
            transaction_id,
            attributes,
        })
    }
    pub fn mapped_address(&self) -> Option<SocketAddr> {
        for attr in &self.attributes {
            if let StunAttribute::XorMappedAddress(addr) = attr {
                return Some(*addr);
            }
            if let StunAttribute::MappedAddress(addr) = attr {
                return Some(*addr);
            }
        }
        None
    }
}
pub struct StunClient {
    local_addr: SocketAddr,
    username: String,
    password: String,
}

impl StunClient {
    pub fn new(local_addr: SocketAddr, username: &str, password: &str) -> Self {
        Self {
            local_addr,
            username: username.to_string(),
            password: password.to_string(),
        }
    }
    pub fn create_binding_request(&self, tie_breaker: u64, use_candidate: bool) -> StunMessage {
        // Use inline priority calculation to avoid import issues
        let priority = compute_priority_internal(CandidateType::ServerReflexive, 1, 65535);

        let mut msg = StunMessage::new_binding_request()
            .with_username(&self.username)
            .with_priority(priority)
            .with_ice_controlling(tie_breaker);

        if use_candidate {
            msg = msg.with_use_candidate();
        }

        msg
    }
}

#[derive(Clone, Copy)]
enum CandidateType {
    Host,
    ServerReflexive,
    PeerReflexive,
    Relayed,
    Unknown,
}

fn compute_priority_internal(candidate_type: CandidateType, component_id: u16, local_pref: u32) -> u32 {
    let type_preference = match candidate_type {
        CandidateType::Host => 126,
        CandidateType::ServerReflexive => 100,
        CandidateType::PeerReflexive => 110,
        CandidateType::Relayed => 0,
        CandidateType::Unknown => 0,
    };
    (local_pref << 1) | (65535 - component_id.min(65535) as u32) + type_preference
}

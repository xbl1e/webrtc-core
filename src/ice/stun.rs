use thiserror::Error;

pub const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
pub const STUN_HEADER_SIZE: usize = 20;

#[derive(Error, Debug)]
pub enum StunError {
    #[error("buffer too short")]
    TooShort,
    #[error("invalid magic cookie")]
    InvalidMagic,
    #[error("invalid message length")]
    InvalidLength,
    #[error("unknown attribute type {0}")]
    UnknownAttribute(u16),
    #[error("attribute too large")]
    AttributeTooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StunClass {
    Request,
    Indication,
    SuccessResponse,
    ErrorResponse,
}

impl StunClass {
    fn bits(&self) -> u16 {
        match self {
            StunClass::Request => 0x0000,
            StunClass::Indication => 0x0010,
            StunClass::SuccessResponse => 0x0100,
            StunClass::ErrorResponse => 0x0110,
        }
    }

    pub fn from_bits(bits: u16) -> Option<Self> {
        let _class_bits = (bits & 0x0110) >> 4 | (bits & 0x0001);
        let c_bits = ((bits >> 7) & 0x2) | ((bits >> 4) & 0x1);
        match c_bits {
            0x0 => Some(StunClass::Request),
            0x1 => Some(StunClass::Indication),
            0x2 => Some(StunClass::SuccessResponse),
            0x3 => Some(StunClass::ErrorResponse),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StunMethod {
    Binding = 0x0001,
    Allocate = 0x0003,
    Refresh = 0x0004,
    Send = 0x0006,
    Data = 0x0007,
    CreatePermission = 0x0008,
    ChannelBind = 0x0009,
}

impl StunMethod {
    pub fn bits(&self) -> u16 {
        *self as u16
    }

    pub fn from_bits(bits: u16) -> Option<Self> {
        match bits & 0x3EEF {
            0x0001 => Some(StunMethod::Binding),
            0x0003 => Some(StunMethod::Allocate),
            0x0004 => Some(StunMethod::Refresh),
            0x0006 => Some(StunMethod::Send),
            0x0007 => Some(StunMethod::Data),
            0x0008 => Some(StunMethod::CreatePermission),
            0x0009 => Some(StunMethod::ChannelBind),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum StunAttribute {
    MappedAddress { family: u8, port: u16, addr: [u8; 16], addr_len: u8 },
    XorMappedAddress { family: u8, port: u16, addr: [u8; 16], addr_len: u8 },
    Username(Vec<u8>),
    MessageIntegrity([u8; 20]),
    Fingerprint(u32),
    Priority(u32),
    UseCandidate,
    IceControlled(u64),
    IceControlling(u64),
    Software(Vec<u8>),
    ErrorCode { code: u16, reason: Vec<u8> },
    Unknown { attr_type: u16, data: Vec<u8> },
}

impl StunAttribute {
    pub fn attr_type(&self) -> u16 {
        match self {
            StunAttribute::MappedAddress { .. } => 0x0001,
            StunAttribute::XorMappedAddress { .. } => 0x0020,
            StunAttribute::Username(_) => 0x0006,
            StunAttribute::MessageIntegrity(_) => 0x0008,
            StunAttribute::Fingerprint(_) => 0x8028,
            StunAttribute::Priority(_) => 0x0024,
            StunAttribute::UseCandidate => 0x0025,
            StunAttribute::IceControlled(_) => 0x8029,
            StunAttribute::IceControlling(_) => 0x802A,
            StunAttribute::Software(_) => 0x8022,
            StunAttribute::ErrorCode { .. } => 0x0009,
            StunAttribute::Unknown { attr_type, .. } => *attr_type,
        }
    }
}

pub struct StunMessage {
    pub method: StunMethod,
    pub class: StunClass,
    pub transaction_id: [u8; 12],
    pub attributes: Vec<StunAttribute>,
}

impl StunMessage {
    pub fn new_request(method: StunMethod) -> Self {
        let mut tid = [0u8; 12];
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        tid[0..8].copy_from_slice(&t.to_le_bytes());
        Self { method, class: StunClass::Request, transaction_id: tid, attributes: Vec::new() }
    }

    pub fn new_response(method: StunMethod, transaction_id: [u8; 12]) -> Self {
        Self { method, class: StunClass::SuccessResponse, transaction_id, attributes: Vec::new() }
    }

    pub fn add_attribute(&mut self, attr: StunAttribute) {
        self.attributes.push(attr);
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize, StunError> {
        let mut attr_buf = [0u8; 4096];
        let mut attr_len = 0usize;
        for attr in &self.attributes {
            let written = self.encode_attr(attr, &mut attr_buf[attr_len..])?;
            attr_len += written;
        }
        let total = STUN_HEADER_SIZE + attr_len;
        if out.len() < total {
            return Err(StunError::TooShort);
        }
        let msg_type = self.encode_type();
        out[0..2].copy_from_slice(&msg_type.to_be_bytes());
        out[2..4].copy_from_slice(&(attr_len as u16).to_be_bytes());
        out[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        out[8..20].copy_from_slice(&self.transaction_id);
        out[STUN_HEADER_SIZE..total].copy_from_slice(&attr_buf[..attr_len]);
        Ok(total)
    }

    fn encode_type(&self) -> u16 {
        let method = self.method.bits();
        let class = self.class.bits();
        let m_low = method & 0x000F;
        let m_mid = (method >> 4) & 0x0007;
        let m_high = (method >> 7) & 0x001F;
        let c_low = (class >> 4) & 0x0001;
        let c_high = class & 0x0001;
        m_high << 9 | c_high << 8 | m_mid << 5 | c_low << 4 | m_low
    }

    fn encode_attr(&self, attr: &StunAttribute, out: &mut [u8]) -> Result<usize, StunError> {
        let attr_type = attr.attr_type();
        let mut value_buf = [0u8; 256];
        let vlen = match attr {
            StunAttribute::Priority(p) => {
                value_buf[0..4].copy_from_slice(&p.to_be_bytes());
                4
            }
            StunAttribute::UseCandidate => 0,
            StunAttribute::IceControlled(tie) | StunAttribute::IceControlling(tie) => {
                value_buf[0..8].copy_from_slice(&tie.to_be_bytes());
                8
            }
            StunAttribute::Username(u) => {
                let l = u.len().min(256);
                value_buf[..l].copy_from_slice(&u[..l]);
                l
            }
            StunAttribute::MessageIntegrity(hmac) => {
                value_buf[..20].copy_from_slice(hmac);
                20
            }
            StunAttribute::Fingerprint(crc) => {
                value_buf[0..4].copy_from_slice(&crc.to_be_bytes());
                4
            }
            StunAttribute::Software(s) => {
                let l = s.len().min(256);
                value_buf[..l].copy_from_slice(&s[..l]);
                l
            }
            StunAttribute::Unknown { data, .. } => {
                let l = data.len().min(256);
                value_buf[..l].copy_from_slice(&data[..l]);
                l
            }
            StunAttribute::MappedAddress { family, port, addr, addr_len } => {
                value_buf[0] = 0;
                value_buf[1] = *family;
                value_buf[2..4].copy_from_slice(&port.to_be_bytes());
                let al = *addr_len as usize;
                value_buf[4..4 + al].copy_from_slice(&addr[..al]);
                4 + al
            }
            StunAttribute::XorMappedAddress { family, port, addr, addr_len } => {
                value_buf[0] = 0;
                value_buf[1] = *family;
                let xport = port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
                value_buf[2..4].copy_from_slice(&xport.to_be_bytes());
                let al = *addr_len as usize;
                value_buf[4..4 + al].copy_from_slice(&addr[..al]);
                4 + al
            }
            StunAttribute::ErrorCode { code, reason } => {
                let cls = code / 100;
                let num = code % 100;
                value_buf[2] = cls as u8;
                value_buf[3] = num as u8;
                let rl = reason.len().min(252);
                value_buf[4..4 + rl].copy_from_slice(&reason[..rl]);
                4 + rl
            }
        };
        let padded = (vlen + 3) & !3;
        let total = 4 + padded;
        if out.len() < total {
            return Err(StunError::AttributeTooLarge);
        }
        out[0..2].copy_from_slice(&attr_type.to_be_bytes());
        out[2..4].copy_from_slice(&(vlen as u16).to_be_bytes());
        out[4..4 + vlen].copy_from_slice(&value_buf[..vlen]);
        for i in vlen..padded {
            out[4 + i] = 0;
        }
        Ok(total)
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StunError> {
        if buf.len() < STUN_HEADER_SIZE {
            return Err(StunError::TooShort);
        }
        let magic = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        if magic != STUN_MAGIC_COOKIE {
            return Err(StunError::InvalidMagic);
        }
        let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
        let msg_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        if buf.len() < STUN_HEADER_SIZE + msg_len {
            return Err(StunError::InvalidLength);
        }
        let method_bits = msg_type & 0x3EEF;
        let method = StunMethod::from_bits(method_bits)
            .ok_or(StunError::UnknownAttribute(msg_type))?;
        let class = StunClass::from_bits(msg_type)
            .ok_or(StunError::UnknownAttribute(msg_type))?;
        let mut tid = [0u8; 12];
        tid.copy_from_slice(&buf[8..20]);
        let mut attributes = Vec::new();
        let mut off = STUN_HEADER_SIZE;
        let end = STUN_HEADER_SIZE + msg_len;
        while off + 4 <= end {
            let attr_type = u16::from_be_bytes([buf[off], buf[off+1]]);
            let attr_len = u16::from_be_bytes([buf[off+2], buf[off+3]]) as usize;
            off += 4;
            if off + attr_len > end {
                return Err(StunError::InvalidLength);
            }
            let data = buf[off..off + attr_len].to_vec();
            let attr = Self::decode_attr(attr_type, &data);
            attributes.push(attr);
            let padded = (attr_len + 3) & !3;
            off += padded;
        }
        Ok(Self { method, class, transaction_id: tid, attributes })
    }

    fn decode_attr(attr_type: u16, data: &[u8]) -> StunAttribute {
        match attr_type {
            0x0006 => StunAttribute::Username(data.to_vec()),
            0x0008 => {
                let mut hmac = [0u8; 20];
                if data.len() >= 20 { hmac.copy_from_slice(&data[..20]); }
                StunAttribute::MessageIntegrity(hmac)
            }
            0x8028 => {
                let crc = if data.len() >= 4 { u32::from_be_bytes([data[0], data[1], data[2], data[3]]) } else { 0 };
                StunAttribute::Fingerprint(crc)
            }
            0x0024 => {
                let p = if data.len() >= 4 { u32::from_be_bytes([data[0], data[1], data[2], data[3]]) } else { 0 };
                StunAttribute::Priority(p)
            }
            0x0025 => StunAttribute::UseCandidate,
            0x8029 => {
                let t = if data.len() >= 8 { u64::from_be_bytes(data[..8].try_into().unwrap_or([0u8;8])) } else { 0 };
                StunAttribute::IceControlled(t)
            }
            0x802A => {
                let t = if data.len() >= 8 { u64::from_be_bytes(data[..8].try_into().unwrap_or([0u8;8])) } else { 0 };
                StunAttribute::IceControlling(t)
            }
            _ => StunAttribute::Unknown { attr_type, data: data.to_vec() },
        }
    }

    pub fn transaction_id_matches(&self, other: &[u8; 12]) -> bool {
        self.transaction_id == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stun_binding_request_roundtrip() {
        let mut msg = StunMessage::new_request(StunMethod::Binding);
        msg.add_attribute(StunAttribute::Priority(1234567));
        msg.add_attribute(StunAttribute::UseCandidate);
        let mut buf = [0u8; 512];
        let n = msg.encode(&mut buf).unwrap();
        assert!(n >= STUN_HEADER_SIZE);
        let decoded = StunMessage::decode(&buf[..n]).unwrap();
        assert_eq!(decoded.method, StunMethod::Binding);
        assert_eq!(decoded.class, StunClass::Request);
        let has_priority = decoded.attributes.iter().any(|a| matches!(a, StunAttribute::Priority(_)));
        assert!(has_priority);
        let has_use_cand = decoded.attributes.iter().any(|a| matches!(a, StunAttribute::UseCandidate));
        assert!(has_use_cand);
    }

    #[test]
    fn stun_invalid_magic_rejected() {
        let buf = [0x00u8, 0x01, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF, 0,0,0,0,0,0,0,0,0,0,0,0];
        assert!(StunMessage::decode(&buf).is_err());
    }
}

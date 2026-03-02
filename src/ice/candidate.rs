use std::net::SocketAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateType {
    Host,
    ServerReflexive,
    PeerReflexive,
    Relay,
}

impl CandidateType {
    pub fn type_preference(&self) -> u32 {
        match self {
            CandidateType::Host => 126,
            CandidateType::PeerReflexive => 110,
            CandidateType::ServerReflexive => 100,
            CandidateType::Relay => 0,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateType::Host => "host",
            CandidateType::PeerReflexive => "prflx",
            CandidateType::ServerReflexive => "srflx",
            CandidateType::Relay => "relay",
        }
    }
}

impl std::fmt::Display for CandidateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportProtocol {
    Udp,
    Tcp,
}

impl TransportProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransportProtocol::Udp => "UDP",
            TransportProtocol::Tcp => "TCP",
        }
    }
}

#[derive(Clone, Debug)]
pub struct IceCandidate {
    pub foundation: [u8; 32],
    pub foundation_len: usize,
    pub component_id: u8,
    pub protocol: TransportProtocol,
    pub priority: u32,
    pub address: SocketAddr,
    pub candidate_type: CandidateType,
    pub related_address: Option<SocketAddr>,
    pub generation: u32,
    pub ufrag: [u8; 32],
    pub ufrag_len: usize,
}

impl IceCandidate {
    pub fn new_host(address: SocketAddr, component: u8) -> Self {
        let priority = Self::compute_priority(CandidateType::Host, 65535, component);
        Self {
            foundation: [0u8; 32],
            foundation_len: 0,
            component_id: component,
            protocol: TransportProtocol::Udp,
            priority,
            address,
            candidate_type: CandidateType::Host,
            related_address: None,
            generation: 0,
            ufrag: [0u8; 32],
            ufrag_len: 0,
        }
    }

    pub fn new_srflx(address: SocketAddr, related: SocketAddr, component: u8) -> Self {
        let priority = Self::compute_priority(CandidateType::ServerReflexive, 65535, component);
        Self {
            foundation: [0u8; 32],
            foundation_len: 0,
            component_id: component,
            protocol: TransportProtocol::Udp,
            priority,
            address,
            candidate_type: CandidateType::ServerReflexive,
            related_address: Some(related),
            generation: 0,
            ufrag: [0u8; 32],
            ufrag_len: 0,
        }
    }

    pub fn compute_priority(
        candidate_type: CandidateType,
        local_preference: u32,
        component_id: u8,
    ) -> u32 {
        (candidate_type.type_preference() << 24)
            | (local_preference << 8)
            | (256u32.saturating_sub(component_id as u32))
    }

    pub fn set_foundation(&mut self, foundation: &str) {
        let bytes = foundation.as_bytes();
        let len = bytes.len().min(32);
        self.foundation[..len].copy_from_slice(&bytes[..len]);
        self.foundation_len = len;
    }

    pub fn foundation_str(&self) -> &str {
        std::str::from_utf8(&self.foundation[..self.foundation_len]).unwrap_or("")
    }

    pub fn set_ufrag(&mut self, ufrag: &str) {
        let bytes = ufrag.as_bytes();
        let len = bytes.len().min(32);
        self.ufrag[..len].copy_from_slice(&bytes[..len]);
        self.ufrag_len = len;
    }

    pub fn to_sdp_attribute(&self) -> String {
        let addr_str = self.address.ip().to_string();
        let port = self.address.port();
        let foundation = self.foundation_str();
        let typ = self.candidate_type.as_str();
        let proto = self.protocol.as_str();
        let mut s = format!(
            "candidate:{} {} {} {} {} {} typ {}",
            foundation, self.component_id, proto, self.priority, addr_str, port, typ
        );
        if let Some(rel) = self.related_address {
            s.push_str(&format!(" raddr {} rport {}", rel.ip(), rel.port()));
        }
        s
    }
}

impl std::fmt::Display for IceCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_sdp_attribute())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn candidate_priority_ordering() {
        let host_prio = IceCandidate::compute_priority(CandidateType::Host, 65535, 1);
        let srflx_prio = IceCandidate::compute_priority(CandidateType::ServerReflexive, 65535, 1);
        let relay_prio = IceCandidate::compute_priority(CandidateType::Relay, 65535, 1);
        assert!(host_prio > srflx_prio);
        assert!(srflx_prio > relay_prio);
    }

    #[test]
    fn candidate_sdp_format() {
        let addr: SocketAddr = "192.168.1.1:5000".parse().unwrap();
        let mut c = IceCandidate::new_host(addr, 1);
        c.set_foundation("1");
        let sdp = c.to_sdp_attribute();
        assert!(sdp.contains("typ host"));
        assert!(sdp.contains("192.168.1.1"));
        assert!(sdp.contains("5000"));
    }
}

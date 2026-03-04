use std::net::SocketAddr;
use std::sync::Arc;
use parking_lot::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TurnError {
    #[error("connection failed")]
    ConnectionFailed,
    #[error("authentication failed")]
    AuthFailed,
    #[error("allocation failed")]
    AllocationFailed,
    #[error("permission denied")]
    PermissionDenied,
    #[error("channel error")]
    ChannelError,
    #[error("timeout")]
    Timeout,
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("not connected")]
    NotConnected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Connecting,
    Authenticating,
    Allocated,
    Refreshing,
    Closed,
}

#[derive(Clone, Debug)]
pub struct TurnAllocation {
    pub relayed_addr: SocketAddr,
    pub mapped_addr: Option<SocketAddr>,
    pub lifetime: u32,
    pub bandwidth: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct TurnPermission {
    pub peer_addr: SocketAddr,
    pub channel: u16,
    pub expires: std::time::Instant,
}

pub struct TurnClient {
    server: SocketAddr,
    username: String,
    password: String,
    state: Mutex<TurnState>,
    allocation: Mutex<Option<TurnAllocation>>,
    permissions: Mutex<Vec<TurnPermission>>,
    channel_bindings: Mutex<Vec<(SocketAddr, u16)>>,
    transaction_id: Mutex<u32>,
    realm: Mutex<Option<String>>,
    nonce: Mutex<Option<String>>,
}

impl TurnClient {
    pub fn new(server: SocketAddr, username: String, password: String) -> Self {
        Self {
            server,
            username,
            password,
            state: Mutex::new(TurnState::Idle),
            allocation: Mutex::new(None),
            permissions: Mutex::new(Vec::new()),
            channel_bindings: Mutex::new(Vec::new()),
            transaction_id: Mutex::new(0),
            realm: Mutex::new(None),
            nonce: Mutex::new(None),
        }
    }

    pub fn state(&self) -> TurnState {
        *self.state.lock()
    }

    pub fn is_allocated(&self) -> bool {
        *self.state.lock() == TurnState::Allocated
    }

    pub fn allocation(&self) -> Option<TurnAllocation> {
        self.allocation.lock().clone()
    }

    pub fn relayed_address(&self) -> Option<SocketAddr> {
        self.allocation.lock().as_ref().map(|a| a.relayed_addr)
    }

    fn next_transaction_id(&self) -> [u8; 12] {
        let mut tid = *self.transaction_id.lock();
        tid = tid.wrapping_add(1);
        *self.transaction_id.lock() = tid;
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&tid.to_be_bytes());
        bytes
    }

    pub fn create_allocation_request(&self) -> Vec<u8> {
        let mut msg = vec![0u8; 20];
        msg[0..2].copy_from_slice(&(0x0003u16).to_be_bytes());
        let tid = self.next_transaction_id();
        msg[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
        msg[8..20].copy_from_slice(&tid);
        
        let mut pos = 20;
        msg.resize(pos + 4 + 4, 0);
        msg[pos..pos+2].copy_from_slice(&0x000au16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&4u16.to_be_bytes());
        msg[pos+4..pos+8].copy_from_slice(&3600u32.to_be_bytes());
        
        let total_len = msg.len();
        msg[2..4].copy_from_slice(&(total_len as u16 - 20).to_be_bytes());
        msg
    }

    pub fn create_permission_request(&self, peer: SocketAddr) -> Vec<u8> {
        let mut msg = vec![0u8; 20];
        msg[0..2].copy_from_slice(&(0x0008u16).to_be_bytes());
        let tid = self.next_transaction_id();
        msg[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
        msg[8..20].copy_from_slice(&tid);
        
        let mut pos = 20;
        msg.resize(pos + 8 + 4, 0);
        msg[pos..pos+2].copy_from_slice(&0x000cu16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&4u16.to_be_bytes());
        let xport = peer.port() ^ (0x2112 >> 16) as u16;
        msg[pos+4..pos+6].copy_from_slice(&1u16.to_be_bytes());
        msg[pos+6..pos+8].copy_from_slice(&xport.to_be_bytes());
        
        let total_len = msg.len();
        msg[2..4].copy_from_slice(&(total_len as u16 - 20).to_be_bytes());
        msg
    }

    pub fn create_channel_bind_request(&self, peer: SocketAddr, channel: u16) -> Vec<u8> {
        let mut msg = vec![0u8; 20];
        msg[0..2].copy_from_slice(&(0x0009u16).to_be_bytes());
        let tid = self.next_transaction_id();
        msg[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
        msg[8..20].copy_from_slice(&tid);
        
        let mut pos = 20;
        msg.resize(pos + 8 + 8, 0);
        msg[pos..pos+2].copy_from_slice(&0x000cu16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&4u16.to_be_bytes());
        msg[pos+4..pos+6].copy_from_slice(&channel.to_be_bytes());
        msg[pos+6..pos+8].copy_from_slice(&0u16.to_be_bytes());
        
        pos += 8;
        msg[pos..pos+2].copy_from_slice(&0x000cu16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&4u16.to_be_bytes());
        let xport = peer.port() ^ (0x2112 >> 16) as u16;
        msg[pos+4..pos+6].copy_from_slice(&1u16.to_be_bytes());
        msg[pos+6..pos+8].copy_from_slice(&xport.to_be_bytes());
        
        let total_len = msg.len();
        msg[2..4].copy_from_slice(&(total_len as u16 - 20).to_be_bytes());
        msg
    }

    pub fn create_refresh_request(&self, lifetime: u32) -> Vec<u8> {
        let mut msg = vec![0u8; 20];
        msg[0..2].copy_from_slice(&(0x0004u16).to_be_bytes());
        let tid = self.next_transaction_id();
        msg[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
        msg[8..20].copy_from_slice(&tid);
        
        let mut pos = 20;
        msg.resize(pos + 4 + 4, 0);
        msg[pos..pos+2].copy_from_slice(&0x000bu16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&4u16.to_be_bytes());
        msg[pos+4..pos+8].copy_from_slice(&lifetime.to_be_bytes());
        
        let total_len = msg.len();
        msg[2..4].copy_from_slice(&(total_len as u16 - 20).to_be_bytes());
        msg
    }

    pub fn handle_allocation_response(&self, data: &[u8]) -> Result<TurnAllocation, TurnError> {
        if data.len() < 20 {
            return Err(TurnError::Protocol("response too short".to_string()));
        }
        
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        if msg_type != 0x0103 {
            return Err(TurnError::Protocol(format!("unexpected response type: {:04x}", msg_type)));
        }
        
        let mut relayed_addr = None;
        let mut mapped_addr = None;
        let mut lifetime = 3600u32;
        
        let mut pos = 20;
        while pos + 4 <= data.len() {
            let attr_type = u16::from_be_bytes([data[pos], data[pos+1]]);
            let attr_len = u16::from_be_bytes([data[pos+2], data[pos+3]]) as usize;
            pos += 4;
            
            if pos + attr_len > data.len() {
                break;
            }
            
            match attr_type {
                0x0012 => {
                    if attr_len >= 8 {
                        let family = data[pos];
                        if family == 1 {
                            let port = u16::from_be_bytes([data[pos+2], data[pos+3]]);
                            let addr_bytes = &data[pos+4..pos+8];
                            let addr = format!("{}.{}.{}.{}", addr_bytes[0], addr_bytes[1], addr_bytes[2], addr_bytes[3]);
                            relayed_addr = Some(SocketAddr::new(addr.parse().unwrap(), port));
                        }
                    }
                }
                0x0020 => {
                    if attr_len >= 8 {
                        let port = u16::from_be_bytes([data[pos+2], data[pos+3]]) ^ (0x2112 >> 16) as u16;
                        let addr_bytes = &data[pos+4..pos+8];
                        let addr = format!("{}.{}.{}.{}", addr_bytes[0], addr_bytes[1], addr_bytes[2], addr_bytes[3]);
                        mapped_addr = Some(SocketAddr::new(addr.parse().unwrap(), port));
                    }
                }
                0x000f => {
                    if attr_len >= 4 {
                        lifetime = u32::from_be_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
                    }
                }
                _ => {}
            }
            
            let padded = (attr_len + 3) & !3;
            pos += padded;
        }
        
        let relayed = relayed_addr.ok_or(TurnError::AllocationFailed)?;
        let alloc = TurnAllocation {
            relayed_addr: relayed,
            mapped_addr,
            lifetime,
            bandwidth: None,
        };
        
        *self.state.lock() = TurnState::Allocated;
        *self.allocation.lock() = Some(alloc.clone());
        
        Ok(alloc)
    }

    pub fn add_permission(&self, peer: SocketAddr) -> Result<u16, TurnError> {
        if !self.is_allocated() {
            return Err(TurnError::NotConnected);
        }
        
        let channel = 0x4000 | (self.permissions.lock().len() as u16 & 0x3FFF);
        
        let perm = TurnPermission {
            peer_addr: peer,
            channel,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(600),
        };
        
        self.permissions.lock().push(perm);
        self.channel_bindings.lock().push((peer, channel));
        
        Ok(channel)
    }

    pub fn send_indication(&self, data: &[u8], peer: SocketAddr) -> Result<Vec<u8>, TurnError> {
        if !self.is_allocated() {
            return Err(TurnError::NotConnected);
        }
        
        let mut msg = vec![0u8; 20 + 4 + 4 + data.len()];
        msg[0..2].copy_from_slice(&(0x0017u16).to_be_bytes());
        let tid = self.next_transaction_id();
        msg[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
        msg[8..20].copy_from_slice(&tid);
        
        msg[20..22].copy_from_slice(&0x0014u16.to_be_bytes());
        msg[22..24].copy_from_slice(&4u16.to_be_bytes());
        let xport = peer.port() ^ (0x2112 >> 16) as u16;
        msg[24..26].copy_from_slice(&1u16.to_be_bytes());
        msg[26..28].copy_from_slice(&xport.to_be_bytes());
        
        msg[28..32].copy_from_slice(&0x0013u16.to_be_bytes());
        msg[32..36].copy_from_slice(&(data.len() as u16).to_be_bytes());
        msg[36..36+data.len()].copy_from_slice(data);
        
        msg[2..4].copy_from_slice(&(msg.len() as u16 - 20).to_be_bytes());
        
        Ok(msg)
    }

    pub fn create_send_indication(&self, data: &[u8], peer: SocketAddr) -> Vec<u8> {
        let mut msg = vec![0u8; 20];
        msg[0..2].copy_from_slice(&(0x0006u16).to_be_bytes());
        let tid = self.next_transaction_id();
        msg[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
        msg[8..20].copy_from_slice(&tid);
        
        let mut pos = 20;
        
        msg.resize(pos + 4 + 4, 0);
        msg[pos..pos+2].copy_from_slice(&0x0014u16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&4u16.to_be_bytes());
        let xport = peer.port() ^ (0x2112 >> 16) as u16;
        msg[pos+4..pos+6].copy_from_slice(&1u16.to_be_bytes());
        msg[pos+6..pos+8].copy_from_slice(&xport.to_be_bytes());
        
        pos += 8;
        msg.resize(pos + 4 + data.len(), 0);
        msg[pos..pos+2].copy_from_slice(&0x0013u16.to_be_bytes());
        msg[pos+2..pos+4].copy_from_slice(&(data.len() as u16).to_be_bytes());
        msg[pos+4..pos+4+data.len()].copy_from_slice(data);
        
        let total_len = msg.len();
        msg[2..4].copy_from_slice(&(total_len as u16 - 20).to_be_bytes());
        
        msg
    }

    pub fn close(&self) {
        *self.state.lock() = TurnState::Closed;
        *self.allocation.lock() = None;
        self.permissions.lock().clear();
        self.channel_bindings.lock().clear();
    }
}

pub struct TurnClientPool {
    clients: Mutex<Vec<Arc<TurnClient>>>,
    default_server: Mutex<Option<SocketAddr>>,
}

impl TurnClientPool {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(Vec::new()),
            default_server: Mutex::new(None),
        }
    }

    pub fn set_default_server(&self, server: SocketAddr) {
        *self.default_server.lock() = Some(server);
    }

    pub fn create_client(&self, server: SocketAddr, username: String, password: String) -> Arc<TurnClient> {
        let client = Arc::new(TurnClient::new(server, username, password));
        self.clients.lock().push(client.clone());
        client
    }

    pub fn get_or_create_client(&self, server: SocketAddr, username: String, password: String) -> Arc<TurnClient> {
        let clients = self.clients.lock();
        if let Some(client) = clients.iter().find(|c| {
            let alloc = c.allocation();
            alloc.is_some() && c.state() == TurnState::Allocated
        }) {
            return client.clone();
        }
        drop(clients);
        self.create_client(server, username, password)
    }
}

impl Default for TurnClientPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_client_creation() {
        let server: SocketAddr = "127.0.0.1:3478".parse().unwrap();
        let client = TurnClient::new(server, "user".to_string(), "pass".to_string());
        assert_eq!(client.state(), TurnState::Idle);
        assert!(!client.is_allocated());
    }

    #[test]
    fn allocation_request_format() {
        let server: SocketAddr = "127.0.0.1:3478".parse().unwrap();
        let client = TurnClient::new(server, "user".to_string(), "pass".to_string());
        let req = client.create_allocation_request();
        assert!(req.len() >= 20);
        assert_eq!(u16::from_be_bytes([req[0], req[1]]), 0x0003);
    }

    #[test]
    fn permission_request_format() {
        let server: SocketAddr = "127.0.0.1:3478".parse().unwrap();
        let client = TurnClient::new(server, "user".to_string(), "pass".to_string());
        let peer: SocketAddr = "192.168.1.100:5000".parse().unwrap();
        let req = client.create_permission_request(peer);
        assert!(req.len() >= 20);
        assert_eq!(u16::from_be_bytes([req[0], req[1]]), 0x0008);
    }
}

use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SctpError {
    #[error("association not found")]
    AssociationNotFound,
    #[error("stream not found")]
    StreamNotFound,
    #[error("buffer full")]
    BufferFull,
    #[error("invalid parameter")]
    InvalidParameter,
    #[error("protocol violation")]
    ProtocolViolation,
    #[error("not connected")]
    NotConnected,
    #[error("closed")]
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SctpState {
    Closed,
    CookieWait,
    CookieEchoed,
    Established,
    ShutdownPending,
    ShutdownReceived,
    ShutdownSent,
    ClosedReceived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkType {
    Data = 0,
    Init = 1,
    InitAck = 2,
    Sack = 3,
    Heartbeat = 4,
    HeartbeatAck = 5,
    Abort = 6,
    Shutdown = 7,
    ShutdownAck = 8,
    Error = 9,
    CookieEcho = 10,
    CookieAck = 11,
    Ecne = 12,
    Cwr = 13,
    ShutdownComplete = 14,
}

#[derive(Clone, Debug)]
pub struct SctpStream {
    pub stream_id: u16,
    pub state: StreamState,
    pub unordered: bool,
    pub max_retransmits: Option<u16>,
    pub pending_messages: VecDeque<SctpMessage>,
    pub buffered_amount: usize,
    pub buffered_amount_low_threshold: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    Open,
    Closing,
    Closed,
}

impl SctpStream {
    pub fn new(stream_id: u16, unordered: bool) -> Self {
        Self {
            stream_id,
            state: StreamState::Idle,
            unordered,
            max_retransmits: None,
            pending_messages: VecDeque::new(),
            buffered_amount: 0,
            buffered_amount_low_threshold: 65536,
        }
    }

    pub fn send(&mut self, data: Vec<u8>) -> Result<(), SctpError> {
        if self.state == StreamState::Closed {
            return Err(SctpError::Closed);
        }
        
        if self.buffered_amount > 16 * 1024 * 1024 {
            return Err(SctpError::BufferFull);
        }
        
        let msg = SctpMessage {
            data,
            ppid: 0,
            unordered: self.unordered,
            complete: true,
        };
        
        self.buffered_amount += msg.data.len();
        self.pending_messages.push_back(msg);
        
        Ok(())
    }

    pub fn send_binary(&mut self, data: Vec<u8>) -> Result<(), SctpError> {
        let mut msg = SctpMessage {
            data,
            ppid: 53,
            unordered: self.unordered,
            complete: true,
        };
        
        self.buffered_amount += msg.data.len();
        self.pending_messages.push_back(msg);
        
        Ok(())
    }

    pub fn send_text(&mut self, text: &str) -> Result<(), SctpError> {
        self.send(text.as_bytes().to_vec())
    }

    pub fn pop_message(&mut self) -> Option<SctpMessage> {
        let msg = self.pending_messages.pop_front()?;
        self.buffered_amount = self.buffered_amount.saturating_sub(msg.data.len());
        Some(msg)
    }

    pub fn buffered_amount(&self) -> usize {
        self.buffered_amount
    }

    pub fn set_buffered_amount_low_threshold(&mut self, threshold: usize) {
        self.buffered_amount_low_threshold = threshold;
    }
}

#[derive(Clone, Debug)]
pub struct SctpMessage {
    pub data: Vec<u8>,
    pub ppid: u32,
    pub unordered: bool,
    pub complete: bool,
}

pub struct SctpAssociation {
    pub association_id: u32,
    pub state: SctpState,
    pub local_port: u16,
    pub remote_port: u16,
    pub local_tag: u32,
    pub remote_tag: u32,
    pub initial_tsn: u32,
    pub initial_remote_tsn: u32,
    pub next_tsn: u32,
    pub last_received_tsn: u32,
    pub streams: Mutex<Vec<SctpStream>>,
    pub pending_outbound: VecDeque<SctpMessage>,
    pub inbound_queue: VecDeque<SctpMessage>,
}

impl SctpAssociation {
    pub fn new_client(local_port: u16, remote_port: u16) -> Self {
        let local_tag = Self::generate_tag();
        
        Self {
            association_id: local_tag,
            state: SctpState::CookieWait,
            local_port,
            remote_port,
            local_tag,
            remote_tag: 0,
            initial_tsn: Self::generate_tsn(),
            initial_remote_tsn: 0,
            next_tsn: Self::generate_tsn(),
            last_received_tsn: 0,
            streams: Mutex::new(Vec::new()),
            pending_outbound: VecDeque::new(),
            inbound_queue: VecDeque::new(),
        }
    }

    pub fn new_server(local_port: u16) -> Self {
        Self {
            association_id: 0,
            state: SctpState::Closed,
            local_port,
            remote_port: 0,
            local_tag: Self::generate_tag(),
            remote_tag: 0,
            initial_tsn: Self::generate_tsn(),
            initial_remote_tsn: 0,
            next_tsn: Self::generate_tsn(),
            last_received_tsn: 0,
            streams: Mutex::new(Vec::new()),
            pending_outbound: VecDeque::new(),
            inbound_queue: VecDeque::new(),
        }
    }

    fn generate_tag() -> u32 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        (nanos as u32) ^ 0x12345678
    }

    fn generate_tsn() -> u32 {
        Self::generate_tag()
    }

    pub fn create_init_chunk(&self) -> Vec<u8> {
        let mut chunk = vec![0u8; 20];
        chunk[0] = ChunkType::Init as u8;
        
        let init_tag = self.local_tag;
        let mut pos = 4;
        
        chunk[pos..pos+4].copy_from_slice(&init_tag.to_be_bytes());
        pos += 4;
        
        chunk[pos..pos+4].copy_from_slice(&self.initial_tsn.to_be_bytes());
        pos += 4;
        
        chunk[pos..pos+2].copy_from_slice(&1u16.to_be_bytes());
        pos += 2;
        
        chunk[pos..pos+2].copy_from_slice(&65535u16.to_be_bytes());
        pos += 2;
        
        chunk[pos..pos+2].copy_from_slice(&0u16.to_be_bytes());
        pos += 2;
        
        let total_len = chunk.len() - 4;
        chunk[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
        
        chunk
    }

    pub fn create_init_ack_chunk(&self) -> Vec<u8> {
        let mut chunk = vec![0u8; 20];
        chunk[0] = ChunkType::InitAck as u8;
        
        let init_tag = self.local_tag;
        let mut pos = 4;
        
        chunk[pos..pos+4].copy_from_slice(&init_tag.to_be_bytes());
        pos += 4;
        
        chunk[pos..pos+4].copy_from_slice(&self.initial_tsn.to_be_bytes());
        pos += 4;
        
        chunk[pos..pos+2].copy_from_slice(&1u16.to_be_bytes());
        pos += 2;
        
        chunk[pos..pos+2].copy_from_slice(&65535u16.to_be_bytes());
        pos += 2;
        
        chunk[pos..pos+2].copy_from_slice(&0u16.to_be_bytes());
        pos += 2;
        
        let total_len = chunk.len() - 4;
        chunk[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
        
        chunk
    }

    pub fn create_cookie_echo_chunk(&self, cookie: &[u8]) -> Vec<u8> {
        let mut chunk = vec![0u8; 4];
        chunk[0] = ChunkType::CookieEcho as u8;
        
        chunk.extend_from_slice(cookie);
        
        let total_len = chunk.len() - 4;
        chunk[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
        
        chunk
    }

    pub fn create_cookie_ack_chunk(&self) -> Vec<u8> {
        let mut chunk = vec![0u8; 4];
        chunk[0] = ChunkType::CookieAck as u8;
        chunk[2..4].copy_from_slice(&0u16.to_be_bytes());
        chunk
    }

    pub fn handle_init(&mut self, data: &[u8]) -> Result<Vec<u8>, SctpError> {
        if data.len() < 16 {
            return Err(SctpError::ProtocolViolation);
        }
        
        self.remote_tag = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        self.initial_remote_tsn = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        
        self.state = SctpState::CookieEchoed;
        
        Ok(self.create_init_ack_chunk())
    }

    pub fn handle_init_ack(&mut self, data: &[u8]) -> Result<(), SctpError> {
        if data.len() < 16 {
            return Err(SctpError::ProtocolViolation);
        }
        
        self.remote_tag = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        
        self.state = SctpState::Established;
        
        Ok(())
    }

    pub fn handle_cookie_echo(&mut self, data: &[u8]) -> Result<Vec<u8>, SctpError> {
        self.state = SctpState::Established;
        Ok(self.create_cookie_ack_chunk())
    }

    pub fn handle_cookie_ack(&mut self) -> Result<(), SctpError> {
        self.state = SctpState::Established;
        Ok(())
    }

    pub fn create_data_chunk(&mut self, stream_id: u16, data: Vec<u8>, ppid: u32, unordered: bool) -> Vec<u8> {
        let tsn = self.next_tsn;
        self.next_tsn = self.next_tsn.wrapping_add(1);
        
        let flags = if unordered { 0x02 } else { 0x00 };
        
        let mut chunk = vec![0u8; 16 + data.len()];
        chunk[0] = ChunkType::Data as u8;
        
        let payload_len = 16 + data.len();
        chunk[2..4].copy_from_slice(&(payload_len as u16).to_be_bytes());
        
        chunk[4] = flags;
        
        chunk[5..7].copy_from_slice(&stream_id.to_be_bytes());
        
        chunk[8..12].copy_from_slice(&tsn.to_be_bytes());
        
        chunk[12..16].copy_from_slice(&ppid.to_be_bytes());
        
        chunk[16..].copy_from_slice(&data);
        
        chunk
    }

    pub fn handle_data_chunk(&mut self, data: &[u8]) -> Result<Option<SctpMessage>, SctpError> {
        if data.len() < 16 {
            return Err(SctpError::ProtocolViolation);
        }
        
        let tsn = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        
        if tsn > self.last_received_tsn {
            self.last_received_tsn = tsn;
        }
        
        let stream_id = u16::from_be_bytes([data[5], data[6]]);
        let ppid = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        
        let flags = data[4];
        let unordered = (flags & 0x02) != 0;
        
        let payload = data[16..].to_vec();
        
        let msg = SctpMessage {
            data: payload,
            ppid,
            unordered,
            complete: true,
        };
        
        self.inbound_queue.push_back(msg.clone());
        
        Ok(Some(msg))
    }

    pub fn create_sack_chunk(&self) -> Vec<u8> {
        let mut chunk = vec![0u8; 16];
        chunk[0] = ChunkType::Sack as u8;
        
        chunk[4..8].copy_from_slice(&self.last_received_tsn.to_be_bytes());
        
        chunk[8..12].copy_from_slice(&0u32.to_be_bytes());
        
        chunk[12..16].copy_from_slice(&0u32.to_be_bytes());
        
        let total_len = chunk.len() - 4;
        chunk[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
        
        chunk
    }

    pub fn handle_sack_chunk(&self, _data: &[u8]) -> Result<(), SctpError> {
        Ok(())
    }

    pub fn open_stream(&self, stream_id: u16, unordered: bool) -> Result<SctpStream, SctpError> {
        let streams = self.streams.lock();
        if stream_id as usize >= streams.len() {
            return Err(SctpError::StreamNotFound);
        }
        Ok(streams[stream_id as usize].clone())
    }

    pub fn add_stream(&self, stream_id: u16, unordered: bool) {
        let mut streams = self.streams.lock();
        while streams.len() <= stream_id as usize {
            streams.push(SctpStream::new(streams.len() as u16, false));
        }
        streams[stream_id as usize] = SctpStream::new(stream_id, unordered);
    }

    pub fn poll_inbound(&self) -> Option<SctpMessage> {
        self.inbound_queue.pop_front()
    }

    pub fn is_connected(&self) -> bool {
        self.state == SctpState::Established
    }

    pub fn state(&self) -> SctpState {
        self.state
    }
}

pub struct SctpTransport {
    association: Arc<Mutex<Option<SctpAssociation>>>,
    max_message_size: usize,
}

impl SctpTransport {
    pub fn new() -> Self {
        Self {
            association: Arc::new(Mutex::new(None)),
            max_message_size: 65536,
        }
    }

    pub fn connect(&self, local_port: u16, remote_port: u16) -> Arc<SctpAssociation> {
        let assoc = Arc::new(SctpAssociation::new_client(local_port, remote_port));
        *self.association.lock() = Some(assoc.clone());
        assoc
    }

    pub fn accept(&self, local_port: u16) -> Arc<SctpAssociation> {
        let assoc = Arc::new(SctpAssociation::new_server(local_port));
        *self.association.lock() = Some(assoc.clone());
        assoc
    }

    pub fn association(&self) -> Option<Arc<SctpAssociation>> {
        self.association.lock().clone()
    }

    pub fn handle_packet(&self, data: &[u8]) -> Result<Option<SctpMessage>, SctpError> {
        let mut assoc_guard = self.association.lock();
        let assoc = assoc_guard.as_mut().ok_or(SctpError::AssociationNotFound)?;
        
        if data.is_empty() {
            return Ok(None);
        }
        
        let chunk_type = data[0];
        
        match chunk_type {
            1 => {
                let response = assoc.handle_init(&data[4..])?;
                Ok(None)
            }
            2 => {
                assoc.handle_init_ack(&data[4..])?;
                Ok(None)
            }
            10 => {
                let response = assoc.handle_cookie_echo(&data[4..])?;
                Ok(None)
            }
            11 => {
                assoc.handle_cookie_ack()?;
                Ok(None)
            }
            0 => {
                assoc.handle_data_chunk(&data[4..])
            }
            3 => {
                assoc.handle_sack_chunk(&data[4..])?;
                Ok(None)
            }
            _ => Err(SctpError::ProtocolViolation),
        }
    }

    pub fn reset_stream(&self, stream_id: u16) {
        if let Some(assoc) = self.association.lock().as_mut() {
            let mut streams = assoc.streams.lock();
            if (stream_id as usize) < streams.len() {
                streams[stream_id as usize].state = StreamState::Closed;
            }
        }
    }
}

impl Default for SctpTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sctp_association_creation() {
        let assoc = SctpAssociation::new_client(5000, 5000);
        assert_eq!(assoc.state, SctpState::CookieWait);
    }

    #[test]
    fn sctp_stream_send() {
        let mut stream = SctpStream::new(0, false);
        stream.send(b"hello".to_vec()).unwrap();
        assert_eq!(stream.buffered_amount(), 5);
    }

    #[test]
    fn sctp_data_chunk_format() {
        let assoc = SctpAssociation::new_client(5000, 5000);
        let chunk = assoc.create_data_chunk(0, b"test".to_vec(), 53, false);
        assert_eq!(chunk[0], 0);
        assert!(chunk.len() >= 20);
    }

    #[test]
    fn sctp_transport() {
        let transport = SctpTransport::new();
        let assoc = transport.connect(5000, 5000);
        assert!(assoc.is_connected() || assoc.state() == SctpState::CookieWait);
    }
}

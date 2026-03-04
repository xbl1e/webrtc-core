use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::Mutex;
use thiserror::Error;
use bytes::Bytes;

#[derive(Error, Debug)]
pub enum DataChannelError {
    #[error("channel not open")]
    NotOpen,
    #[error("buffer full")]
    BufferFull,
    #[error("invalid state")]
    InvalidState,
    #[error("send error")]
    SendError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataChannelState {
    Connecting,
    Open,
    Closing,
    Closed,
}

impl Default for DataChannelState {
    fn default() -> Self {
        DataChannelState::Connecting
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataChannelType {
    Control = 0x00,
    Data = 0x01,
}

#[derive(Clone, Debug)]
pub enum DataChannelMessage {
    Text(Bytes),
    Binary(Bytes),
}

#[derive(Clone, Debug)]
pub struct DataChannelConfig {
    pub ordered: bool,
    pub max_packet_lifetime_ms: Option<u32>,
    pub max_retransmits: Option<u16>,
    pub label: String,
    pub protocol: String,
    pub negotiated: bool,
}

impl Default for DataChannelConfig {
    fn default() -> Self {
        Self {
            ordered: true,
            max_packet_lifetime_ms: None,
            max_retransmits: None,
            label: String::new(),
            protocol: String::new(),
            negotiated: false,
        }
    }
}

pub struct DataChannel {
    pub id: u16,
    pub config: DataChannelConfig,
    pub state: Mutex<DataChannelState>,
    pub buffered_amount: AtomicUsize,
    pub buffered_amount_low_threshold: AtomicUsize,
    pub messages: Mutex<VecDeque<DataChannelMessage>>,
}

impl DataChannel {
    pub fn new(id: u16, config: DataChannelConfig) -> Self {
        Self {
            id,
            config,
            state: Mutex::new(DataChannelState::Connecting),
            buffered_amount: AtomicUsize::new(0),
            buffered_amount_low_threshold: AtomicUsize::new(65536),
            messages: Mutex::new(VecDeque::new()),
        }
    }

    pub fn send_text(&self, text: &str) -> Result<(), DataChannelError> {
        self.send(DataChannelMessage::Text(Bytes::from(text)))
    }

    pub fn send_binary(&self, data: &[u8]) -> Result<(), DataChannelError> {
        self.send(DataChannelMessage::Binary(Bytes::copy_from_slice(data)))
    }

    fn send(&self, msg: DataChannelMessage) -> Result<(), DataChannelError> {
        let state = *self.state.lock();
        if state != DataChannelState::Open {
            return Err(DataChannelError::NotOpen);
        }
        
        let size = match &msg {
            DataChannelMessage::Text(t) => t.len(),
            DataChannelMessage::Binary(b) => b.len(),
        };
        
        let current = self.buffered_amount.load(Ordering::Relaxed);
        if current + size > 16 * 1024 * 1024 {
            return Err(DataChannelError::BufferFull);
        }
        
        self.buffered_amount.fetch_add(size, Ordering::Relaxed);
        self.messages.lock().push_back(msg);
        
        Ok(())
    }

    pub fn pop_message(&self) -> Option<DataChannelMessage> {
        let msg = self.messages.lock().pop_front()?;
        let size = match &msg {
            DataChannelMessage::Text(t) => t.len(),
            DataChannelMessage::Binary(b) => b.len(),
        };
        self.buffered_amount.fetch_sub(size, Ordering::Relaxed);
        Some(msg)
    }

    pub fn buffered_amount(&self) -> usize {
        self.buffered_amount.load(Ordering::Relaxed)
    }

    pub fn set_buffered_amount_low_threshold(&self, threshold: usize) {
        self.buffered_amount_low_threshold.store(threshold, Ordering::Relaxed);
    }

    pub fn buffered_amount_low_threshold(&self) -> usize {
        self.buffered_amount_low_threshold.load(Ordering::Relaxed)
    }

    pub fn is_open(&self) -> bool {
        *self.state.lock() == DataChannelState::Open
    }

    pub fn is_closed(&self) -> bool {
        *self.state.lock() == DataChannelState::Closed
    }

    pub fn state(&self) -> DataChannelState {
        *self.state.lock()
    }

    pub fn open(&self) {
        *self.state.lock() = DataChannelState::Open;
    }

    pub fn close(&self) {
        *self.state.lock() = DataChannelState::Closed;
        self.messages.lock().clear();
    }
}

pub struct DataChannelManager {
    channels: Mutex<Vec<Arc<DataChannel>>>,
    next_channel_id: Mutex<u16>,
}

impl DataChannelManager {
    pub fn new() -> Self {
        Self {
            channels: Mutex::new(Vec::new()),
            next_channel_id: Mutex::new(0),
        }
    }

    pub fn create_data_channel(&self, label: &str, config: DataChannelConfig) -> Arc<DataChannel> {
        let mut next_id = self.next_channel_id.lock();
        let id = *next_id;
        *next_id = next_id.wrapping_add(1);
        
        let mut cfg = config;
        if cfg.label.is_empty() {
            cfg.label = label.to_string();
        }
        
        let channel = Arc::new(DataChannel::new(id, cfg));
        self.channels.lock().push(channel.clone());
        
        channel
    }

    pub fn get_channel(&self, id: u16) -> Option<Arc<DataChannel>> {
        self.channels.lock().iter()
            .find(|c| c.id == id)
            .cloned()
    }

    pub fn remove_channel(&self, id: u16) {
        self.channels.lock().retain(|c| c.id != id);
    }

    pub fn channels(&self) -> Vec<Arc<DataChannel>> {
        self.channels.lock().clone()
    }

    pub fn open_channels(&self) -> Vec<Arc<DataChannel>> {
        self.channels.lock().iter()
            .filter(|c| c.is_open())
            .cloned()
            .collect()
    }
}

impl Default for DataChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DataChannelProtocol {
    manager: Arc<DataChannelManager>,
    sctp_stream_id: u16,
}

impl DataChannelProtocol {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(DataChannelManager::new()),
            sctp_stream_id: 0,
        }
    }

    pub fn create_channel(&self, label: &str, config: DataChannelConfig) -> Arc<DataChannel> {
        self.manager.create_data_channel(label, config)
    }

    pub fn handle_open_message(&self, data: &[u8]) -> Result<Arc<DataChannel>, DataChannelError> {
        if data.len() < 4 {
            return Err(DataChannelError::InvalidState);
        }
        
        let channel_type = data[0];
        let flags = data[1];
        let label_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        
        if data.len() < 4 + label_len + 2 {
            return Err(DataChannelError::InvalidState);
        }
        
        let label = String::from_utf8_lossy(&data[4..4+label_len]).to_string();
        
        let proto_len_offset = 4 + label_len;
        let proto_len = u16::from_be_bytes([data[proto_len_offset], data[proto_len_offset+1]]) as usize;
        
        let protocol = if proto_len > 0 {
            let proto_start = proto_len_offset + 2;
            String::from_utf8_lossy(&data[proto_start..proto_start+proto_len]).to_string()
        } else {
            String::new()
        };
        
        let ordered = (flags & 0x02) == 0;
        
        let config = DataChannelConfig {
            label,
            protocol,
            ordered,
            max_packet_lifetime_ms: None,
            max_retransmits: None,
            negotiated: false,
        };
        
        let channel = self.manager.create_data_channel(&config.label, config);
        channel.open();
        
        Ok(channel)
    }

    pub fn encode_open_message(&self, label: &str, protocol: &str, ordered: bool) -> Vec<u8> {
        let label_bytes = label.as_bytes();
        let proto_bytes = protocol.as_bytes();
        
        let mut msg = vec![0u8; 4 + label_bytes.len() + 2 + proto_bytes.len()];
        
        msg[0] = DataChannelType::Control as u8;
        
        let mut flags = 0u8;
        if !ordered {
            flags |= 0x02;
        }
        msg[1] = flags;
        
        msg[2..4].copy_from_slice(&(label_bytes.len() as u16).to_be_bytes());
        msg[4..4+label_bytes.len()].copy_from_slice(label_bytes);
        
        let proto_offset = 4 + label_bytes.len();
        msg[proto_offset..proto_offset+2].copy_from_slice(&(proto_bytes.len() as u16).to_be_bytes());
        msg[proto_offset+2..proto_offset+2+proto_bytes.len()].copy_from_slice(proto_bytes);
        
        msg
    }

    pub fn handle_data_message(&self, data: &[u8]) -> Result<DataChannelMessage, DataChannelError> {
        if data.is_empty() {
            return Err(DataChannelError::InvalidState);
        }
        
        let msg_type = data[0];
        
        match msg_type {
            0x00 | 0x02 => {
                Ok(DataChannelMessage::Text(Bytes::from(data[1..].to_vec())))
            }
            0x01 | 0x03 => {
                Ok(DataChannelMessage::Binary(Bytes::from(data[1..].to_vec())))
            }
            _ => Err(DataChannelError::InvalidState),
        }
    }

    pub fn encode_data_message(&self, msg: &DataChannelMessage) -> Vec<u8> {
        match msg {
            DataChannelMessage::Text(t) => {
                let mut out = vec![0u8; 1 + t.len()];
                out[0] = 0x00;
                out[1..].copy_from_slice(t);
                out
            }
            DataChannelMessage::Binary(b) => {
                let mut out = vec![0u8; 1 + b.len()];
                out[0] = 0x01;
                out[1..].copy_from_slice(b);
                out
            }
        }
    }

    pub fn manager(&self) -> &Arc<DataChannelManager> {
        &self.manager
    }
}

impl Default for DataChannelProtocol {
    fn default() -> Self {
        Self::new()
    }
}

use std::collections::VecDeque;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_channel_creation() {
        let config = DataChannelConfig::default();
        let channel = DataChannel::new(0, config);
        assert_eq!(channel.state(), DataChannelState::Connecting);
    }

    #[test]
    fn data_channel_send_text() {
        let config = DataChannelConfig::default();
        let channel = DataChannel::new(0, config);
        channel.open();
        channel.send_text("hello").unwrap();
        assert!(channel.is_open());
    }

    #[test]
    fn data_channel_send_binary() {
        let config = DataChannelConfig::default();
        let channel = DataChannel::new(0, config);
        channel.open();
        channel.send_binary(b"binary data").unwrap();
    }

    #[test]
    fn data_channel_manager() {
        let manager = DataChannelManager::new();
        let channel = manager.create_data_channel("test", DataChannelConfig::default());
        assert_eq!(channel.config.label, "test");
    }

    #[test]
    fn open_message_encoding() {
        let protocol = DataChannelProtocol::new();
        let msg = protocol.encode_open_message("test-channel", "json", true);
        assert!(msg.len() > 0);
    }
}

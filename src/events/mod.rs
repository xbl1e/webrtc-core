//! Event system for W3C-compatible WebRTC callbacks.
//!
//! Provides typed event emission and subscription for:
//! - ICE candidate events
//! - Track events
//! - Data channel events
//! - Connection state changes

use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt::Debug;
type EventHandler<T> = Box<dyn Fn(T) + Send + Sync + 'static>;
#[derive(Clone)]
pub struct EventEmitter<T: Clone + Debug + Send + Sync> {
    handlers: Arc<RwLock<Vec<EventHandler<T>>>>,
}

impl<T: Clone + Debug + Send + Sync> EventEmitter<T> {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }
    pub fn on(&self, handler: impl Fn(T) + Send + Sync + 'static) {
        self.handlers.write().push(Box::new(handler));
    }
    pub fn emit(&self, event: T) {
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            handler(event.clone());
        }
    }
    pub fn clear(&self) {
        self.handlers.write().clear();
    }
    pub fn handler_count(&self) -> usize {
        self.handlers.read().len()
    }
}

impl<T: Clone + Debug + Send + Sync> Default for EventEmitter<T> {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Debug)]
pub struct IceCandidateEvent {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IceConnectionState {
    New,
    Checking,
    Connected,
    Completed,
    Failed,
    Disconnected,
    Closed,
}

impl std::fmt::Display for IceConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IceConnectionState::New => write!(f, "new"),
            IceConnectionState::Checking => write!(f, "checking"),
            IceConnectionState::Connected => write!(f, "connected"),
            IceConnectionState::Completed => write!(f, "completed"),
            IceConnectionState::Failed => write!(f, "failed"),
            IceConnectionState::Disconnected => write!(f, "disconnected"),
            IceConnectionState::Closed => write!(f, "closed"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

impl std::fmt::Display for PeerConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerConnectionState::New => write!(f, "new"),
            PeerConnectionState::Connecting => write!(f, "connecting"),
            PeerConnectionState::Connected => write!(f, "connected"),
            PeerConnectionState::Disconnected => write!(f, "disconnected"),
            PeerConnectionState::Failed => write!(f, "failed"),
            PeerConnectionState::Closed => write!(f, "closed"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalingState {
    Stable,
    HaveLocalOffer,
    HaveRemoteOffer,
    HaveLocalPranswer,
    HaveRemotePranswer,
    Closed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
}
#[derive(Clone, Debug)]
pub struct TrackEvent {
    pub track_id: String,
    pub stream_id: String,
    pub kind: MediaKind,
    pub ssrc: u32,
}
#[derive(Clone, Debug)]
pub struct DataChannelEvent {
    pub label: String,
    pub protocol: String,
    pub channel_type: DataChannelType,
    pub ordered: bool,
    pub max_retransmits: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataChannelType {
    DataChannelReliable,
    DataChannelReliableUnordered,
    DataChannelPartialReliable,
    DataChannelPartialReliableUnordered,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IceGatheringState {
    New,
    Gathering,
    Complete,
}
#[derive(Clone)]
pub struct PeerConnectionEvents {
    pub on_ice_candidate: EventEmitter<IceCandidateEvent>,
    pub on_ice_candidate_error: EventEmitter<IceCandidateError>,
    pub on_ice_connection_state_change: EventEmitter<IceConnectionState>,
    pub on_ice_gathering_state_change: EventEmitter<IceGatheringState>,
    pub on_signaling_state_change: EventEmitter<SignalingState>,
    pub on_track: EventEmitter<TrackEvent>,
    pub on_data_channel: EventEmitter<DataChannelEvent>,
    pub on_connection_state_change: EventEmitter<PeerConnectionState>,
}

#[derive(Clone, Debug)]
pub struct IceCandidateError {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub error_code: i32,
    pub error_text: String,
}

impl PeerConnectionEvents {
    pub fn new() -> Self {
        Self {
            on_ice_candidate: EventEmitter::new(),
            on_ice_candidate_error: EventEmitter::new(),
            on_ice_connection_state_change: EventEmitter::new(),
            on_ice_gathering_state_change: EventEmitter::new(),
            on_signaling_state_change: EventEmitter::new(),
            on_track: EventEmitter::new(),
            on_data_channel: EventEmitter::new(),
            on_connection_state_change: EventEmitter::new(),
        }
    }
}

impl Default for PeerConnectionEvents {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone)]
pub struct EventBus {
    emitters: Arc<RwLock<HashMap<String, Box<dyn erased_serde::Serialize + Send + Sync>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            emitters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[allow(dead_code)]
    pub fn subscribe<T: erased_serde::Serialize + Send + Sync + 'static>(&self, name: &str) {
        let _ = name;
    }

    #[allow(dead_code)]
    pub fn publish<T: erased_serde::Serialize + Send + Sync + 'static>(&self, name: &str, event: T) {
        let _ = (name, event);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

mod erased_serde {
    use std::fmt::Debug;

    pub trait Serialize: Send + Sync {
        fn erased_serialize(&self) -> Box<dyn erased_serde::Serializer>;
    }

    pub trait Serializer: Send + Sync {
        fn serialize<T: ?Sized + Serialize>(&self, _value: &T) {
            // Stub
        }
    }

    impl<T: Send + Sync + 'static> Serialize for T
    where
        T: Debug + serde::Serialize,
    {
        fn erased_serialize(&self) -> Box<dyn Serializer> {
            Box::new(SerdeWrapper(self))
        }
    }

    struct SerdeWrapper<T: ?Sized>(*const T);

    impl<T: ?Sized + Debug + serde::Serialize> Serializer for SerdeWrapper<T> {
        fn serialize<U: ?Sized + Serialize>(&self, _value: &U) {
            // Stub implementation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_emitter_basic() {
        let emitter: EventEmitter<String> = EventEmitter::new();
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();

        emitter.on(move |s| {
            received_clone.lock().unwrap().push(s);
        });

        emitter.emit("hello".to_string());
        emitter.emit("world".to_string());

        let msgs = received.lock().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], "hello");
        assert_eq!(msgs[1], "world");
    }

    #[test]
    fn peer_connection_events_default() {
        let events = PeerConnectionEvents::new();
        assert_eq!(events.on_ice_candidate.handler_count(), 0);
    }
}

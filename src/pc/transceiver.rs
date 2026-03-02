use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
    Data,
}

impl MediaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
            MediaKind::Data => "application",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransceiverDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
    Stopped,
}

impl TransceiverDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransceiverDirection::SendRecv => "sendrecv",
            TransceiverDirection::SendOnly => "sendonly",
            TransceiverDirection::RecvOnly => "recvonly",
            TransceiverDirection::Inactive => "inactive",
            TransceiverDirection::Stopped => "stopped",
        }
    }

    pub fn can_send(&self) -> bool {
        matches!(self, TransceiverDirection::SendRecv | TransceiverDirection::SendOnly)
    }

    pub fn can_receive(&self) -> bool {
        matches!(self, TransceiverDirection::SendRecv | TransceiverDirection::RecvOnly)
    }
}

impl Default for TransceiverDirection {
    fn default() -> Self {
        TransceiverDirection::SendRecv
    }
}

#[derive(Clone, Debug)]
pub struct CodecParameters {
    pub mime_type: String,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub channels: Option<u8>,
    pub sdp_fmtp_line: Option<String>,
}

impl CodecParameters {
    pub fn opus() -> Self {
        Self {
            mime_type: "audio/opus".to_string(),
            payload_type: 111,
            clock_rate: 48000,
            channels: Some(2),
            sdp_fmtp_line: Some("minptime=10;useinbandfec=1".to_string()),
        }
    }

    pub fn vp8() -> Self {
        Self {
            mime_type: "video/VP8".to_string(),
            payload_type: 96,
            clock_rate: 90000,
            channels: None,
            sdp_fmtp_line: None,
        }
    }

    pub fn vp9() -> Self {
        Self {
            mime_type: "video/VP9".to_string(),
            payload_type: 98,
            clock_rate: 90000,
            channels: None,
            sdp_fmtp_line: Some("profile-id=0".to_string()),
        }
    }

    pub fn h264() -> Self {
        Self {
            mime_type: "video/H264".to_string(),
            payload_type: 102,
            clock_rate: 90000,
            channels: None,
            sdp_fmtp_line: Some("level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f".to_string()),
        }
    }

    pub fn av1() -> Self {
        Self {
            mime_type: "video/AV1".to_string(),
            payload_type: 45,
            clock_rate: 90000,
            channels: None,
            sdp_fmtp_line: None,
        }
    }
}

pub struct RtpSender {
    pub ssrc: u32,
    pub rtx_ssrc: u32,
    _active: AtomicBool,
    packets_sent: AtomicU32,
    bytes_sent: std::sync::atomic::AtomicU64,
    _codec: Mutex<Option<CodecParameters>>,
}

impl RtpSender {
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            rtx_ssrc: ssrc + 1,
            _active: AtomicBool::new(true),
            packets_sent: AtomicU32::new(0),
            bytes_sent: std::sync::atomic::AtomicU64::new(0),
            _codec: Mutex::new(None),
        }
    }

    pub fn set_codec(&self, codec: CodecParameters) {
        *self._codec.lock().unwrap() = Some(codec);
    }

    pub fn record_sent(&self, bytes: usize) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn packets_sent(&self) -> u32 {
        self.packets_sent.load(Ordering::Relaxed)
    }

    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    pub fn is_active(&self) -> bool {
        self._active.load(Ordering::Relaxed)
    }

    pub fn set_active(&self, active: bool) {
        self._active.store(active, Ordering::Relaxed);
    }
}

pub struct RtpReceiver {
    pub ssrc: u32,
    _active: AtomicBool,
    packets_received: AtomicU32,
    bytes_received: std::sync::atomic::AtomicU64,
    _codec: Mutex<Option<CodecParameters>>,
}

impl RtpReceiver {
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            _active: AtomicBool::new(true),
            packets_received: AtomicU32::new(0),
            bytes_received: std::sync::atomic::AtomicU64::new(0),
            _codec: Mutex::new(None),
        }
    }

    pub fn record_received(&self, bytes: usize) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn packets_received(&self) -> u32 {
        self.packets_received.load(Ordering::Relaxed)
    }

    pub fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }
}

pub struct RtpTransceiver {
    pub mid: String,
    pub kind: MediaKind,
    pub sender: Arc<RtpSender>,
    pub receiver: Arc<RtpReceiver>,
    direction: Mutex<TransceiverDirection>,
    stopped: AtomicBool,
    codecs: Mutex<Vec<CodecParameters>>,
}

impl RtpTransceiver {
    pub fn new(mid: &str, kind: MediaKind, tx_ssrc: u32, rx_ssrc: u32) -> Self {
        Self {
            mid: mid.to_string(),
            kind,
            sender: Arc::new(RtpSender::new(tx_ssrc)),
            receiver: Arc::new(RtpReceiver::new(rx_ssrc)),
            direction: Mutex::new(TransceiverDirection::SendRecv),
            stopped: AtomicBool::new(false),
            codecs: Mutex::new(Vec::new()),
        }
    }

    pub fn direction(&self) -> TransceiverDirection {
        *self.direction.lock().unwrap()
    }

    pub fn set_direction(&self, dir: TransceiverDirection) {
        *self.direction.lock().unwrap() = dir;
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        *self.direction.lock().unwrap() = TransceiverDirection::Stopped;
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    pub fn set_codecs(&self, codecs: Vec<CodecParameters>) {
        *self.codecs.lock().unwrap() = codecs;
    }

    pub fn negotiated_codec(&self) -> Option<CodecParameters> {
        self.codecs.lock().unwrap().first().cloned()
    }

    pub fn can_send(&self) -> bool {
        self.direction().can_send() && !self.is_stopped()
    }

    pub fn can_receive(&self) -> bool {
        self.direction().can_receive() && !self.is_stopped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transceiver_direction() {
        let tc = RtpTransceiver::new("0", MediaKind::Audio, 100, 200);
        assert!(tc.can_send());
        assert!(tc.can_receive());
        tc.set_direction(TransceiverDirection::RecvOnly);
        assert!(!tc.can_send());
        assert!(tc.can_receive());
        tc.stop();
        assert!(!tc.can_send());
        assert!(!tc.can_receive());
    }

    #[test]
    fn rtp_sender_accounting() {
        let sender = RtpSender::new(0x1234);
        sender.record_sent(1200);
        sender.record_sent(800);
        assert_eq!(sender.packets_sent(), 2);
        assert_eq!(sender.bytes_sent(), 2000);
    }
}

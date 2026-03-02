use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use super::configuration::RtcConfiguration;
use super::transceiver::{RtpTransceiver, MediaKind, TransceiverDirection, CodecParameters};
use super::stats::RtcStatsReport;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

impl Default for PeerConnectionState {
    fn default() -> Self {
        PeerConnectionState::New
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalingState {
    Stable,
    HaveLocalOffer,
    HaveRemoteOffer,
    HaveLocalPranswer,
    HaveRemotePranswer,
    Closed,
}

impl Default for SignalingState {
    fn default() -> Self {
        SignalingState::Stable
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SdpType {
    Offer,
    PrAnswer,
    Answer,
    Rollback,
}

#[derive(Clone, Debug)]
pub struct SessionDescription {
    pub sdp_type: SdpType,
    pub sdp: String,
}

impl SessionDescription {
    pub fn offer(sdp: &str) -> Self {
        Self { sdp_type: SdpType::Offer, sdp: sdp.to_string() }
    }

    pub fn answer(sdp: &str) -> Self {
        Self { sdp_type: SdpType::Answer, sdp: sdp.to_string() }
    }

    pub fn is_offer(&self) -> bool {
        self.sdp_type == SdpType::Offer
    }

    pub fn is_answer(&self) -> bool {
        self.sdp_type == SdpType::Answer
    }
}

pub struct PeerConnection {
    cfg: RtcConfiguration,
    state: Mutex<PeerConnectionState>,
    signaling_state: Mutex<SignalingState>,
    transceivers: Mutex<Vec<Arc<RtpTransceiver>>>,
    local_description: Mutex<Option<SessionDescription>>,
    remote_description: Mutex<Option<SessionDescription>>,
    stats: Arc<RtcStatsReport>,
    closed: AtomicBool,
    next_ssrc: Mutex<u32>,
    next_mid: Mutex<u32>,
}

impl PeerConnection {
    pub fn new(cfg: RtcConfiguration) -> Self {
        Self {
            cfg,
            state: Mutex::new(PeerConnectionState::New),
            signaling_state: Mutex::new(SignalingState::Stable),
            transceivers: Mutex::new(Vec::new()),
            local_description: Mutex::new(None),
            remote_description: Mutex::new(None),
            stats: Arc::new(RtcStatsReport::new()),
            closed: AtomicBool::new(false),
            next_ssrc: Mutex::new(0x10000000),
            next_mid: Mutex::new(0),
        }
    }

    fn alloc_ssrc(&self) -> u32 {
        let mut s = self.next_ssrc.lock().unwrap();
        let v = *s;
        *s = s.wrapping_add(2);
        v
    }

    fn alloc_mid(&self) -> String {
        let mut m = self.next_mid.lock().unwrap();
        let v = *m;
        *m += 1;
        v.to_string()
    }

    pub fn add_transceiver(&self, kind: MediaKind) -> Arc<RtpTransceiver> {
        let tx_ssrc = self.alloc_ssrc();
        let rx_ssrc = self.alloc_ssrc();
        let mid = self.alloc_mid();
        let tc = Arc::new(RtpTransceiver::new(&mid, kind, tx_ssrc, rx_ssrc));
        let default_codecs = match kind {
            MediaKind::Audio => vec![CodecParameters::opus()],
            MediaKind::Video => vec![CodecParameters::vp8(), CodecParameters::vp9(), CodecParameters::h264()],
            MediaKind::Data => vec![],
        };
        tc.set_codecs(default_codecs);
        self.transceivers.lock().unwrap().push(tc.clone());
        tc
    }

    pub fn add_transceiver_with_direction(
        &self,
        kind: MediaKind,
        direction: TransceiverDirection,
    ) -> Arc<RtpTransceiver> {
        let tc = self.add_transceiver(kind);
        tc.set_direction(direction);
        tc
    }

    pub fn create_offer(&self) -> Result<SessionDescription, String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("PeerConnection is closed".to_string());
        }
        let transceivers = self.transceivers.lock().unwrap();
        let mut sdp = String::new();
        sdp.push_str("v=0\r\n");
        sdp.push_str("o=- 0 0 IN IP4 0.0.0.0\r\n");
        sdp.push_str("s=-\r\n");
        sdp.push_str("t=0 0\r\n");
        sdp.push_str("a=msid-semantic: WMS\r\n");

        for tc in transceivers.iter() {
            let codec = tc.negotiated_codec();
            let kind_str = tc.kind.as_str();
            let dir_str = tc.direction().as_str();
            let pt = codec.as_ref().map(|c| c.payload_type).unwrap_or(96);
            let cr = codec.as_ref().map(|c| c.clock_rate).unwrap_or(90000);
            let mime = codec.as_ref().map(|c| c.mime_type.split('/').last().unwrap_or("").to_string()).unwrap_or_default();
            sdp.push_str(&format!("m={} 9 UDP/TLS/RTP/SAVPF {}\r\n", kind_str, pt));
            sdp.push_str("c=IN IP4 0.0.0.0\r\n");
            sdp.push_str("a=rtcp:9 IN IP4 0.0.0.0\r\n");
            sdp.push_str(&format!("a=mid:{}\r\n", tc.mid));
            sdp.push_str(&format!("a={}\r\n", dir_str));
            sdp.push_str(&format!("a=rtpmap:{} {}/{}\r\n", pt, mime, cr));
            if let Some(ref c) = codec {
                if let Some(ref fmtp) = c.sdp_fmtp_line {
                    sdp.push_str(&format!("a=fmtp:{} {}\r\n", pt, fmtp));
                }
                if let Some(ch) = c.channels {
                    if ch > 1 {
                        sdp.push_str(&format!("a=rtpmap:{} {}/{}/{}\r\n", pt, mime, cr, ch));
                    }
                }
            }
            sdp.push_str(&format!("a=ssrc:{} cname:webrtc-core\r\n", tc.sender.ssrc));
        }

        let desc = SessionDescription::offer(&sdp);
        *self.local_description.lock().unwrap() = Some(desc.clone());
        *self.signaling_state.lock().unwrap() = SignalingState::HaveLocalOffer;
        Ok(desc)
    }

    pub fn create_answer(&self) -> Result<SessionDescription, String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("PeerConnection is closed".to_string());
        }
        let remote = self.remote_description.lock().unwrap().clone();
        if remote.is_none() {
            return Err("no remote description set".to_string());
        }
        let transceivers = self.transceivers.lock().unwrap();
        let mut sdp = String::new();
        sdp.push_str("v=0\r\n");
        sdp.push_str("o=- 0 0 IN IP4 0.0.0.0\r\n");
        sdp.push_str("s=-\r\n");
        sdp.push_str("t=0 0\r\n");

        for tc in transceivers.iter() {
            let codec = tc.negotiated_codec();
            let kind_str = tc.kind.as_str();
            let dir_str = match tc.direction() {
                crate::pc::transceiver::TransceiverDirection::SendRecv => "sendrecv",
                crate::pc::transceiver::TransceiverDirection::SendOnly => "recvonly",
                crate::pc::transceiver::TransceiverDirection::RecvOnly => "sendonly",
                crate::pc::transceiver::TransceiverDirection::Inactive => "inactive",
                crate::pc::transceiver::TransceiverDirection::Stopped => "inactive",
            };
            let pt = codec.as_ref().map(|c| c.payload_type).unwrap_or(96);
            sdp.push_str(&format!("m={} 9 UDP/TLS/RTP/SAVPF {}\r\n", kind_str, pt));
            sdp.push_str("c=IN IP4 0.0.0.0\r\n");
            sdp.push_str(&format!("a=mid:{}\r\n", tc.mid));
            sdp.push_str(&format!("a={}\r\n", dir_str));
        }

        let desc = SessionDescription::answer(&sdp);
        *self.local_description.lock().unwrap() = Some(desc.clone());
        *self.signaling_state.lock().unwrap() = SignalingState::Stable;
        Ok(desc)
    }

    pub fn set_local_description(&self, desc: SessionDescription) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("closed".to_string());
        }
        let mut sig = self.signaling_state.lock().unwrap();
        match desc.sdp_type {
            SdpType::Offer => {
                if *sig != SignalingState::Stable && *sig != SignalingState::HaveRemoteOffer {
                    return Err("invalid state for local offer".to_string());
                }
                *self.local_description.lock().unwrap() = Some(desc);
                *sig = SignalingState::HaveLocalOffer;
            }
            SdpType::Answer | SdpType::PrAnswer => {
                if *sig != SignalingState::HaveRemoteOffer {
                    return Err("invalid state for local answer".to_string());
                }
                *self.local_description.lock().unwrap() = Some(desc);
                *sig = SignalingState::Stable;
                *self.state.lock().unwrap() = PeerConnectionState::Connected;
            }
            SdpType::Rollback => {
                *self.local_description.lock().unwrap() = None;
                *sig = SignalingState::Stable;
            }
        }
        Ok(())
    }

    pub fn set_remote_description(&self, desc: SessionDescription) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("closed".to_string());
        }
        let mut sig = self.signaling_state.lock().unwrap();
        match desc.sdp_type {
            SdpType::Offer => {
                if *sig != SignalingState::Stable && *sig != SignalingState::HaveLocalOffer {
                    return Err("invalid state for remote offer".to_string());
                }
                *self.remote_description.lock().unwrap() = Some(desc);
                *sig = SignalingState::HaveRemoteOffer;
                *self.state.lock().unwrap() = PeerConnectionState::Connecting;
            }
            SdpType::Answer | SdpType::PrAnswer => {
                if *sig != SignalingState::HaveLocalOffer {
                    return Err("invalid state for remote answer".to_string());
                }
                *self.remote_description.lock().unwrap() = Some(desc);
                *sig = SignalingState::Stable;
                *self.state.lock().unwrap() = PeerConnectionState::Connected;
            }
            SdpType::Rollback => {
                *self.remote_description.lock().unwrap() = None;
                *sig = SignalingState::Stable;
            }
        }
        Ok(())
    }

    pub fn state(&self) -> PeerConnectionState {
        *self.state.lock().unwrap()
    }

    pub fn signaling_state(&self) -> SignalingState {
        *self.signaling_state.lock().unwrap()
    }

    pub fn local_description(&self) -> Option<SessionDescription> {
        self.local_description.lock().unwrap().clone()
    }

    pub fn remote_description(&self) -> Option<SessionDescription> {
        self.remote_description.lock().unwrap().clone()
    }

    pub fn get_stats(&self) -> Arc<RtcStatsReport> {
        self.stats.clone()
    }

    pub fn transceivers(&self) -> Vec<Arc<RtpTransceiver>> {
        self.transceivers.lock().unwrap().clone()
    }

    pub fn transceiver_by_mid(&self, mid: &str) -> Option<Arc<RtpTransceiver>> {
        self.transceivers.lock().unwrap()
            .iter()
            .find(|t| t.mid == mid)
            .cloned()
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        *self.state.lock().unwrap() = PeerConnectionState::Closed;
        *self.signaling_state.lock().unwrap() = SignalingState::Closed;
        let transceivers = self.transceivers.lock().unwrap();
        for tc in transceivers.iter() {
            tc.stop();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn configuration(&self) -> &RtcConfiguration {
        &self.cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pc::configuration::RtcConfiguration;

    #[test]
    fn peer_connection_offer_answer() {
        let cfg = RtcConfiguration::new().with_stun("stun:stun.l.google.com:19302");
        let offerer = PeerConnection::new(cfg.clone());
        let answerer = PeerConnection::new(cfg);

        offerer.add_transceiver(MediaKind::Audio);
        offerer.add_transceiver(MediaKind::Video);
        let offer = offerer.create_offer().unwrap();
        assert!(offer.is_offer());
        assert_eq!(offerer.signaling_state(), SignalingState::HaveLocalOffer);

        answerer.set_remote_description(offer.clone()).unwrap();
        assert_eq!(answerer.signaling_state(), SignalingState::HaveRemoteOffer);

        answerer.add_transceiver(MediaKind::Audio);
        answerer.add_transceiver(MediaKind::Video);
        let answer = answerer.create_answer().unwrap();
        assert!(answer.is_answer());

        offerer.set_remote_description(answer).unwrap();
        assert_eq!(offerer.signaling_state(), SignalingState::Stable);
        assert_eq!(offerer.state(), PeerConnectionState::Connected);
    }

    #[test]
    fn peer_connection_close() {
        let pc = PeerConnection::new(RtcConfiguration::new());
        pc.add_transceiver(MediaKind::Audio);
        pc.close();
        assert!(pc.is_closed());
        assert_eq!(pc.state(), PeerConnectionState::Closed);
        assert!(pc.transceivers().iter().all(|t| t.is_stopped()));
    }

    #[test]
    fn peer_connection_transceiver_lookup() {
        let pc = PeerConnection::new(RtcConfiguration::new());
        let tc = pc.add_transceiver(MediaKind::Video);
        let mid = tc.mid.clone();
        let found = pc.transceiver_by_mid(&mid);
        assert!(found.is_some());
        assert_eq!(found.unwrap().mid, mid);
    }
}

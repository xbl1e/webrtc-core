use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct InboundRtpStats {
    pub ssrc: u32,
    pub packets_received: u64,
    pub bytes_received: u64,
    pub packets_lost: i64,
    pub jitter: f64,
    pub last_packet_received_timestamp: Option<f64>,
    pub codec_id: Option<String>,
    pub kind: String,
    pub nack_count: u32,
    pub fir_count: u32,
    pub pli_count: u32,
    pub frames_received: u64,
    pub frames_decoded: u64,
    pub key_frames_decoded: u32,
    pub total_decode_time_s: f64,
    pub qp_sum: Option<u64>,
}

impl Default for InboundRtpStats {
    fn default() -> Self {
        Self {
            ssrc: 0,
            packets_received: 0,
            bytes_received: 0,
            packets_lost: 0,
            jitter: 0.0,
            last_packet_received_timestamp: None,
            codec_id: None,
            kind: "audio".to_string(),
            nack_count: 0,
            fir_count: 0,
            pli_count: 0,
            frames_received: 0,
            frames_decoded: 0,
            key_frames_decoded: 0,
            total_decode_time_s: 0.0,
            qp_sum: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutboundRtpStats {
    pub ssrc: u32,
    pub packets_sent: u64,
    pub bytes_sent: u64,
    pub target_bitrate: f64,
    pub frames_encoded: u64,
    pub key_frames_encoded: u32,
    pub total_encode_time_s: f64,
    pub quality_limitation_reason: String,
    pub retransmitted_packets_sent: u64,
    pub retransmitted_bytes_sent: u64,
    pub qp_sum: Option<u64>,
}

impl Default for OutboundRtpStats {
    fn default() -> Self {
        Self {
            ssrc: 0,
            packets_sent: 0,
            bytes_sent: 0,
            target_bitrate: 0.0,
            frames_encoded: 0,
            key_frames_encoded: 0,
            total_encode_time_s: 0.0,
            quality_limitation_reason: "none".to_string(),
            retransmitted_packets_sent: 0,
            retransmitted_bytes_sent: 0,
            qp_sum: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct IceCandidatePairStats {
    pub local_candidate_id: String,
    pub remote_candidate_id: String,
    pub state: String,
    pub nominated: bool,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub total_round_trip_time: f64,
    pub current_round_trip_time: f64,
    pub available_outgoing_bitrate: f64,
    pub available_incoming_bitrate: f64,
    pub requests_sent: u64,
    pub responses_received: u64,
    pub requests_received: u64,
}

impl Default for IceCandidatePairStats {
    fn default() -> Self {
        Self {
            local_candidate_id: String::new(),
            remote_candidate_id: String::new(),
            state: "frozen".to_string(),
            nominated: false,
            bytes_sent: 0,
            bytes_received: 0,
            total_round_trip_time: 0.0,
            current_round_trip_time: 0.0,
            available_outgoing_bitrate: 0.0,
            available_incoming_bitrate: 0.0,
            requests_sent: 0,
            responses_received: 0,
            requests_received: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum RtcStats {
    InboundRtp(InboundRtpStats),
    OutboundRtp(OutboundRtpStats),
    CandidatePair(IceCandidatePairStats),
}

pub struct RtcStatsReport {
    stats: Mutex<HashMap<String, RtcStats>>,
}

impl RtcStatsReport {
    pub fn new() -> Self {
        Self { stats: Mutex::new(HashMap::new()) }
    }

    pub fn insert(&self, id: &str, stat: RtcStats) {
        self.stats.lock().unwrap().insert(id.to_string(), stat);
    }

    pub fn get(&self, id: &str) -> Option<RtcStats> {
        self.stats.lock().unwrap().get(id).cloned()
    }

    pub fn all(&self) -> Vec<(String, RtcStats)> {
        self.stats.lock().unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn inbound_rtp_stats(&self) -> Vec<InboundRtpStats> {
        self.stats.lock().unwrap().values()
            .filter_map(|s| if let RtcStats::InboundRtp(r) = s { Some(r.clone()) } else { None })
            .collect()
    }

    pub fn outbound_rtp_stats(&self) -> Vec<OutboundRtpStats> {
        self.stats.lock().unwrap().values()
            .filter_map(|s| if let RtcStats::OutboundRtp(r) = s { Some(r.clone()) } else { None })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.stats.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for RtcStatsReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_report_insert_get() {
        let report = RtcStatsReport::new();
        let mut inbound = InboundRtpStats::default();
        inbound.ssrc = 0x1234;
        inbound.packets_received = 1000;
        report.insert("inbound-audio-0x1234", RtcStats::InboundRtp(inbound));
        let got = report.get("inbound-audio-0x1234").unwrap();
        if let RtcStats::InboundRtp(r) = got {
            assert_eq!(r.ssrc, 0x1234);
            assert_eq!(r.packets_received, 1000);
        } else {
            panic!("wrong stat type");
        }
    }
}

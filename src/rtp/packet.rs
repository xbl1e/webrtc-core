use super::header::RtpHeader;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoRotation {
    Degree0 = 0,
    Degree90 = 1,
    Degree180 = 2,
    Degree270 = 3,
}

impl Default for VideoRotation {
    fn default() -> Self {
        VideoRotation::Degree0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VideoMetadata {
    pub spatial_layer: u8,
    pub temporal_layer: u8,
    pub is_keyframe: bool,
    pub rotation: VideoRotation,
    pub width: u16,
    pub height: u16,
    pub frame_id: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketType {
    Audio,
    Video,
    Data,
    Rtcp,
}

impl Default for PacketType {
    fn default() -> Self {
        PacketType::Audio
    }
}

pub struct MediaPacket {
    pub len: usize,
    pub data: [u8; 1500],
    pub timestamp: u64,
    pub seq: u16,
    pub ssrc: u32,
    pub layer: u8,
    pub kind: PacketType,
    pub video: VideoMetadata,
    pub arrival_ns: u64,
    pub transport_seq: u16,
}

impl Default for MediaPacket {
    fn default() -> Self {
        Self {
            len: 0,
            data: [0u8; 1500],
            timestamp: 0,
            seq: 0,
            ssrc: 0,
            layer: 0,
            kind: PacketType::Audio,
            video: VideoMetadata::default(),
            arrival_ns: 0,
            transport_seq: 0,
        }
    }
}

impl MediaPacket {
    pub fn from_slice(s: &[u8]) -> Self {
        let mut pkt = Self::default();
        let len = s.len().min(1500);
        pkt.data[..len].copy_from_slice(&s[..len]);
        pkt.len = len;
        if let Some(h) = RtpHeader::parse(&s[..len]) {
            pkt.seq = h.sequence_number;
            pkt.timestamp = h.timestamp as u64;
            pkt.ssrc = h.ssrc;
        } else if len >= 12 {
            pkt.seq = u16::from_be_bytes([s[2], s[3]]);
            pkt.timestamp = u32::from_be_bytes([s[4], s[5], s[6], s[7]]) as u64;
            pkt.ssrc = u32::from_be_bytes([s[8], s[9], s[10], s[11]]);
        }
        pkt
    }

    pub fn payload_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }

    pub fn payload(&self) -> &[u8] {
        &self.data[..self.len]
    }

    pub fn reserve_tail(&mut self, extra: usize) -> bool {
        self.len + extra <= self.data.len()
    }

    pub fn parse_header(&self) -> Option<RtpHeader> {
        RtpHeader::parse(&self.data[..self.len])
    }

    pub fn is_video(&self) -> bool {
        self.kind == PacketType::Video
    }

    pub fn is_audio(&self) -> bool {
        self.kind == PacketType::Audio
    }
}

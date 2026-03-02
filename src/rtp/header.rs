use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsrcList {
    pub list: [u32; 15],
    pub count: u8,
}

impl CsrcList {
    pub fn new() -> Self {
        Self { list: [0u32; 15], count: 0 }
    }

    pub fn push(&mut self, csrc: u32) -> bool {
        if self.count >= 15 {
            return false;
        }
        self.list[self.count as usize] = csrc;
        self.count += 1;
        true
    }

    pub fn as_slice(&self) -> &[u32] {
        &self.list[..self.count as usize]
    }
}

impl Default for CsrcList {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RtpExtension {
    pub profile: u16,
    pub data: [u8; 256],
    pub length: usize,
}

impl RtpExtension {
    pub fn new(profile: u16) -> Self {
        Self { profile, data: [0u8; 256], length: 0 }
    }

    pub fn write_data(&mut self, src: &[u8]) -> bool {
        let len = src.len().min(256);
        self.data[..len].copy_from_slice(&src[..len]);
        self.length = len;
        src.len() <= 256
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.length]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RtpExtensionMap {
    pub abs_send_time_id: Option<u8>,
    pub transport_sequence_number_id: Option<u8>,
    pub video_orientation_id: Option<u8>,
    pub audio_level_id: Option<u8>,
    pub mid_id: Option<u8>,
    pub rid_id: Option<u8>,
    pub rrid_id: Option<u8>,
}

impl RtpExtensionMap {
    pub fn new() -> Self {
        Self {
            abs_send_time_id: None,
            transport_sequence_number_id: None,
            video_orientation_id: None,
            audio_level_id: None,
            mid_id: None,
            rid_id: None,
            rrid_id: None,
        }
    }
}

impl Default for RtpExtensionMap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RtpHeader {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub csrc_count: u8,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub csrc: CsrcList,
    pub ext: Option<RtpExtension>,
    pub header_size: usize,
}

impl RtpHeader {
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 12 {
            return None;
        }
        let version = (buf[0] >> 6) & 0x3;
        if version != 2 {
            return None;
        }
        let padding = (buf[0] >> 5) & 0x1 == 1;
        let extension = (buf[0] >> 4) & 0x1 == 1;
        let csrc_count = buf[0] & 0x0f;
        let marker = (buf[1] >> 7) & 0x1 == 1;
        let payload_type = buf[1] & 0x7f;
        let sequence_number = u16::from_be_bytes([buf[2], buf[3]]);
        let timestamp = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ssrc = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);

        let mut offset = 12usize;
        let mut csrc_list = CsrcList::new();
        for _ in 0..csrc_count {
            if offset + 4 > buf.len() {
                return None;
            }
            let c = u32::from_be_bytes([buf[offset], buf[offset+1], buf[offset+2], buf[offset+3]]);
            csrc_list.push(c);
            offset += 4;
        }

        let mut ext = None;
        if extension {
            if offset + 4 > buf.len() {
                return None;
            }
            let profile = u16::from_be_bytes([buf[offset], buf[offset+1]]);
            let ext_len_words = u16::from_be_bytes([buf[offset+2], buf[offset+3]]) as usize;
            offset += 4;
            let ext_bytes = ext_len_words * 4;
            if offset + ext_bytes > buf.len() {
                return None;
            }
            let mut e = RtpExtension::new(profile);
            e.write_data(&buf[offset..offset + ext_bytes]);
            offset += ext_bytes;
            ext = Some(e);
        }

        Some(Self {
            version,
            padding,
            extension,
            csrc_count,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrc: csrc_list,
            ext,
            header_size: offset,
        })
    }

    pub fn write_into(&self, buf: &mut [u8]) -> Option<usize> {
        let ext_flag = if self.ext.is_some() { 1u8 } else { 0u8 };
        let min_size = 12 + self.csrc_count as usize * 4;
        if buf.len() < min_size {
            return None;
        }
        buf[0] = (2 << 6) | ((self.padding as u8) << 5) | (ext_flag << 4) | (self.csrc_count & 0x0f);
        buf[1] = ((self.marker as u8) << 7) | (self.payload_type & 0x7f);
        buf[2..4].copy_from_slice(&self.sequence_number.to_be_bytes());
        buf[4..8].copy_from_slice(&self.timestamp.to_be_bytes());
        buf[8..12].copy_from_slice(&self.ssrc.to_be_bytes());
        let mut off = 12usize;
        for &c in self.csrc.as_slice() {
            if off + 4 > buf.len() { return None; }
            buf[off..off+4].copy_from_slice(&c.to_be_bytes());
            off += 4;
        }
        if let Some(ref e) = self.ext {
            if off + 4 + e.length > buf.len() { return None; }
            buf[off..off+2].copy_from_slice(&e.profile.to_be_bytes());
            let words = ((e.length + 3) / 4) as u16;
            buf[off+2..off+4].copy_from_slice(&words.to_be_bytes());
            off += 4;
            buf[off..off + e.length].copy_from_slice(e.as_slice());
            off += (words as usize) * 4;
        }
        Some(off)
    }
}

impl fmt::Display for RtpHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RTP seq={} ts={} ssrc={:#010x} pt={} m={}", self.sequence_number, self.timestamp, self.ssrc, self.payload_type, self.marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        let mut buf = [0u8; 64];
        let hdr = RtpHeader {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: true,
            payload_type: 111,
            sequence_number: 1234,
            timestamp: 9876,
            ssrc: 0xdeadbeef,
            csrc: CsrcList::new(),
            ext: None,
            header_size: 12,
        };
        let n = hdr.write_into(&mut buf).unwrap();
        assert_eq!(n, 12);
        let parsed = RtpHeader::parse(&buf[..n]).unwrap();
        assert_eq!(parsed.sequence_number, 1234);
        assert_eq!(parsed.ssrc, 0xdeadbeef);
        assert_eq!(parsed.payload_type, 111);
        assert!(parsed.marker);
    }

    #[test]
    fn parse_requires_v2() {
        let buf = [0x00u8; 12];
        assert!(RtpHeader::parse(&buf).is_none());
    }

    #[test]
    fn parse_too_short() {
        let buf = [0x80u8; 11];
        assert!(RtpHeader::parse(&buf).is_none());
    }
}

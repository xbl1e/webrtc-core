use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SdpParseError {
    #[error("invalid line: {0}")]
    InvalidLine(String),
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("invalid attribute: {0}")]
    InvalidAttribute(String),
    #[error("invalid media description: {0}")]
    InvalidMedia(String),
    #[error("invalid connection data: {0}")]
    InvalidConnection(String),
    #[error("invalid timing: {0}")]
    InvalidTiming(String),
    #[error("invalid origin: {0}")]
    InvalidOrigin(String),
}

#[derive(Clone, Debug, Default)]
pub struct SdpOrigin {
    pub username: String,
    pub session_id: u64,
    pub session_version: u64,
    pub nettype: String,
    pub addrtype: String,
    pub unicast_address: String,
}

#[derive(Clone, Debug, Default)]
pub struct SdpTiming {
    pub start: u64,
    pub stop: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SdpConnection {
    pub nettype: String,
    pub addrtype: String,
    pub connection_address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SdpDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
}

impl Default for SdpDirection {
    fn default() -> Self {
        SdpDirection::SendRecv
    }
}

impl SdpDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            SdpDirection::SendRecv => "sendrecv",
            SdpDirection::SendOnly => "sendonly",
            SdpDirection::RecvOnly => "recvonly",
            SdpDirection::Inactive => "inactive",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sendrecv" => Some(SdpDirection::SendRecv),
            "sendonly" => Some(SdpDirection::SendOnly),
            "recvonly" => Some(SdpDirection::RecvOnly),
            "inactive" => Some(SdpDirection::Inactive),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SdpRtpMap {
    pub payload_type: u8,
    pub encoding_name: String,
    pub clock_rate: u32,
    pub encoding_params: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SdpFmtp {
    pub payload_type: u8,
    pub format: String,
}

#[derive(Clone, Debug, Default)]
pub struct SdpSsrc {
    pub ssrc: u32,
    pub cname: Option<String>,
    pub msid: Option<String>,
    pub mslabel: Option<String>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SdpMedia {
    pub media_type: String,
    pub port: u16,
    pub port_count: Option<u16>,
    pub proto: String,
    pub formats: Vec<String>,
    pub mid: Option<String>,
    pub direction: SdpDirection,
    pub connection: Option<SdpConnection>,
    pub rtpmaps: HashMap<u8, SdpRtpMap>,
    pub fmtps: Vec<SdpFmtp>,
    pub ssrcs: Vec<SdpSsrc>,
    pub ice_ufrag: Option<String>,
    pub ice_pwd: Option<String>,
    pub candidates: Vec<String>,
    pub rtcp_mux: bool,
    pub bundle_only: bool,
}

impl SdpMedia {
    pub fn is_audio(&self) -> bool {
        self.media_type == "audio"
    }

    pub fn is_video(&self) -> bool {
        self.media_type == "video"
    }

    pub fn is_application(&self) -> bool {
        self.media_type == "application"
    }
}

#[derive(Clone, Debug, Default)]
pub struct SdpSession {
    pub version: u32,
    pub origin: SdpOrigin,
    pub session_name: String,
    pub session_info: Option<String>,
    pub timing: SdpTiming,
    pub connection: Option<SdpConnection>,
    pub media: Vec<SdpMedia>,
    pub group: Option<String>,
    pub mids: Vec<String>,
    pub ice_ufrag: Option<String>,
    pub ice_pwd: Option<String>,
    pub fingerprint: Option<String>,
    pub setup: Option<String>,
}

impl SdpSession {
    pub fn parse(s: &str) -> Result<Self, SdpParseError> {
        let mut session = SdpSession::default();
        let mut current_media: Option<SdpMedia> = None;
        let mut current_ssrc: Option<SdpSsrc> = None;

        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.len() < 2 || !line.contains('=') {
                continue;
            }

            let type_char = line.chars().next().unwrap();
            let value = &line[2..];

            match type_char {
                'v' => {
                    session.version = value.parse().unwrap_or(0);
                }
                'o' => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 6 {
                        session.origin = SdpOrigin {
                            username: parts[0].to_string(),
                            session_id: parts[1].parse().unwrap_or(0),
                            session_version: parts[2].parse().unwrap_or(0),
                            nettype: parts[3].to_string(),
                            addrtype: parts[4].to_string(),
                            unicast_address: parts[5].to_string(),
                        };
                    }
                }
                's' => {
                    session.session_name = value.to_string();
                }
                'i' => {
                    if current_media.is_none() {
                        session.session_info = Some(value.to_string());
                    }
                }
                't' => {
                    let parts: Vec<&str> = value.split_whitespace().collect();
                    if parts.len() >= 2 {
                        session.timing = SdpTiming {
                            start: parts[0].parse().unwrap_or(0),
                            stop: parts[1].parse().unwrap_or(0),
                        };
                    }
                }
                'c' => {
                    let conn = parse_connection(value)?;
                    if let Some(ref mut media) = current_media {
                        media.connection = Some(conn);
                    } else {
                        session.connection = Some(conn);
                    }
                }
                'm' => {
                    if let Some(media) = current_media.take() {
                        session.media.push(media);
                    }
                    current_media = Some(parse_media_line(value)?);
                }
                'a' => {
                    let (attr_name, attr_value) = if let Some(eq_pos) = value.find(':') {
                        (&value[..eq_pos], Some(&value[eq_pos + 1..]))
                    } else {
                        (value, None)
                    };

                    if let Some(ref mut media) = current_media {
                        match attr_name {
                            "mid" => {
                                media.mid = attr_value.map(|v| v.to_string());
                                if let Some(mid) = &media.mid {
                                    session.mids.push(mid.clone());
                                }
                            }
                            "sendrecv" => media.direction = SdpDirection::SendRecv,
                            "sendonly" => media.direction = SdpDirection::SendOnly,
                            "recvonly" => media.direction = SdpDirection::RecvOnly,
                            "inactive" => media.direction = SdpDirection::Inactive,
                            "rtpmap" => {
                                if let Some(v) = attr_value {
                                    if let Some(rtpmap) = parse_rtpmap(v) {
                                        media.rtpmaps.insert(rtpmap.payload_type, rtpmap);
                                    }
                                }
                            }
                            "fmtp" => {
                                if let Some(v) = attr_value {
                                    if let Some(fmtp) = parse_fmtp(v) {
                                        media.fmtps.push(fmtp);
                                    }
                                }
                            }
                            "ssrc" => {
                                if let Some(v) = attr_value {
                                    let parts: Vec<&str> = v.split_whitespace().collect();
                                    if !parts.is_empty() {
                                        if let Some(ref ssrc) = current_ssrc {
                                            media.ssrcs.push(ssrc.clone());
                                        }
                                        let ssrc_val = parts[0].parse().unwrap_or(0);
                                        current_ssrc = Some(SdpSsrc {
                                            ssrc: ssrc_val,
                                            ..Default::default()
                                        });
                                        if parts.len() > 1 {
                                            let attr_parts: Vec<&str> = parts[1].split(':').collect();
                                            if attr_parts.len() >= 2 {
                                                match attr_parts[0] {
                                                    "cname" => current_ssrc.as_mut().unwrap().cname = Some(attr_parts[1].to_string()),
                                                    "msid" => current_ssrc.as_mut().unwrap().msid = Some(attr_parts[1].to_string()),
                                                    "mslabel" => current_ssrc.as_mut().unwrap().mslabel = Some(attr_parts[1].to_string()),
                                                    "label" => current_ssrc.as_mut().unwrap().label = Some(attr_parts[1].to_string()),
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            "ice-ufrag" => media.ice_ufrag = attr_value.map(|v| v.to_string()),
                            "ice-pwd" => media.ice_pwd = attr_value.map(|v| v.to_string()),
                            "candidate" => {
                                if let Some(v) = attr_value {
                                    media.candidates.push(v.to_string());
                                }
                            }
                            "rtcp-mux" => media.rtcp_mux = true,
                            "bundle-only" => media.bundle_only = true,
                            _ => {}
                        }
                    } else {
                        match attr_name {
                            "group" => {
                                if let Some(v) = attr_value {
                                    let parts: Vec<&str> = v.split_whitespace().collect();
                                    if !parts.is_empty() {
                                        session.group = Some(parts[0].to_string());
                                        session.mids = parts[1..].iter().map(|s| s.to_string()).collect();
                                    }
                                }
                            }
                            "ice-ufrag" => session.ice_ufrag = attr_value.map(|v| v.to_string()),
                            "ice-pwd" => session.ice_pwd = attr_value.map(|v| v.to_string()),
                            "fingerprint" => session.fingerprint = attr_value.map(|v| v.to_string()),
                            "setup" => session.setup = attr_value.map(|v| v.to_string()),
                            "msid-semantic" => {}
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(ssrc) = current_ssrc {
            if let Some(ref mut media) = current_media {
                media.ssrcs.push(ssrc);
            }
        }
        if let Some(media) = current_media {
            session.media.push(media);
        }

        Ok(session)
    }

    pub fn to_sdp(&self) -> String {
        let mut s = String::new();

        s.push_str(&format!("v={}\r\n", self.version));
        s.push_str(&format!(
            "o={} {} {} {} {} {}\r\n",
            self.origin.username,
            self.origin.session_id,
            self.origin.session_version,
            self.origin.nettype,
            self.origin.addrtype,
            self.origin.unicast_address
        ));
        s.push_str(&format!("s={}\r\n", self.session_name));
        s.push_str(&format!("t={} {}\r\n", self.timing.start, self.timing.stop));

        if let Some(ref conn) = self.connection {
            s.push_str(&format!(
                "c={} {} {}\r\n",
                conn.nettype, conn.addrtype, conn.connection_address
            ));
        }

        if let Some(ref group) = self.group {
            s.push_str(&format!("a=group:{} {}\r\n", group, self.mids.join(" ")));
        }

        if let Some(ref ufrag) = self.ice_ufrag {
            s.push_str(&format!("a=ice-ufrag:{}\r\n", ufrag));
        }
        if let Some(ref pwd) = self.ice_pwd {
            s.push_str(&format!("a=ice-pwd:{}\r\n", pwd));
        }
        if let Some(ref fp) = self.fingerprint {
            s.push_str(&format!("a=fingerprint:{}\r\n", fp));
        }
        if let Some(ref setup) = self.setup {
            s.push_str(&format!("a=setup:{}\r\n", setup));
        }

        for media in &self.media {
            let port_str = if let Some(count) = media.port_count {
                format!("{}/{}", media.port, count)
            } else {
                media.port.to_string()
            };
            s.push_str(&format!(
                "m={} {} {} {}\r\n",
                media.media_type,
                port_str,
                media.proto,
                media.formats.join(" ")
            ));

            if let Some(ref conn) = media.connection {
                s.push_str(&format!(
                    "c={} {} {}\r\n",
                    conn.nettype, conn.addrtype, conn.connection_address
                ));
            }

            if let Some(ref mid) = media.mid {
                s.push_str(&format!("a=mid:{}\r\n", mid));
            }

            s.push_str(&format!("a={}\r\n", media.direction.as_str()));

            for (pt, rtpmap) in &media.rtpmaps {
                let params = if let Some(ref p) = rtpmap.encoding_params {
                    format!("/{}", p)
                } else {
                    String::new()
                };
                s.push_str(&format!(
                    "a=rtpmap:{} {}/{}{}\r\n",
                    pt, rtpmap.encoding_name, rtpmap.clock_rate, params
                ));
            }

            for fmtp in &media.fmtps {
                s.push_str(&format!("a=fmtp:{} {}\r\n", fmtp.payload_type, fmtp.format));
            }

            for ssrc in &media.ssrcs {
                s.push_str(&format!("a=ssrc:{}", ssrc.ssrc));
                if let Some(ref cname) = ssrc.cname {
                    s.push_str(&format!(" cname:{}", cname));
                }
                s.push_str("\r\n");
            }

            if let Some(ref ufrag) = media.ice_ufrag {
                s.push_str(&format!("a=ice-ufrag:{}\r\n", ufrag));
            }
            if let Some(ref pwd) = media.ice_pwd {
                s.push_str(&format!("a=ice-pwd:{}\r\n", pwd));
            }

            for candidate in &media.candidates {
                s.push_str(&format!("a=candidate:{}\r\n", candidate));
            }

            if media.rtcp_mux {
                s.push_str("a=rtcp-mux\r\n");
            }
        }

        s
    }

    pub fn media_by_mid(&self, mid: &str) -> Option<&SdpMedia> {
        self.media.iter().find(|m| m.mid.as_deref() == Some(mid))
    }

    pub fn media_by_mid_mut(&mut self, mid: &str) -> Option<&mut SdpMedia> {
        self.media.iter_mut().find(|m| m.mid.as_deref() == Some(mid))
    }
}

fn parse_connection(value: &str) -> Result<SdpConnection, SdpParseError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(SdpParseError::InvalidConnection(value.to_string()));
    }
    Ok(SdpConnection {
        nettype: parts[0].to_string(),
        addrtype: parts[1].to_string(),
        connection_address: parts[2].to_string(),
    })
}

fn parse_media_line(value: &str) -> Result<SdpMedia, SdpParseError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(SdpParseError::InvalidMedia(value.to_string()));
    }

    let (port, port_count) = if parts[1].contains('/') {
        let port_parts: Vec<&str> = parts[1].split('/').collect();
        (
            port_parts[0].parse().unwrap_or(0),
            Some(port_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1)),
        )
    } else {
        (parts[1].parse().unwrap_or(0), None)
    };

    Ok(SdpMedia {
        media_type: parts[0].to_string(),
        port,
        port_count,
        proto: parts[2].to_string(),
        formats: parts[3..].iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    })
}

fn parse_rtpmap(value: &str) -> Option<SdpRtpMap> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let pt: u8 = parts[0].parse().ok()?;
    if parts.len() < 2 {
        return None;
    }

    let encoding_parts: Vec<&str> = parts[1].split('/').collect();
    if encoding_parts.is_empty() {
        return None;
    }

    Some(SdpRtpMap {
        payload_type: pt,
        encoding_name: encoding_parts[0].to_string(),
        clock_rate: encoding_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(90000),
        encoding_params: encoding_parts.get(2).map(|s| s.to_string()),
    })
}

fn parse_fmtp(value: &str) -> Option<SdpFmtp> {
    let space_pos = value.find(' ')?;
    let pt: u8 = value[..space_pos].parse().ok()?;
    let format = value[space_pos + 1..].to_string();
    Some(SdpFmtp {
        payload_type: pt,
        format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_sdp() {
        let sdp = "v=0\r\n\
o=- 123 456 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=mid:0\r\n\
a=sendrecv\r\n\
a=rtpmap:111 opus/48000/2\r\n";
        let session = SdpSession::parse(sdp).unwrap();
        assert_eq!(session.version, 0);
        assert_eq!(session.origin.session_id, 123);
        assert_eq!(session.media.len(), 1);
        assert_eq!(session.media[0].mid, Some("0".to_string()));
        assert!(session.media[0].rtpmaps.contains_key(&111));
    }

    #[test]
    fn sdp_roundtrip() {
        let sdp = "v=0\r\n\
o=- 0 0 IN IP4 0.0.0.0\r\n\
s=-\r\n\
t=0 0\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
a=mid:video\r\n\
a=sendrecv\r\n\
a=rtpmap:96 VP8/90000\r\n";
        let session = SdpSession::parse(sdp).unwrap();
        let resdp = session.to_sdp();
        assert!(resdp.contains("m=video"));
        assert!(resdp.contains("VP8"));
    }

    #[test]
    fn parse_direction() {
        assert_eq!(SdpDirection::from_str("sendrecv"), Some(SdpDirection::SendRecv));
        assert_eq!(SdpDirection::from_str("sendonly"), Some(SdpDirection::SendOnly));
        assert_eq!(SdpDirection::from_str("recvonly"), Some(SdpDirection::RecvOnly));
        assert_eq!(SdpDirection::from_str("inactive"), Some(SdpDirection::Inactive));
    }
}

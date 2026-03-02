use webrtc_core::pc::{SdpSession, SdpDirection};

fn main() {
    println!("SDP Parser Example\n");

    let sdp = "v=0\r\n\
o=- 1234567890 1234567891 IN IP4 192.168.1.100\r\n\
s=WebRTC Session\r\n\
t=0 0\r\n\
a=group:BUNDLE 0 1\r\n\
a=ice-ufrag:local_ufrag\r\n\
a=ice-pwd:local_password_12345\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=mid:0\r\n\
a=sendrecv\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=ssrc:123456 cname:user1\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 97\r\n\
a=mid:1\r\n\
a=sendonly\r\n\
a=rtpmap:96 VP8/90000\r\n\
a=rtpmap:97 VP9/90000\r\n\
";

    let session = SdpSession::parse(sdp).expect("parse failed");
    
    println!("Session: {}", session.session_name);
    println!("Origin: {}@{}", session.origin.username, session.origin.unicast_address);
    
    println!("\nICE:");
    if let Some(ref u) = session.ice_ufrag { println!("  Ufrag: {}", u); }
    if let Some(ref p) = session.ice_pwd { println!("  Pwd: {}", p); }
    
    println!("\nBundle: {:?}", session.group);
    println!("MIDs: {:?}", session.mids);

    println!("\nMedia:");
    for (i, m) in session.media.iter().enumerate() {
        println!("  [{}] {} mid={:?} dir={}", i, m.media_type, m.mid, m.direction.as_str());
        for (pt, r) in &m.rtpmaps {
            println!("       PT {}: {}/{}", pt, r.encoding_name, r.clock_rate);
        }
    }

    println!("\nLookup by MID:");
    if let Some(m) = session.media_by_mid("0") {
        println!("  mid=0: {}", m.media_type);
    }
    if let Some(m) = session.media_by_mid("1") {
        println!("  mid=1: {}", m.media_type);
    }

    println!("\nDirection conversion:");
    for d in [SdpDirection::SendRecv, SdpDirection::SendOnly, SdpDirection::RecvOnly, SdpDirection::Inactive] {
        println!("  {} -> {:?}", d.as_str(), SdpDirection::from_str(d.as_str()));
    }
}

use webrtc_core::pc::{SdpSession, SdpDirection};

fn main() {
    println!("=== SDP Parser Example ===\n");

    let offer_sdp = "v=0\r\n\
o=- 1234567890 1234567891 IN IP4 192.168.1.100\r\n\
s=WebRTC Session\r\n\
t=0 0\r\n\
a=group:BUNDLE 0 1\r\n\
a=ice-ufrag:local_ufrag\r\n\
a=ice-pwd:local_password_12345\r\n\
a=fingerprint:sha-256 01:23:45:67:89:AB:CD:EF\r\n\
a=setup:actpass\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
c=IN IP4 0.0.0.0\r\n\
a=mid:0\r\n\
a=sendrecv\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=fmtp:111 minptime=10;useinbandfec=1\r\n\
a=ssrc:123456 cname:user1\r\n\
a=ssrc:123456 msid:stream1 track1\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 97\r\n\
c=IN IP4 0.0.0.0\r\n\
a=mid:1\r\n\
a=sendonly\r\n\
a=rtpmap:96 VP8/90000\r\n\
a=rtpmap:97 VP9/90000\r\n\
a=ssrc:789012 cname:user1\r\n\
";

    println!("=== Parsing Offer SDP ===");
    let session = SdpSession::parse(offer_sdp).expect("Failed to parse SDP");
    
    println!("Session version: {}", session.version);
    println!("Session name: {}", session.session_name);
    println!("Origin: {}@{} ({})",
        session.origin.username,
        session.origin.session_id,
        session.origin.unicast_address);
    
    println!("\nICE credentials:");
    if let Some(ref ufrag) = session.ice_ufrag {
        println!("  Ufrag: {}", ufrag);
    }
    if let Some(ref pwd) = session.ice_pwd {
        println!("  Password: {}", pwd);
    }
    
    println!("\nBundle group: {:?}", session.group);
    println!("MIDs: {:?}", session.mids);
    
    println!("\n=== Media Lines ===");
    for (i, media) in session.media.iter().enumerate() {
        println!("\nMedia {}:", i);
        println!("  Type: {}", media.media_type);
        println!("  MID: {:?}", media.mid);
        println!("  Direction: {}", media.direction.as_str());
        println!("  Formats: {:?}", media.formats);
        
        println!("  RTP Maps:");
        for (pt, rtpmap) in &media.rtpmaps {
            println!("    PT {}: {}/{}{}",
                pt, rtpmap.encoding_name, rtpmap.clock_rate,
                rtpmap.encoding_params.as_ref()
                    .map(|p| format!("/{}", p))
                    .unwrap_or_default());
        }
        
        if !media.fmtps.is_empty() {
            println!("  FMTP:");
            for fmtp in &media.fmtps {
                println!("    PT {}: {}", fmtp.payload_type, fmtp.format);
            }
        }
        
        if !media.ssrcs.is_empty() {
            println!("  SSRCs:");
            for ssrc in &media.ssrcs {
                println!("    {}: cname={:?}",
                    ssrc.ssrc, ssrc.cname);
            }
        }
    }

    println!("\n=== Looking up media by MID ===");
    if let Some(audio_media) = session.media_by_mid("0") {
        println!("Audio media found: {}", audio_media.media_type);
        println!("Is audio: {}", audio_media.is_audio());
    }
    if let Some(video_media) = session.media_by_mid("1") {
        println!("Video media found: {}", video_media.media_type);
        println!("Is video: {}", video_media.is_video());
    }

    println!("\n=== Generating SDP ===");
    let regenerated = session.to_sdp();
    println!("Regenerated SDP (first 500 chars):");
    println!("{}", &regenerated[..regenerated.len().min(500)]);

    println!("\n=== Direction Conversion ===");
    for dir in [SdpDirection::SendRecv, SdpDirection::SendOnly, 
                SdpDirection::RecvOnly, SdpDirection::Inactive] {
        println!("{} -> {:?}", dir.as_str(), SdpDirection::from_str(dir.as_str()));
    }

    println!("\n=== SDP Parser Example Complete ===");
}

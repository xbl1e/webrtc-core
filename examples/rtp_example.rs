use webrtc_core::rtp::{RtpHeader, CsrcList, Packetizer, PacketizerConfig, Depacketizer, MediaPacket, PacketType};

fn main() {
    println!("RTP Pipeline Example\n");

    let cfg = PacketizerConfig {
        payload_type: 96,
        ssrc: 0xDEADBEEF,
        clock_rate: 90000,
        mtu: 1200,
        initial_seq: 1000,
        initial_timestamp: 90000,
    };
    let mut packetizer = Packetizer::new(cfg);
    println!("PT: {}, SSRC: {:#X}", cfg.payload_type, cfg.ssrc);

    let video_frame = vec![0xAAu8; 3000];
    let mut out_pkts: Vec<MediaPacket> = (0..8).map(|_| MediaPacket::default()).collect();
    
    println!("\nPacketization:");
    println!("Input: {} bytes", video_frame.len());
    let n = packetizer.packetize(&video_frame, 3000, PacketType::Video, &mut out_pkts);
    println!("Output: {} packets", n);

    for i in 0..n {
        if let Some(hdr) = out_pkts[i].parse_header() {
            println!("  [{}] seq={} marker={} {} bytes", i, hdr.sequence_number, hdr.marker, out_pkts[i].len);
        }
    }

    println!("\nDepacketization:");
    let mut depacketizer = Depacketizer::new(64);
    let mut total = 0usize;

    for i in 0..n {
        let raw = &out_pkts[i].data[..out_pkts[i].len];
        let mut buf = vec![0u8; 2000];
        if let Ok((hdr, plen)) = depacketizer.process(raw, &mut buf) {
            total += plen;
            if hdr.marker {
                println!("Frame complete: {} bytes", total);
            }
        }
    }

    println!("\nRTP Header:");
    let hdr = RtpHeader {
        version: 2,
        padding: false,
        extension: false,
        csrc_count: 0,
        marker: true,
        payload_type: 111,
        sequence_number: 1234,
        timestamp: 9876,
        ssrc: 0xDEADBEEF,
        csrc: CsrcList::new(),
        ext: None,
        header_size: 12,
    };
    
    let mut buf = [0u8; 64];
    hdr.write_into(&mut buf).unwrap();
    let parsed = RtpHeader::parse(&buf).unwrap();
    println!("{}", parsed);
}

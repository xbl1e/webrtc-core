use webrtc_core::rtp::{RtpHeader, CsrcList, Packetizer, PacketizerConfig, Depacketizer, MediaPacket, PacketType};

fn main() {
    println!("=== RTP Packetizer/Depacketizer Example ===\n");

    let cfg = PacketizerConfig {
        payload_type: 96,
        ssrc: 0xDEADBEEF,
        clock_rate: 90000,
        mtu: 1200,
        initial_seq: 1000,
        initial_timestamp: 90000,
    };
    let mut packetizer = Packetizer::new(cfg);
    println!("Created packetizer with PT={}, SSRC={:#010X}", 
        cfg.payload_type, cfg.ssrc);

    let video_frame = vec![0xAAu8; 3000];
    let mut out_pkts: Vec<MediaPacket> = (0..8).map(|_| MediaPacket::default()).collect();
    
    println!("\n=== Packetization ===");
    println!("Input frame: {} bytes", video_frame.len());
    let n = packetizer.packetize(&video_frame, 3000, PacketType::Video, &mut out_pkts);
    println!("Output: {} RTP packets", n);

    for i in 0..n {
        let pkt = &out_pkts[i];
        if let Some(hdr) = pkt.parse_header() {
            println!("  Packet {}: seq={}, ts={}, marker={}, {} bytes",
                i, hdr.sequence_number, hdr.timestamp, hdr.marker, pkt.len);
        }
    }

    println!("\n=== Depacketization ===");
    let mut depacketizer = Depacketizer::new(64);
    let mut reassembled_bytes = 0usize;
    let mut frame_count = 0usize;

    for i in 0..n {
        let raw = &out_pkts[i].data[..out_pkts[i].len];
        let mut payload_buf = vec![0u8; 2000];
        
        if let Ok((hdr, plen)) = depacketizer.process(raw, &mut payload_buf) {
            reassembled_bytes += plen;
            if hdr.marker {
                frame_count += 1;
                println!("  Frame {} complete: seq={}, total {} bytes reassembled",
                    frame_count, hdr.sequence_number, reassembled_bytes);
            }
        }
    }

    println!("\n=== RTP Header Parsing ===");
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
    let written = hdr.write_into(&mut buf).unwrap();
    println!("Serialized header: {} bytes", written);
    
    let parsed = RtpHeader::parse(&buf[..written]).unwrap();
    println!("Parsed: {}", parsed);
    println!("  Version: {}", parsed.version);
    println!("  Marker: {}", parsed.marker);
    println!("  PT: {}", parsed.payload_type);
    println!("  Seq: {}", parsed.sequence_number);
    println!("  Timestamp: {}", parsed.timestamp);
    println!("  SSRC: {:#010X}", parsed.ssrc);

    println!("\n=== Out-of-Order Handling ===");
    println!("Out-of-order packets detected: {}", depacketizer.out_of_order_count());
    println!("Total packets processed: {}", depacketizer.total_received());

    println!("\n=== RTP Example Complete ===");
}

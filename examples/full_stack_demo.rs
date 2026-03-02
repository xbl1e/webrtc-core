use std::sync::Arc;
use std::time::Instant;
use webrtc_core::{
    cc::{GccConfig, GccController, TwccAggregator, TwccFeedback},
    e2ee::{KeyStore, SFrameConfig, SFrameContext},
    ice::{IceAgent, IceAgentConfig, IceRole},
    observability::EngineMetrics,
    pc::{configuration::RtcConfiguration, peer_connection::PeerConnection, transceiver::MediaKind},
    rtp::{
        depacketizer::Depacketizer,
        packet::{MediaPacket, PacketType},
        packetizer::{Packetizer, PacketizerConfig},
    },
    video::{
        quality_scaler::{QualityScaler, QualityScalerConfig},
        scalability::{SvcLayerSelector, SvcMode, SvcLayer},
        simulcast::{SimulcastConfig, SimulcastSelector},
    },
    SlabAllocator,
};

fn main() {
    println!("=== webrtc-core Full Stack Demo ===\n");

    demo_peer_connection_negotiation();
    demo_rtp_pipeline();
    demo_congestion_control();
    demo_sframe_e2ee();
    demo_ice_agent();
    demo_simulcast_adaptation();
    demo_svc_layer_selection();
    demo_quality_scaler();
    demo_metrics_collection();
    demo_srtp_performance();

    println!("\n=== All demos completed successfully ===");
}

fn demo_peer_connection_negotiation() {
    println!("[1] PeerConnection Offer/Answer Negotiation");
    let cfg = RtcConfiguration::new()
        .with_stun("stun:stun.l.google.com:19302")
        .with_turn("turn:turn.example.com:3478", "user", "pass");

    let offerer = PeerConnection::new(cfg.clone());
    let answerer = PeerConnection::new(cfg);

    let _audio = offerer.add_transceiver(MediaKind::Audio);
    let _video = offerer.add_transceiver(MediaKind::Video);

    let offer = offerer.create_offer().expect("create offer");
    println!("  Offer SDP length: {} bytes", offer.sdp.len());

    answerer.set_remote_description(offer).unwrap();
    answerer.add_transceiver(MediaKind::Audio);
    answerer.add_transceiver(MediaKind::Video);
    let answer = answerer.create_answer().expect("create answer");
    println!("  Answer SDP length: {} bytes", answer.sdp.len());

    offerer.set_remote_description(answer).unwrap();

    use webrtc_core::pc::peer_connection::PeerConnectionState;
    assert_eq!(offerer.state(), PeerConnectionState::Connected);
    assert_eq!(answerer.state(), PeerConnectionState::Connected);
    println!("  State: Connected (both peers)");
    println!("  Transceivers: {}", offerer.transceivers().len());
    println!("  OK\n");
}

fn demo_rtp_pipeline() {
    println!("[2] RTP Packetizer/Depacketizer Pipeline");
    let cfg = PacketizerConfig {
        payload_type: 96,
        ssrc: 0xDEADBEEF,
        clock_rate: 90000,
        mtu: 1200,
        initial_seq: 1000,
        initial_timestamp: 90000,
    };
    let mut packetizer = Packetizer::new(cfg);
    let video_frame = vec![0x01u8; 3000];
    let mut out_pkts: Vec<MediaPacket> = (0..8).map(|_| MediaPacket::default()).collect();
    let n = packetizer.packetize(&video_frame, 3000, PacketType::Video, &mut out_pkts);
    println!("  3KB video frame -> {} RTP packets", n);

    let mut depacketizer = Depacketizer::new(64);
    let mut reassembled_bytes = 0usize;
    for i in 0..n {
        let raw = &out_pkts[i].data[..out_pkts[i].len];
        let mut payload_buf = vec![0u8; 2000];
        if let Ok((hdr, plen)) = depacketizer.process(raw, &mut payload_buf) {
            reassembled_bytes += plen;
            if hdr.marker {
                println!("  Frame complete at seq={}, total reassembled={} bytes", hdr.sequence_number, reassembled_bytes);
            }
        }
    }
    println!("  Out-of-order count: {}", depacketizer.out_of_order_count());
    println!("  OK\n");
}

fn demo_congestion_control() {
    println!("[3] GCC Congestion Control");
    let cfg = GccConfig {
        initial_bitrate_bps: 1_000_000,
        min_bitrate_bps: 50_000,
        max_bitrate_bps: 10_000_000,
        ..Default::default()
    };
    let gcc = GccController::new(cfg);

    let agg = TwccAggregator::new();
    for i in 0..20u16 {
        agg.on_packet_sent(i, 1_000_000 * i as u64, 1200);
    }
    for i in 0..20u16 {
        agg.on_packet_received(i, 1_200_000 * i as u64 + 50_000);
    }

    let fb = agg.compute_feedback();
    gcc.on_feedback(&fb);

    println!("  Initial target: 1.0 Mbps");
    println!("  After normal feedback: {:.2} Mbps", gcc.target_bitrate_bps() as f32 / 1_000_000.0);
    println!("  Loss fraction: {:.1}%", agg.loss_fraction() * 100.0);

    let overuse_fb = TwccFeedback {
        sent_count: 10,
        received_count: 10,
        inter_arrival_delta_ns: 150_000_000,
        inter_departure_delta_ns: 0,
    };
    gcc.on_feedback(&overuse_fb);
    gcc.on_feedback(&overuse_fb);
    println!("  After delay overuse: {:.2} Mbps", gcc.target_bitrate_bps() as f32 / 1_000_000.0);
    println!("  OK\n");
}

fn demo_sframe_e2ee() {
    println!("[4] SFrame End-to-End Encryption");
    let store = Arc::new(KeyStore::new());
    let key = [0x42u8; 32];
    let salt = [0x01u8; 12];
    store.add_key(0, &key, salt);

    let alice_ctx = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);
    store.add_key(1, &[0x24u8; 32], [0x02u8; 12]);
    let bob_ctx = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);

    let rtp_header = [0x80u8, 0x60, 0x00, 0x01, 0x00, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x04, 0xD2];
    let plaintext = b"Hello from Alice! This is a confidential video frame payload.";

    let mut encrypted = vec![0u8; 512];
    let enc_len = alice_ctx.encrypt_frame(&rtp_header, plaintext, &mut encrypted).unwrap();
    println!("  Plaintext: {} bytes", plaintext.len());
    println!("  Encrypted: {} bytes (+ {} overhead)", enc_len, enc_len - plaintext.len() - rtp_header.len());

    let mut to_decrypt = encrypted[..enc_len].to_vec();
    let plain_len = bob_ctx.decrypt_frame(&mut to_decrypt, rtp_header.len()).unwrap();
    println!("  Decrypted: {} bytes", plain_len);
    println!("  Keys in store: {}", store.key_count());
    println!("  OK\n");
}

fn demo_ice_agent() {
    println!("[5] ICE Agent Candidate Gathering & Connection");
    let mut offerer_cfg = IceAgentConfig::new();
    offerer_cfg.set_local_credentials("iceufrag1234", "icepassword12345678901");
    offerer_cfg.set_remote_credentials("iceufrag5678", "icepassword98765432109");

    let agent = IceAgent::new(offerer_cfg, IceRole::Controlling);
    let addrs = vec![
        "192.168.1.100:5000".parse().unwrap(),
        "10.0.0.1:5001".parse().unwrap(),
    ];
    agent.gather_host_candidates(&addrs);
    println!("  Gathered {} local candidates", agent.local_candidate_count());

    let remote = webrtc_core::ice::candidate::IceCandidate::new_host(
        "192.168.1.200:6000".parse().unwrap(), 1
    );
    agent.add_remote_candidate(remote);

    use webrtc_core::ice::agent::IceState;
    assert_eq!(agent.state(), IceState::Checking);
    println!("  ICE state: Checking");

    agent.simulate_successful_check();
    assert!(agent.is_connected());
    println!("  ICE state: Connected");
    println!("  Selected address: {:?}", agent.selected_address().unwrap());

    while let Some(event) = agent.poll_event() {
        println!("  Event: {:?}", event);
    }
    println!("  OK\n");
}

fn demo_simulcast_adaptation() {
    println!("[6] Simulcast Bandwidth Adaptation");
    let cfg = SimulcastConfig::standard_3_layer(0x1000);
    let selector = SimulcastSelector::new(cfg);

    let scenarios = [
        (5_000_000u32, "high BW"),
        (1_000_000u32, "medium BW"),
        (300_000u32, "low BW"),
    ];

    for (bps, label) in &scenarios {
        selector.update_bandwidth(*bps);
        let layer = selector.active_layer().unwrap();
        println!("  {} ({} bps): layer={} {}x{} @{} bps",
            label, bps, layer.rid_str(),
            layer.resolution.width, layer.resolution.height,
            layer.max_bitrate_bps
        );
    }
    let (fwd, dropped) = selector.stats_snapshot();
    println!("  OK\n");
    let _ = (fwd, dropped);
}

fn demo_svc_layer_selection() {
    println!("[7] SVC Layer Selection");
    let selector = SvcLayerSelector::new(SvcMode::L3T3);
    selector.set_target(2, 2);
    selector.set_congestion_target(0, 0);

    println!("  Normal mode - L3T3:");
    for s in 0..3u8 {
        for t in 0..3u8 {
            let layer = SvcLayer::new(s, t);
            let fwd = selector.should_forward(layer);
            if fwd {
                println!("    S{}T{}: forwarded", s, t);
            }
        }
    }

    selector.set_congested(true);
    println!("  Congested mode (base layer only):");
    let base_fwd = selector.should_forward(SvcLayer::new(0, 0));
    let s1t1_fwd = selector.should_forward(SvcLayer::new(1, 1));
    println!("    S0T0: {}", if base_fwd { "forwarded" } else { "dropped" });
    println!("    S1T1: {}", if s1t1_fwd { "forwarded" } else { "dropped" });
    println!("  OK\n");
}

fn demo_quality_scaler() {
    println!("[8] Quality Scaler (QP-based Resolution Adaptation)");
    let cfg = QualityScalerConfig {
        min_frames_before_scale: 10,
        high_qp_threshold: 37,
        low_qp_threshold: 20,
        ..Default::default()
    };
    let scaler = QualityScaler::new(cfg, 1280, 720);

    let (w0, h0) = scaler.current_resolution();
    println!("  Initial: {}x{}", w0, h0);

    for _ in 0..10 {
        scaler.report_qp(50);
    }
    let (w1, h1) = scaler.current_resolution();
    println!("  After high QP: {}x{} (scale downs: {})", w1, h1, scaler.scale_down_count());

    for _ in 0..10 {
        scaler.report_qp(10);
    }
    let (w2, h2) = scaler.current_resolution();
    println!("  After low QP: {}x{} (scale ups: {})", w2, h2, scaler.scale_up_count());
    println!("  Average QP: {}", scaler.average_qp());
    println!("  OK\n");
}

fn demo_metrics_collection() {
    println!("[9] Engine Metrics Collection");
    let metrics = EngineMetrics::new();

    for _ in 0..1000 {
        metrics.audio.record_received(160);
        metrics.video.record_received(1200);
    }
    for _ in 0..50 {
        metrics.audio.record_dropped();
    }
    metrics.audio.record_nack();
    metrics.video.record_keyframe();
    metrics.video.record_pli();

    let snap = metrics.audio.snapshot(64_000, 500_000);
    println!("  Audio packets received: {}", snap.packets_received);
    println!("  Audio loss fraction: {:.1}%", snap.loss_fraction * 100.0);
    println!("  NACK count: {}", snap.nack_count);

    let vsnap = metrics.video.snapshot(2_000_000, 1_000_000);
    println!("  Video keyframes: {}", vsnap.keyframe_count);
    println!("  Video PLI count: {}", vsnap.pli_count);
    println!("  Total PPS: {}", metrics.total_pps());
    println!("  OK\n");
}

fn demo_srtp_performance() {
    println!("[10] SRTP Hot Path Performance");
    let slab = Arc::new(SlabAllocator::new(4096));
    let key = [0u8; 32];
    let srtp = webrtc_core::srtp::SrtpContext::new(&key);
    let iterations = 100_000usize;

    let start = Instant::now();
    for _ in 0..iterations {
        if let Some(g) = SlabAllocator::allocate_guard(&slab) {
            unsafe { let p = g.get_mut(); p.len = 160; }
            let idx = g.into_index();
            let _ = srtp.protect_index_inplace(&slab, idx, &[0u8; 12], &[]);
            slab.free(idx);
        }
    }
    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    let throughput_mpps = iterations as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    println!("  Iterations: {}", iterations);
    println!("  Avg SRTP protect: {:.1} ns/packet", avg_ns);
    println!("  Throughput: {:.2} Mpps", throughput_mpps);
    if avg_ns < 1000.0 {
        println!("  Sub-microsecond SRTP: YES ({:.1} ns)", avg_ns);
    }
    println!("  OK\n");
}

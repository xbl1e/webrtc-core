use webrtc_core::video::{
    VideoFrame, VideoCodec, VideoFrameBuffer, FrameAssembler,
    SimulcastConfig, SimulcastSelector, SvcMode, SvcLayer, SvcLayerSelector,
    QualityScaler, QualityScalerConfig,
};

fn main() {
    println!("Video Processing Example\n");

    println!("Frame Buffer:");
    let buffer = VideoFrameBuffer::new(16);
    println!("Capacity: {}", buffer.capacity());

    let frame = VideoFrame::new(VideoCodec::Vp8, 90000);
    let data = vec![0xAAu8; 100];
    buffer.push_frame(frame, &data);
    println!("Pushed frame, len: {}", buffer.len());

    buffer.pop_frame(|f, d| {
        println!("Popped: codec={:?}, ts={}, {} bytes", f.codec, f.rtp_timestamp, d.len());
    });

    println!("\nSimulcast:");
    let cfg = SimulcastConfig::standard_3_layer(0x1000);
    for layer in &cfg.layers {
        println!("  {}: {}x{} @ {} bps", layer.rid_str(), 
            layer.resolution.width, layer.resolution.height, layer.max_bitrate_bps);
    }

    let selector = SimulcastSelector::new(cfg);
    println!("\nBandwidth adaptation:");
    for (bps, _) in [(5_000_000, "high"), (1_000_000, "med"), (300_000, "low")] {
        selector.update_bandwidth(bps);
        if let Some(layer) = selector.active_layer() {
            println!("  {} bps -> layer {} @ {} bps", bps, layer.rid_str(), layer.max_bitrate_bps);
        }
    }

    println!("\nSVC Layer Selection:");
    let svc = SvcLayerSelector::new(SvcMode::L3T3);
    svc.set_target(2, 2);
    println!("Mode: L3T3, target S2T2");

    print!("Forwarding:");
    for s in 0..3 {
        for t in 0..3 {
            if svc.should_forward(SvcLayer::new(s, t)) {
                print!(" S{}T{}", s, t);
            }
        }
    }
    println!();

    println!("\nQuality Scaler:");
    let scaler = QualityScaler::new(QualityScalerConfig {
        min_frames_before_scale: 5,
        high_qp_threshold: 37,
        low_qp_threshold: 20,
        ..Default::default()
    }, 1280, 720);

    let (w, h) = scaler.current_resolution();
    println!("Start: {}x{}", w, h);

    for _ in 0..5 {
        scaler.report_qp(50);
    }
    let (w, h) = scaler.current_resolution();
    println!("After high QP: {}x{}", w, h);

    for _ in 0..5 {
        scaler.report_qp(10);
    }
    let (w, h) = scaler.current_resolution();
    println!("After low QP: {}x{}", w, h);
}

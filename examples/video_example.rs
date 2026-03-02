use webrtc_core::video::{
    VideoFrame, VideoCodec, VideoFrameType, VideoResolution,
    VideoFrameBuffer, FrameAssembler,
};
use webrtc_core::video::{SimulcastConfig, SimulcastLayer, SimulcastSelector};
use webrtc_core::video::{SvcMode, SvcLayer, SvcLayerSelector};
use webrtc_core::video::{QualityScaler, QualityScalerConfig, ScalingDecision};

fn main() {
    println!("=== Video Processing Example ===\n");

    println!("=== Video Frame Buffer ===");
    let buffer = VideoFrameBuffer::new(16);
    println!("Created frame buffer with capacity: {}", buffer.capacity());

    let frame = VideoFrame::new(VideoCodec::Vp8, 90000);
    let data = vec![0xAAu8; 100];
    buffer.push_frame(frame, &data);
    println!("Pushed frame, buffer len: {}", buffer.len());

    buffer.pop_frame(|f, d| {
        println!("Popped frame: codec={:?}, ts={}, {} bytes",
            f.codec, f.rtp_timestamp, d.len());
    });

    println!("\n=== Frame Assembly ===");
    let assembler = FrameAssembler::new(64, VideoCodec::Vp8);
    assembler.push_rtp_fragment(100, 90000, false, &[0x00, 0x01, 0x02]);
    assembler.push_rtp_fragment(101, 90000, true, &[0x03, 0x04, 0x05]);
    println!("Is frame complete: {}", assembler.is_complete());

    let mut out = vec![0u8; 1024];
    if let Some((frame, len)) = assembler.assemble_into(&mut out) {
        println!("Assembled frame: {} bytes", len);
    }

    println!("\n=== Simulcast Selection ===");
    let cfg = SimulcastConfig::standard_3_layer(0x1000);
    println!("Created simulcast config with {} layers:", cfg.layers.len());
    for layer in &cfg.layers {
        println!("  {}: {}x{} @ {} bps",
            layer.rid_str(), layer.resolution.width, layer.resolution.height,
            layer.max_bitrate_bps);
    }

    let selector = SimulcastSelector::new(cfg);
    
    println!("\nSimulcast bandwidth adaptation:");
    for (bps, label) in [(5_000_000, "high"), (1_000_000, "medium"), (300_000, "low")] {
        selector.update_bandwidth(bps);
        if let Some(layer) = selector.active_layer() {
            println!("  {} BW ({} bps): layer {} @ {} bps",
                label, bps, layer.rid_str(), layer.max_bitrate_bps);
        }
    }

    println!("\n=== SVC Layer Selection ===");
    let svc_selector = SvcLayerSelector::new(SvcMode::L3T3);
    svc_selector.set_target(2, 2);
    println!("Created SVC selector for L3T3 mode");

    println!("\nNormal mode layers forwarded:");
    for s in 0..3u8 {
        for t in 0..3u8 {
            let layer = SvcLayer::new(s, t);
            if svc_selector.should_forward(layer) {
                println!("  S{}T{}: forwarded", s, t);
            }
        }
    }

    svc_selector.set_congestion_target(0, 0);
    svc_selector.set_congested(true);
    println!("\nCongested mode (base layer only):");
    println!("  S0T0 forwarded: {}", svc_selector.should_forward(SvcLayer::new(0, 0)));
    println!("  S1T1 forwarded: {}", svc_selector.should_forward(SvcLayer::new(1, 1)));

    println!("\n=== Quality Scaler ===");
    let scaler = QualityScaler::new(QualityScalerConfig {
        min_frames_before_scale: 5,
        high_qp_threshold: 37,
        low_qp_threshold: 20,
        ..Default::default()
    }, 1280, 720);

    let (w, h) = scaler.current_resolution();
    println!("Initial resolution: {}x{}", w, h);

    println!("\nSimulating high QP (congestion):");
    for _ in 0..5 {
        let decision = scaler.report_qp(50);
        let (w, h) = scaler.current_resolution();
        println!("  QP=50 -> {:?}, resolution={}x{}", decision, w, h);
    }

    println!("\nSimulating low QP (recovery):");
    for _ in 0..5 {
        let decision = scaler.report_qp(10);
        let (w, h) = scaler.current_resolution();
        println!("  QP=10 -> {:?}, resolution={}x{}", decision, w, h);
    }

    println!("\nScaler stats:");
    println!("  Average QP: {}", scaler.average_qp());
    println!("  Scale downs: {}", scaler.scale_down_count());
    println!("  Scale ups: {}", scaler.scale_up_count());

    println!("\n=== Video Example Complete ===");
}

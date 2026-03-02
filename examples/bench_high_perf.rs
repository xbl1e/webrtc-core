use std::time::Instant;
use webrtc_core::engine_handle::EngineHandle;

fn main() {
    const NUM_PACKETS: usize = 1_000_000;
    const PACKET_SIZE: usize = 160;
    const SIMULATED_LOSS_RATE: f64 = 0.01;

    let handle = EngineHandle::builder()
        .slab_capacity(8192)
        .index_capacity(8192)
        .build();

    let payload = vec![0u8; PACKET_SIZE];
    let mut latencies = Vec::with_capacity(NUM_PACKETS);

    let start = Instant::now();

    for i in 0..NUM_PACKETS as u32 {
        let seq = i as u16;
        
        let packet_start = Instant::now();
        
        let result = handle.feed_packet(&payload, seq, 0x12345678);
        
        let elapsed = packet_start.elapsed().as_nanos() as u64;
        
        if result.is_ok() {
            if (i as f64) / (NUM_PACKETS as f64) > SIMULATED_LOSS_RATE {
                latencies.push(elapsed);
            }
        }
        
        std::hint::black_box(i);
    }

    let total_time = start.elapsed();
    let total_time_secs = total_time.as_secs_f64();

    if latencies.is_empty() {
        println!("No packets processed successfully");
        return;
    }

    latencies.sort_unstable();
    
    let total_latency: u64 = latencies.iter().sum();
    let avg_latency = total_latency / latencies.len() as u64;
    let max_latency = *latencies.last().unwrap();
    
    let p99_idx = ((latencies.len() as f64) * 0.99).ceil() as usize - 1;
    let p99 = latencies[p99_idx.min(latencies.len() - 1)];
    
    let p999_idx = ((latencies.len() as f64) * 0.999).ceil() as usize - 1;
    let p999 = latencies[p999_idx.min(latencies.len() - 1)];
    
    let processed = latencies.len();
    let loss_packets = NUM_PACKETS - processed;
    let loss_percent = (loss_packets as f64 / NUM_PACKETS as f64) * 100.0;
    
    let pps = (NUM_PACKETS as f64 / total_time_secs) as u64;
    
    println!(
        "PPS: {} | AVG: {}us | P99: {}us | P99.9: {}us | MAX: {}us | LOSS: {:.1}%",
        pps,
        avg_latency / 1000,
        p99 / 1000,
        p999 / 1000,
        max_latency / 1000,
        loss_percent
    );
}

use std::sync::Arc;
use std::time::Instant;
use webrtc_core::EngineHandle;

fn main() {
    println!("=== webrtc-core High Performance Benchmark ===\n");

    let handle = EngineHandle::builder()
        .slab_capacity(4096)
        .index_capacity(4096)
        .jitter_capacity(1024)
        .build();

    let slab = handle.slab.clone();

    benchmark_packet_throughput(&handle, &slab);
    benchmark_latency_distribution(&handle, &slab);
    benchmark_ring_buffer_performance(&handle);

    println!("\n=== Benchmark completed ===");
}

fn benchmark_packet_throughput(handle: &EngineHandle, slab: &Arc<webrtc_core::SlabAllocator>) {
    println!("[1] Packet Throughput Benchmark");
    
    let iterations = 100_000usize;
    let payload = vec![0xABu8; 1200];
    
    let start = Instant::now();
    
    for i in 0..iterations {
        let seq = (i % 65536) as u16;
        let ssrc = 0x12345678u32;
        
        if let Some(guard) = webrtc_core::SlabAllocator::allocate_guard(slab) {
            unsafe {
                let p = guard.get_mut();
                let len = payload.len().min(p.data.len());
                p.data[..len].copy_from_slice(&payload[..len]);
                p.len = len;
                p.seq = seq;
                p.timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64;
                p.ssrc = ssrc;
            }
            let idx = guard.into_index();
            if handle.idx_ring.push(idx) {
                // Successfully queued
            } else {
                slab.free(idx);
            }
        }
    }
    
    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    let throughput_mpps = iterations as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Avg time: {:.1} ns/packet", avg_ns);
    println!("  Throughput: {:.2} Mpps", throughput_mpps);
    if avg_ns < 1000.0 {
        println!("  Sub-microsecond: YES");
    }
    println!("  OK\n");
}

fn benchmark_latency_distribution(handle: &EngineHandle, slab: &Arc<webrtc_core::SlabAllocator>) {
    println!("[2] Latency Distribution Benchmark");
    
    let samples = 10_000usize;
    let payload = vec![0xCDu8; 160];
    let mut latencies: Vec<u64> = Vec::with_capacity(samples);
    
    for i in 0..samples {
        let seq = (i % 65536) as u16;
        let ssrc = 0xDEADBEEFu32;
        
        let start = Instant::now();
        
        if let Some(guard) = webrtc_core::SlabAllocator::allocate_guard(slab) {
            unsafe {
                let p = guard.get_mut();
                let len = payload.len().min(p.data.len());
                p.data[..len].copy_from_slice(&payload[..len]);
                p.len = len;
                p.seq = seq;
                p.timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64;
                p.ssrc = ssrc;
            }
            let idx = guard.into_index();
            if !handle.idx_ring.push(idx) {
                slab.free(idx);
            }
        }
        
        let elapsed = start.elapsed().as_nanos() as u64;
        latencies.push(elapsed);
    }
    
    latencies.sort_unstable();
    
    let p50 = latencies[latencies.len() / 2];
    let p99_idx = ((latencies.len() as f64) * 0.99).ceil() as usize;
    let p99 = latencies[p99_idx.saturating_sub(1).min(latencies.len() - 1)];
    let p999_idx = ((latencies.len() as f64) * 0.999).ceil() as usize;
    let p999 = latencies[p999_idx.saturating_sub(1).min(latencies.len() - 1)];
    
    println!("  Samples: {}", samples);
    println!("  P50: {} ns", p50);
    println!("  P99: {} ns", p99);
    println!("  P99.9: {} ns", p999);
    println!("  OK\n");
}

fn benchmark_ring_buffer_performance(handle: &EngineHandle) {
    println!("[3] Ring Buffer Performance");
    
    let iterations = 1_000_000usize;
    let start = Instant::now();
    
    for i in 0..iterations {
        let idx = i % 4096;
        if handle.idx_ring.push(idx) {
            // Successfully pushed
        }
    }
    
    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();
    
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Avg time: {:.1} ns/op", avg_ns);
    println!("  Ops/sec: {:.2} M", ops_per_sec / 1_000_000.0);
    println!("  OK\n");
}

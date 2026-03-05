use criterion::{black_box, criterion_group, criterion_main, Criterion};
use webrtc_core::{LatencyRing, SlabAllocator};
use std::time::{Duration, Instant};

fn bench_slab_get_mut(c: &mut Criterion) {
    let slab = SlabAllocator::new(8192);
    let key = slab.allocate().unwrap();

    c.bench_function("slab_get_mut", |b| {
        b.iter(|| {
            let _ = black_box(slab.get_mut(&key));
        })
    });
}

fn bench_latency_ring_push(c: &mut Criterion) {
    let ring = LatencyRing::new(8192);

    c.bench_function("latency_ring_push", |b| {
        b.iter(|| {
            let _ = black_box(ring.push(black_box(1000)));
        })
    });
}

fn bench_latency_ring_pop_batch(c: &mut Criterion) {
    let ring = LatencyRing::new(8192);
    let mut buf = vec![0u64; 1024];

    c.bench_function("latency_ring_pop_batch", |b| {
        b.iter(|| {
            let _ = black_box(ring.pop_batch(black_box(&mut buf)));
        })
    });
}

fn bench_latency_ring_p99_calculation(c: &mut Criterion) {
    let ring = LatencyRing::new(8192);
    let mut buf = vec![0u64; 8192];

    c.bench_function("latency_ring_p99_calculation", |b| {
        b.iter(|| {
            let n = ring.pop_batch(&mut buf);
            if n > 0 {
                buf.truncate(n);
                buf.sort_unstable();
                let idx = ((n as f64) * 0.99).ceil() as usize;
                let ix = idx.saturating_sub(1).min(n.saturating_sub(1));
                let _ = black_box(buf[ix]);
            }
        })
    });
}

fn bench_realistic_latency_scenario(c: &mut Criterion) {
    c.bench_function("realistic_latency_scenario", |b| {
        b.iter(|| {
            let ring = LatencyRing::new(8192);
            let mut buf = vec![0u64; 1024];

            for i in 0..1000 {
                ring.push(i * 1000);
            }

            let n = ring.pop_batch(&mut buf);
            if n > 0 {
                buf.truncate(n);
                buf.sort_unstable();
                let idx = ((n as f64) * 0.99).ceil() as usize;
                let ix = idx.saturating_sub(1).min(n.saturating_sub(1));
                let p99 = black_box(buf[ix]);
            }
        })
    });
}

criterion_group!(
    benches,
    bench_slab_get_mut,
    bench_latency_ring_push,
    bench_latency_ring_pop_batch,
    bench_latency_ring_p99_calculation,
    bench_realistic_latency_scenario,
);

criterion_main!(benches);

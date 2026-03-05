use criterion::{black_box, criterion_group, criterion_main, Criterion};
use webrtc_core::{ByteRing, IndexRing, LatencyRing};
use std::sync::Arc;
use std::thread;

fn bench_byte_ring_spsc(c: &mut Criterion) {
    let ring = Arc::new(ByteRing::new(1024));
    let producer_ring = ring.clone();

    let producer = thread::spawn(move || {
        for i in 0..10000u8 {
            while !producer_ring.try_write(i) {
                thread::yield_now();
            }
        }
    });

    c.bench_function("byte_ring_spsc_consumer", |b| {
        let mut buf = [0u8; 256];
        b.iter(|| {
            let _ = black_box(ring.read(&mut buf));
        })
    });

    producer.join().ok();
}

fn bench_index_ring_spsc(c: &mut Criterion) {
    let ring = Arc::new(IndexRing::new(1024));
    let producer_ring = ring.clone();

    let producer = thread::spawn(move || {
        for i in 0..10000usize {
            while !producer_ring.push(i) {
                thread::yield_now();
            }
        }
    });

    c.bench_function("index_ring_spsc_consumer", |b| {
        b.iter(|| {
            let _ = black_box(ring.pop());
        })
    });

    producer.join().ok();
}

fn bench_latency_ring_spsc(c: &mut Criterion) {
    let ring = Arc::new(LatencyRing::new(1024));
    let producer_ring = ring.clone();

    let producer = thread::spawn(move || {
        for i in 0..10000u64 {
            while !producer_ring.push(i) {
                thread::yield_now();
            }
        }
    });

    let mut buf = vec![0u64; 256];
    c.bench_function("latency_ring_spsc_consumer", |b| {
        b.iter(|| {
            let _ = black_box(ring.pop_batch(&mut buf));
        })
    });

    producer.join().ok();
}

fn bench_concurrent_slab_allocation(c: &mut Criterion) {
    let slab = Arc::new(webrtc_core::SlabAllocator::new(1024));

    let slab_a = slab.clone();
    let slab_b = slab.clone();
    let slab_c = slab.clone();
    let slab_d = slab.clone();

    c.bench_function("concurrent_slab_alloc", |b| {
        b.iter(|| {
            let _ = black_box(slab_a.allocate());
            let _ = black_box(slab_b.allocate());
            let _ = black_box(slab_c.allocate());
            let _ = black_box(slab_d.allocate());
        })
    });
}

criterion_group!(
    benches,
    bench_byte_ring_spsc,
    bench_index_ring_spsc,
    bench_latency_ring_spsc,
    bench_concurrent_slab_allocation,
);

criterion_main!(benches);

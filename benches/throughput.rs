use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use webrtc_core::{ByteRing, IndexRing, SlabAllocator, RtcpSendQueue, LatencyRing};
use std::sync::Arc;

fn bench_slab_allocate(c: &mut Criterion) {
    let slab = Arc::new(SlabAllocator::new(8192));

    c.bench_function("slab_allocate", |b| {
        b.iter(|| {
            let _key = black_box(slab.allocate());
        })
    });
}

fn bench_slab_allocate_free(c: &mut Criterion) {
    let slab = Arc::new(SlabAllocator::new(8192));
    let mut keys = Vec::new();

    c.bench_function("slab_allocate_free", |b| {
        b.iter(|| {
            if let Some(key) = slab.allocate() {
                keys.push(key);
            }
            if keys.len() > 1000 {
                if let Some(key) = keys.pop() {
                    slab.free(key);
                }
            }
        })
    });
}

fn bench_byte_ring_write(c: &mut Criterion) {
    let ring = ByteRing::new(4096);
    let data = vec![0u8; 1500];

    c.bench_function("byte_ring_write", |b| {
        b.iter(|| {
            let _ = black_box(ring.write(black_box(&data)));
        })
    });
}

fn bench_byte_ring_write_read(c: &mut Criterion) {
    let ring = ByteRing::new(4096);
    let write_data = vec![0u8; 1500];
    let mut read_buf = [0u8; 1500];

    c.bench_function("byte_ring_write_read", |b| {
        b.iter(|| {
            ring.write(black_box(&write_data));
            let _ = black_box(ring.read(&mut read_buf));
        })
    });
}

fn bench_index_ring_push(c: &mut Criterion) {
    let ring = IndexRing::new(4096);

    c.bench_function("index_ring_push", |b| {
        b.iter(|| {
            let _ = black_box(ring.push(black_box(1234)));
        })
    });
}

fn bench_index_ring_push_pop(c: &mut Criterion) {
    let ring = IndexRing::new(4096);

    c.bench_function("index_ring_push_pop", |b| {
        b.iter(|| {
            ring.push(black_box(1234));
            let _ = black_box(ring.pop());
        })
    });
}

fn bench_rtcp_queue_push(c: &mut Criterion) {
    let queue = RtcpSendQueue::new(256);
    let data = vec![0u8; 100];

    c.bench_function("rtcp_queue_push", |b| {
        b.iter(|| {
            let _ = black_box(queue.push_drop_oldest(black_box(&data)));
        })
    });
}

fn bench_rtcp_queue_push_pop(c: &mut Criterion) {
    let queue = RtcpSendQueue::new(256);
    let write_data = vec![0u8; 100];
    let mut read_buf = [0u8; 512];

    c.bench_function("rtcp_queue_push_pop", |b| {
        b.iter(|| {
            queue.push_drop_oldest(black_box(&write_data));
            let _ = black_box(queue.pop(&mut read_buf));
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

fn bench_latency_ring_push_pop_batch(c: &mut Criterion) {
    let ring = LatencyRing::new(8192);
    let mut buf = vec![0u64; 1024];

    c.bench_function("latency_ring_push_pop_batch", |b| {
        b.iter(|| {
            for _ in 0..100 {
                ring.push(black_box(1000));
            }
            let _ = black_box(ring.pop_batch(&mut buf));
        })
    });
}

fn bench_engine_feed_packet(c: &mut Criterion) {
    let handle = webrtc_core::EngineHandle::builder()
        .slab_capacity(8192)
        .index_capacity(8192)
        .build();
    let payload = vec![0u8; 160];

    c.bench_function("engine_feed_packet", |b| {
        b.iter(|| {
            let _ = black_box(handle.feed_packet(black_box(&payload), 1, 0x1234));
        })
    });

    handle.shutdown();
}

fn bench_engine_feed_packet_throughput(c: &mut Criterion) {
    let handle = webrtc_core::EngineHandle::builder()
        .slab_capacity(8192)
        .index_capacity(8192)
        .build();
    let payload = vec![0u8; 160];

    let mut group = c.benchmark_group("engine_throughput");
    group.throughput(Throughput::Bytes(payload.len() as u64));

    group.bench_function("engine_feed_packet_160_bytes", |b| {
        b.iter(|| {
            let _ = black_box(handle.feed_packet(black_box(&payload), 1, 0x1234));
        })
    });

    group.finish();

    handle.shutdown();
}

criterion_group!(
    benches,
    bench_slab_allocate,
    bench_slab_allocate_free,
    bench_byte_ring_write,
    bench_byte_ring_write_read,
    bench_index_ring_push,
    bench_index_ring_push_pop,
    bench_rtcp_queue_push,
    bench_rtcp_queue_push_pop,
    bench_latency_ring_push,
    bench_latency_ring_push_pop_batch,
    bench_engine_feed_packet,
    bench_engine_feed_packet_throughput,
);

criterion_main!(benches);

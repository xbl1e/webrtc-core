# Performance Benchmarking Guide

This document describes how to run and interpret performance benchmarks for webrtc-core.

## Prerequisites

### Required Tools

```bash
cargo install cargo-criterion

cargo install cargo-flamegraph
```

### Optional Tools

For Linux performance profiling:
```bash
sudo apt-get install linux-perf
```

For Valgrind memory profiling:
```bash
sudo apt-get install valgrind
```

## Running Benchmarks

### All Benchmarks

```bash
cargo bench --all-features
```

### Specific Benchmark

```bash
cargo bench --bench throughput
cargo bench --bench latency
cargo bench --bench concurrency
```

### Save Benchmark Results

```bash
cargo bench -- --save-baseline main
cargo bench -- --baseline main
```

## Benchmark Descriptions

### throughput.rs

Tests raw throughput of core data structures:

- `slab_allocate` - Allocation performance
- `slab_allocate_free` - Allocation + free cycle
- `byte_ring_write` - ByteRing write throughput
- `byte_ring_write_read` - ByteRing write+read cycle
- `index_ring_push` - IndexRing push performance
- `index_ring_push_pop` - IndexRing push+pop cycle
- `rtcp_queue_push` - RtcpQueue write performance
- `rtcp_queue_push_pop` - RtcpQueue write+read cycle
- `latency_ring_push` - LatencyRing write performance
- `latency_ring_push_pop_batch` - Batch read performance
- `engine_feed_packet` - Full engine packet feed
- `engine_feed_packet_throughput` - Engine throughput measurement

### latency.rs

Tests latency-critical operations:

- `slab_get_mut` - Slab lookup latency
- `latency_ring_push` - LatencyRing write latency
- `latency_ring_pop_batch` - Batch read latency
- `latency_ring_p99_calculation` - P99 calculation overhead
- `realistic_latency_scenario` - Realistic workload simulation

### concurrency.rs

Tests concurrent access patterns:

- `byte_ring_spsc` - SPSC producer-consumer pattern
- `index_ring_spsc` - SPSC producer-consumer pattern
- `latency_ring_spsc` - SPSC producer-consumer pattern
- `concurrent_slab_alloc` - Multi-threaded allocation

## Profiling

### Flamegraph (Linux/macOS)

```bash
./scripts/profile_flamegraph.sh
```

This generates `flamegraph.svg` showing hot spots.

### Perf (Linux)

```bash
./scripts/profile_perf.sh
```

This shows CPU cycles and instruction counts.

## Interpreting Results

### Throughput Benchmarks

Measured in operations per second or bytes per second.

Higher is better.

### Latency Benchmarks

Measured in nanoseconds per operation.

Lower is better.

Target for P99 latency: < 10 microseconds

### Concurrency Benchmarks

Shows scaling with multiple threads.

Linear or near-linear scaling is ideal.

## Known Limitations

- Benchmarks are isolated measurements, not real-world scenarios
- Cache effects may differ in production
- Network I/O not included in measurements
- Real codec encoding/decoding not tested

## Benchmarking Best Practices

1. Use release builds: `cargo bench` uses release profile
2. Run multiple times for consistency
3. Pin CPU cores for stable results
4. Disable turboboost for consistent clock speeds
5. Close other applications for stable results

## Reproducing Benchmarks

To reproduce benchmark results:

1. Use same or similar hardware
2. Same Rust version: `rustc --version`
3. Same OS and version
4. Run multiple iterations
5. Use same benchmark flags

## Contributing Benchmarks

When adding benchmarks:

1. Follow existing patterns
2. Use `black_box()` to prevent compiler optimization
3. Document what is being measured
4. Include both throughput and latency variants
5. Add comments explaining significance

## Performance Goals

- Slab allocation: < 100ns
- Ring buffer operations: < 50ns
- Engine feed_packet: < 1µs
- P99 latency: < 10µs
- Throughput: > 1M packets/second/core

Note: These are isolated benchmark targets. Real-world performance will vary.

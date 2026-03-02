# webrtc-core


[![crates.io](https://img.shields.io/crates/v/webrtc-core.svg)](https://crates.io/crates/webrtc-core) [![docs.rs](https://img.shields.io/docsrs/webrtc-core)](https://docs.rs/webrtc-core) [![status](https://img.shields.io/badge/status-production-brightgreen.svg)](https://github.com) [![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Production-ready, low-latency core for realtime audio pipelines: slab-based packet buffers, jitter handling, SRTP (AES‑GCM) protection, and RTCP feedback (NACK / TWCC). Designed for high-throughput, low-footprint deployments.

[1] Why this crate

- Purpose-built API for high-throughput, low-latency production systems (VoIP, conferencing, and realtime media services).
- Highly concurrent, low-allocation primitives: slab allocator, index/byte rings, jitter buffer and RTCP queue — engineered to minimize pause and allocation jitter.
- In-place SRTP (AES‑GCM) protection to avoid extra copies on the hot path.

[2] Enterprise readiness

- Deterministic memory usage via preallocated slabs and ring buffers.
- Thread-safe, cache-padded atomics and minimal locking for scalable multi-threaded ingestion.
- Small runtime and dependency surface to ease embeddability and auditability.
- Designed for easy horizontal scaling: stateless ingest + small per-worker footprint.

**Design philosophy:** optimize the real-time audio fast path while keeping memory bounded and latency predictable.

[3] Quickstart

```bash
cargo add webrtc-core
```

Minimal usage

```rust
use webrtc_core::EngineHandle;

let handle = EngineHandle::builder().build();
let payload = vec![0u8; 160];
handle.feed_packet(&payload, 1u16, 0x1234).ok();
```

Provide SRTP key (in-place protection for future packets)

```rust
let key = [0u8; 32];
handle.provide_keying_material(key);
```

RTCP sender (async example)

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use webrtc_core::EngineHandle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr: SocketAddr = "127.0.0.1:9000".parse()?;
    let peer: SocketAddr = "127.0.0.1:9001".parse()?;
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    let handle = EngineHandle::builder().build();
    let rtcp_task = handle.start_rtcp_sender(socket.clone(), peer);

    let payload = vec![0u8; 160];
    let _ = handle.feed_packet(&payload, 42u16, 0x1234);

    let _ = rtcp_task.await;
    Ok(())
}
```

[4] Core API

- `EngineHandle` — ergonomic entry point: builder, `feed_packet`, `provide_keying_material`, `start_rtcp_sender`, `shutdown`.
- `MediaEngine` — internal processing loop: jitter handling, packet processing, RTCP emission.
- `SrtpContext` — AES‑GCM SRTP helpers (in-place protect/unprotect).
- `SlabAllocator`, `IndexRing`, `AudioJitterBuffer`, `RtcpSendQueue` — low-level primitives for high-throughput audio.

[5] Notes

- Emphasizes predictable latency over generality; uses preallocated slabs and ring buffers to avoid allocations in the hot path.
- RTCP generation is conservative and designed to be driven by the jitter buffer's gap detection.

[6] Performance

Measured snapshot (this environment)

- Command run: `cargo run --release --example bench_micro`
- Observed (release build on this Windows dev machine):
    - avg protect_inplace: 31 ns
    - implied theoretical protects/sec: ~32M (1_000_000_000 / 31)

[7] Profiling tips

- Build with `--release` and use `perf` (Linux) or Windows Performance Analyzer for hotspots.
- For host-optimized results set `RUSTFLAGS='-C target-cpu=native'` when building.

[8] Example behaviors

- `examples/bench_micro.rs` prints micro-benchmark metrics (see above).
- `examples/discord_clone.rs` is a simple UDP+RTCP demo and starts a long-running RTCP sender loop - it will run until interrupted.

[9] Credits

- xbl1e - creator and maintainer

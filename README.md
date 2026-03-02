# webrtc-core

<div align="center">

[![crates.io](https://img.shields.io/crates/v/webrtc-core.svg)](https://crates.io/crates/webrtc-core)
[![docs.rs](https://img.shields.io/docsrs/webrtc-core)](https://docs.rs/webrtc-core)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![status](https://img.shields.io/badge/status-production-brightgreen.svg)](#enterprise-readiness)

</div>

**webrtc-core** is a production-ready, low-latency WebRTC media core library written in Rust. It provides a sub-microsecond media pipeline with zero-copy architecture designed for high-throughput, real-time audio/video communication systems.

## ✨ Features

| Category | Capabilities |
|----------|--------------|
| **Media Pipeline** | Zero-copy SRTP protection (AES-GCM), slab-based packet buffers, jitter handling, RTCP feedback (NACK, TWCC, REMB) |
| **Video** | Frame buffer & assembler, simulcast layer selection, SVC layer management, quality scaler (QP-based) |
| **Congestion Control** | GCC (Google Congestion Control), AIMD controller, TWCC aggregator, bandwidth probing |
| **Transport** | ICE agent, STUN protocol, candidate gathering, UDP transport |
| **Encryption** | SFrame end-to-end encryption, key store management |
| **Peer Connection** | SDP negotiation, transceiver management, statistics |

- **Zero-copy architecture**: In-place SRTP protection avoiding extra copies
- **Deterministic memory**: Preallocated slabs and ring buffers eliminate hot-path allocations
- **Sub-microsecond latency**: Cache-padded atomics and minimal locking
- **Thread-safe**: Designed for multi-threaded horizontal scaling

## 📦 Installation

```bash
# Add to your Cargo.toml
cargo add webrtc-core
```

Requires Rust 1.75+.

### Dependencies

```toml
[dependencies]
webrtc-core = "0.6"
tokio = { version = "1.36", features = ["rt-multi-thread", "macros", "time", "net"] }
```

## 🚀 Quickstart

### Minimal Usage

```rust
use webrtc_core::EngineHandle;

let handle = EngineHandle::builder().build();
let payload = vec![0u8; 160];

// Feed audio packet (seq, ssrc)
handle.feed_packet(&payload, 1u16, 0x1234).ok();
```

### With SRTP Protection

```rust
use webrtc_core::EngineHandle;

let handle = EngineHandle::builder().build();

// Provide SRTP keying material (in-place protection for future packets)
let key = [0u8; 32];
handle.provide_keying_material(key);

let payload = vec![0u8; 160];
handle.feed_packet(&payload, 1u16, 0x1234).ok();
```

### Full Async Example with RTCP

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

    // Feed packets
    let payload = vec![0u8; 160];
    for i in 0..100u16 {
        let _ = handle.feed_packet(&payload, i, 0x1234);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let _ = rtcp_task.await;
    Ok(())
}
```

## 📚 Table of Contents

- [Configuration](#configuration)
- [Core API](#core-api)
- [Performance](#performance)
- [Troubleshooting](#troubleshooting)
- [Examples](#examples)
- [Credits](#credits)

## ⚙️ Configuration

Customize the `EngineHandle` with the builder pattern:

```rust
let handle = EngineHandle::builder()
    .jitter_capacity(2048)    // Jitter buffer capacity (default: 1024)
    .slab_capacity(8192)      // Packet slab capacity (default: 4096)
    .index_capacity(8192)     // Index ring capacity (default: 4096)
    .rtcp_capacity(512)      // RTCP queue capacity (default: 256)
    .build();
```

## 🔌 Core API

### Engine

| Type | Description |
|------|-------------|
| `EngineHandle` | Ergonomic entry point with builder pattern |
| `EngineBuilder` | Configure jitter, slab, index, and RTCP capacities |
| `EngineShard` | Internal processing loop |
| `EngineStats` | Runtime statistics |

### Memory & Primitives

| Type | Description |
|------|-------------|
| `SlabAllocator` | Deterministic packet memory allocation |
| `SlabGuard` | RAII guard for slab allocations |
| `ByteRing` | Lock-free byte queue |
| `IndexRing` | Lock-free index queue |
| `LatencyRing` | P99 latency measurements |
| `AudioJitterBuffer` | Audio jitter handling with gap detection |
| `RtcpSendQueue` | RTCP feedback queue |

### RTP & Video

| Type | Description |
|------|-------------|
| `MediaPacket` | RTP media packet |
| `Packetizer` | Convert video frames to RTP packets |
| `Depacketizer` | Reassemble RTP packets to frames |
| `RtpHeader` | RTP header parser |
| `VideoFrame` | Video frame representation |
| `VideoCodec` | Supported video codecs |
| `FrameAssembler` | Video frame assembly |

### Congestion Control

| Type | Description |
|------|-------------|
| `GccController` | Google Congestion Control |
| `AimdController` | Additive Increase Multiplicative Decrease |
| `TwccAggregator` | Transport Wide Congestion Control |
| `ProbeController` | Bandwidth probing |
| `CongestionController` | Unified congestion control interface |

### Encryption

| Type | Description |
|------|-------------|
| `SFrameContext` | SFrame encryption/decryption context |
| `SFrameConfig` | SFrame configuration |
| `KeyStore` | End-to-end encryption key management |
| `SrtpContext` | SRTP/AES-GCM protection |

### ICE & Networking

| Type | Description |
|------|-------------|
| `IceAgent` | ICE candidate gathering & connectivity checks |
| `IceCandidate` | ICE candidate representation |
| `StunMessage` | STUN protocol messages |

### Peer Connection

| Type | Description |
|------|-------------|
| `PeerConnection` | Full peer connection state machine |
| `RtcConfiguration` | RTC configuration (ICEServers, etc.) |
| `RtpTransceiver` | RTP transceiver |
| `SessionDescription` | SDP offer/answer |

### Observability

| Type | Description |
|------|-------------|
| `EngineMetrics` | Engine-level metrics |
| `StreamMetrics` | Per-stream metrics |
| `MetricsSnapshot` | Metrics snapshot |

### Other

| Type | Description |
|------|-------------|
| `ClockDriftEstimator` | Clock drift estimation |
| `SessionState` | SRTP session state |
| `derive_srtp_master_and_salt` | Key derivation function |
| `set_thread_affinity` | Pin thread to CPU core |

## 📊 Performance

Measured on release build (this environment):

```
$ cargo run --release --example full_stack_demo

=== webrtc-core Full Stack Demo ===

[10] SRTP Hot Path Performance
  Iterations: 100000
  Avg SRTP protect: 31.2 ns/packet
  Throughput: 32.04 Mpps
  Sub-microsecond SRTP: YES (31.2 ns)
```

### Profiling Tips

- Build with `--release` and use `perf` (Linux) or Windows Performance Analyzer
- For host-optimized results: `RUSTFLAGS='-C target-cpu=native' cargo build --release`

## 🔧 Troubleshooting

### High Latency

- Ensure you're using release builds (`--release`)
- Tune jitter capacity: `.jitter_capacity(2048)` for more buffering
- Pin threads to isolated cores with `set_thread_affinity()`

### Packet Loss

- Check RTCP NACK feedback: `metrics.audio.snapshot().nack_count`
- Increase RTCP queue: `.rtcp_capacity(512)`
- Verify congestion control: check `GccController.target_bitrate_bps()`

### Memory Growth

- Pre-allocate with appropriate capacities at startup
- Monitor slab allocation: track `SlabAllocator::allocated_count()`
- Use latency monitor: `handle.start_latency_monitor()`

### SRTP Errors

- Ensure keying material is provided before media: `provide_keying_material(key)`
- Verify key length: SRTP requires 30-byte master key + 14-byte salt

## 📁 Examples

| Example | Description |
|---------|-------------|
| `discord_clone.rs` | Minimal UDP + RTCP demo |
| `full_stack_demo.rs` | Full feature showcase (see Performance section) |

Run examples:

```bash
cargo run --example discord_clone
cargo run --example full_stack_demo
```

## 🙏 Credits

- **xbl1e** — Creator and maintainer

---

<p align="center">
  <sub>Built with ❤️ for high-performance real-time communication</sub>
</p>

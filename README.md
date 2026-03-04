# webrtc-core

<div align="left">

[![crates.io](https://img.shields.io/crates/v/webrtc-core.svg)](https://crates.io/crates/webrtc-core)
[![docs.rs](https://img.shields.io/docsrs/webrtc-core)](https://docs.rs/webrtc-core)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![status](https://img.shields.io/badge/status-production-brightgreen.svg)](#enterprise-readiness)

</div>

**webrtc-core v0.7.0** is a production-ready, low-latency WebRTC media core library written in Rust. It provides a sub-microsecond media pipeline with zero-copy architecture designed for high-throughput, real-time audio/video communication systems. Suitable for enterprise deployments as a replacement for libwebrtc.

## ✨ Features

| Category | Capabilities |
|----------|--------------|
| **Media Pipeline** | Zero-copy SRTP protection (AES-GCM/AES-256-GCM), slab-based packet buffers, jitter handling, RTCP feedback (NACK, TWCC, REMB) |
| **Video** | Frame buffer & assembler, simulcast layer selection, SVC layer management, quality scaler (QP-based), RTX support |
| **Audio** | Audio processing pipeline (AEC3, NS, AGC), capture & render abstraction |
| **Congestion Control** | GCC (Google Congestion Control), AIMD controller, TWCC aggregator, bandwidth probing |
| **Transport** | ICE agent, STUN protocol, TURN client (RFC 5766), candidate gathering, connectivity checks, UDP transport |
| **Encryption** | DTLS 1.2/1.3 handshake, SFrame end-to-end encryption, key store management |
| **Data Transport** | SCTP over DTLS, DataChannels (W3C compliant) |
| **Codecs** | FFI support for Opus, VP8, VP9, H.264, AV1 |
| **Peer Connection** | SDP negotiation, transceiver management, statistics, FFI bindings for C/C++ |

- **Zero-copy architecture**: In-place SRTP protection avoiding extra copies
- **Deterministic memory**: Preallocated slabs and ring buffers eliminate hot-path allocations
- **Sub-microsecond latency**: Cache-padded atomics and minimal locking
- **Thread-safe**: Designed for multi-threaded horizontal scaling
- **Enterprise-ready**: Production-viable for companies like Discord, Google, Amazon

## 📦 Installation

```bash
cargo add webrtc-core
```

Requires Rust 1.75+.

### Dependencies

```toml
[dependencies]
webrtc-core = "0.7"
tokio = { version = "1.36", features = ["rt-multi-thread", "macros", "time", "net", "sync", "rt"] }
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

[1] Engine

| Type | Description |
|------|-------------|
| `EngineHandle` | Ergonomic entry point with builder pattern |
| `EngineBuilder` | Configure jitter, slab, index, and RTCP capacities |
| `EngineShard` | Internal processing loop |
| `EngineStats` | Runtime statistics |

[2] Memory & Primitives

| Type | Description |
|------|-------------|
| `SlabAllocator` | Deterministic packet memory allocation |
| `SlabGuard` | RAII guard for slab allocations |
| `ByteRing` | Lock-free byte queue |
| `IndexRing` | Lock-free index queue |
| `LatencyRing` | P99 latency measurements |
| `AudioJitterBuffer` | Audio jitter handling with gap detection |
| `RtcpSendQueue` | RTCP feedback queue |

[3] RTP & Video

| Type | Description |
|------|-------------|
| `MediaPacket` | RTP media packet |
| `Packetizer` | Convert video frames to RTP packets |
| `Depacketizer` | Reassemble RTP packets to frames |
| `RtpHeader` | RTP header parser |
| `VideoFrame` | Video frame representation |
| `VideoCodec` | Supported video codecs |
| `FrameAssembler` | Video frame assembly |

[4] Congestion Control

| Type | Description |
|------|-------------|
| `GccController` | Google Congestion Control |
| `AimdController` | Additive Increase Multiplicative Decrease |
| `TwccAggregator` | Transport Wide Congestion Control |
| `ProbeController` | Bandwidth probing |
| `CongestionController` | Unified congestion control interface |

[5] Encryption

| Type | Description |
|------|-------------|
| `SFrameContext` | SFrame encryption/decryption context |
| `SFrameConfig` | SFrame configuration |
| `KeyStore` | End-to-end encryption key management |
| `SrtpContext` | SRTP/AES-GCM protection |

[6] ICE & Networking

| Type | Description |
|------|-------------|
| `IceAgent` | ICE candidate gathering & connectivity checks |
| `IceCandidate` | ICE candidate representation |
| `StunMessage` | STUN protocol messages |

[7] Peer Connection

| Type | Description |
|------|-------------|
| `PeerConnection` | Full peer connection state machine |
| `RtcConfiguration` | RTC configuration (ICEServers, etc.) |
| `RtpTransceiver` | RTP transceiver |
| `SessionDescription` | SDP offer/answer |

[8] Observability

| Type | Description |
|------|-------------|
| `EngineMetrics` | Engine-level metrics |
| `StreamMetrics` | Per-stream metrics |
| `MetricsSnapshot` | Metrics snapshot |

[9] Other

| Type | Description |
|------|-------------|
| `ClockDriftEstimator` | Clock drift estimation |
| `SessionState` | SRTP session state |
| `derive_srtp_master_and_salt` | Key derivation function |
| `set_thread_affinity` | Pin thread to CPU core |

[10] TURN & NAT Traversal

| Type | Description |
|------|-------------|
| `TurnClient` | TURN client for relay candidates |
| `TurnClientPool` | Pool of TURN clients |
| `TurnAllocation` | TURN allocation state |

[11] DTLS & Handshake

| Type | Description |
|------|-------------|
| `DtlsHandshake` | DTLS handshake state machine |
| `DtlsEndpoint` | DTLS endpoint for encryption |
| `DtlsCipherSuite` | DTLS cipher suite configuration |

[12] SCTP & DataChannels

| Type | Description |
|------|-------------|
| `SctpTransport` | SCTP transport over DTLS |
| `SctpAssociation` | SCTP association |
| `SctpStream` | SCTP stream |
| `DataChannel` | WebRTC DataChannel |
| `DataChannelManager` | DataChannel manager |

[13] Codecs

| Type | Description |
|------|-------------|
| `VideoEncoder` | Video encoder trait |
| `VideoDecoder` | Video decoder trait |
| `AudioEncoder` | Audio encoder trait |
| `AudioDecoder` | Audio decoder trait |
| `CodecRegistry` | Codec registry |

[14] Audio Processing

| Type | Description |
|------|-------------|
| `AudioProcessingPipeline` | Audio processing (AEC, NS, AGC) |
| `AudioProcessingConfig` | Audio processing configuration |
| `AudioFrame` | Audio frame representation |

[15] FFI Bindings

| Type | Description |
|------|-------------|
| C API | C-compatible API for webrtc-core |

### 📌 Profiling Tips

- Build with `--release` and use `perf` (Linux) or Windows Performance Analyzer
- For host-optimized results: `RUSTFLAGS='-C target-cpu=native' cargo build --release`

## 🔧 Troubleshooting

[1] High Latency

- Ensure you're using release builds (`--release`)
- Tune jitter capacity: `.jitter_capacity(2048)` for more buffering
- Pin threads to isolated cores with `set_thread_affinity()`

[2] Packet Loss

- Check RTCP NACK feedback: `metrics.audio.snapshot().nack_count`
- Increase RTCP queue: `.rtcp_capacity(512)`
- Verify congestion control: check `GccController.target_bitrate_bps()`

[3] Memory Growth

- Pre-allocate with appropriate capacities at startup
- Monitor slab allocation: track `SlabAllocator::allocated_count()`
- Use latency monitor: `handle.start_latency_monitor()`

[4] SRTP Errors

- Ensure keying material is provided before media: `provide_keying_material(key)`
- Verify key length: SRTP requires 30-byte master key + 14-byte salt

### 📜 Credits

- xbl1e - creator and maintainer

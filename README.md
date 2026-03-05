# webrtc-core

<div align="left">

[![crates.io](https://img.shields.io/crates/v/webrtc-core.svg)](https://crates.io/crates/webrtc-core)
[![docs.rs](https://img.shields.io/docsrs/webrtc-core)](https://docs.rs/webrtc-core)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![status](https://img.shields.io/badge/status-beta-yellow.svg)](#enterprise-readiness)

</div>

**webrtc-core** is a beta WebRTC media core library written in Rust. It provides a low-latency media pipeline with zero-copy architecture designed for high-throughput, real-time audio/video communication systems.

## Status: Beta

This is a beta release. The library is functional but may contain bugs and the API may change. Use with caution in production environments and report any issues found.

## Features

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

## Architecture Highlights

- **Zero-copy architecture**: In-place SRTP protection avoiding extra copies
- **Deterministic memory**: Preallocated slabs and ring buffers eliminate hot-path allocations
- **Low latency**: Cache-padded atomics and minimal locking
- **Thread-safe**: Designed for multi-threaded horizontal scaling
- **Memory safety**: Uses Rust's ownership model with SlabKey generation counters to prevent use-after-free

## Installation

```bash
cargo add webrtc-core
```

Requires Rust 1.75+.

### Dependencies

```toml
[dependencies]
webrtc-core = "1.0"
tokio = { version = "1.36", features = ["rt-multi-thread", "macros", "time", "net", "sync", "rt"] }
```

## Quickstart

### Minimal Usage

```rust
use webrtc_core::EngineHandle;

let handle = EngineHandle::builder().build();
let payload = vec![0u8; 160];

handle.feed_packet(&payload, 1u16, 0x1234).ok();
```

### With SRTP Protection

```rust
use webrtc_core::EngineHandle;

let handle = EngineHandle::builder().build();

let key = [0u8; 32];
handle.provide_keying_material(key);

let payload = vec![0u8; 160];
handle.feed_packet(&payload, 1u16, 0x1234).ok();
```

## Configuration

Customize the `EngineHandle` with the builder pattern:

```rust
let handle = EngineHandle::builder()
    .jitter_capacity(2048)
    .slab_capacity(8192)
    .index_capacity(8192)
    .rtcp_capacity(512)
    .build();
```

## Memory Safety

The library uses several mechanisms to ensure memory safety:

1. **SlabKey with generation counters**: Prevents use-after-free in the slab allocator
2. **SPSC/MPSC ring buffers**: Proper memory ordering for lock-free structures
3. **Safe Rust APIs**: All unsafe code is properly documented with invariants

See the documentation for details on safe usage patterns.

## Limitations

- Beta status - API may change
- Limited codec support (mostly via FFI)
- FFI memory management requires careful attention
- No comprehensive fuzzing coverage yet
- Performance benchmarks are isolated measurements, not real-world metrics

## Roadmap to v1.0.0

- [x] Fix critical memory safety bugs
- [x] Improve concurrency safety
- [x] Fix FFI memory leaks
- [x] Document all unsafe code
- [x] Add comprehensive unit tests
- [ ] Add integration tests
- [ ] Add fuzzing coverage
- [ ] Performance benchmarks on real hardware
- [ ] Documentation examples
- [ ] Stability review

## Core API

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
| `SlabKey` | Safe handle for slab allocations with generation counters |
| `SlabGuard` | RAII guard for slab allocations |
| `ByteRing` | Lock-free byte queue (SPSC) |
| `IndexRing` | Lock-free index queue (SPSC) |
| `LatencyRing` | P99 latency measurements (SPSC) |
| `AudioJitterBuffer` | Audio jitter handling with gap detection |
| `RtcpSendQueue` | RTCP feedback queue (MPSC) |

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

## FFI Bindings

See [ffi/README.md](ffi/README.md) for C/C++ API documentation.

## Troubleshooting

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

## Testing

Run tests with:

```bash
cargo test --all-features
```

Run with Miri for memory safety validation:

```bash
cargo +nightly miri test
```

## Credits

- xbl1e - creator and maintainer

## License

MIT

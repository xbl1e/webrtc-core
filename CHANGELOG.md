# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2024-03-05

### Added

- SlabKey with generation counters to prevent use-after-free
- Safe APIs for slab allocator (get_mut, get_mut_unchecked)
- SPSC invariant documentation for ring buffers
- Memory ordering fixes for ByteRing, IndexRing, LatencyRing
- Lock protection for TwccAggregator ArrivalSlot
- Mutex for RtcpSendQueue write serialization
- FFI string free function (wc_string_free)
- FFI transceiver free function (wc_transceiver_free)
- Static version string for wc_version()
- Comprehensive unit tests for memory safety
- Integration tests for engine lifecycle
- Module-level documentation
- FFI C header (ffi/include/webrtc-core.h)
- FFI README with memory management rules

### Changed

- Replaced usize indices with SlabKey throughout codebase
- Removed std::panic::catch_unwind from FrameAssembler
- Updated SRTP protect/unprotect to use SlabKey
- Updated AudioJitterBuffer to use SlabKey
- Optimized clone() calls in ICE Agent
- Changed version from 0.7.1 to 1.0.0
- Updated status from "production-ready" to "Beta"
- Removed "sub-microsecond" performance claims

### Fixed

- Critical use-after-free bug in SlabAllocator
- Race condition in ByteRing (memory ordering)
- Race condition in IndexRing (memory ordering)
- Race condition in TwccAggregator (concurrent access)
- Race condition in RtcpQueue (TOCTOU)
- FFI memory leak in wc_session_description_get_sdp
- FFI memory leak in wc_version
- FFI memory leak in wc_peer_connection_add_transceiver
- Invalid memory access in AudioJitterBuffer
- Panic recovery issue in VideoFrameBuffer

### Removed

- All placeholder and TODO comments
- Debug println! statements
- Dead code and commented-out code
- #[allow] suppressions (fixed underlying warnings)

### Documentation

- Added safety invariant documentation for all unsafe blocks
- Added SPSC/MPSC concurrency model documentation
- Added FFI usage guide with examples
- Added memory management rules for FFI
- Added comprehensive README with beta status
- Added CHANGELOG.md

## [0.7.1] - 2024-02-XX

### Added

- Initial release with basic WebRTC functionality
- SRTP protection/unprotection
- ICE agent implementation
- DTLS handshake
- RTCP feedback generation
- Audio jitter buffer
- Video frame buffer
- Congestion control (GCC, AIMD, TWCC)
- FFI bindings for C/C++

### Known Issues

- Use-after-free vulnerability in SlabAllocator
- Race conditions in ring buffers
- FFI memory leaks
- Missing safety documentation
- Limited test coverage

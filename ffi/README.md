# WebRTC Core FFI

C-compatible Foreign Function Interface for webrtc-core library.

## Memory Management Rules

### String Ownership

- `wc_session_description_get_sdp()` returns a heap-allocated string that MUST be freed with `wc_string_free()`
- `wc_version()` returns a static string pointer - DO NOT FREE
- All other string parameters are borrowed and should not be freed by the caller

### Object Ownership

- All `wc_*_create()` functions return heap-allocated objects that MUST be freed with their corresponding `wc_*_free()` function
- Transceivers created with `wc_peer_connection_add_transceiver()` use reference counting - must be freed with `wc_transceiver_free()`
- Never manually free objects with `free()` - always use the provided `*_free()` functions

### Thread Safety

- PeerConnection objects are thread-safe internally
- DataChannel objects are thread-safe internally
- ICE Agent objects are thread-safe internally
- DTLS Endpoint objects are thread-safe internally
- SCTP Transport objects are thread-safe internally
- Do not share the same object across threads without external synchronization if calling mutating operations

## Usage Examples

### C Example

```c
#include "webrtc-core.h"
#include <stdio.h>

int main() {
    WcPeerConnection* pc = wc_peer_connection_create(NULL);
    if (!pc) {
        fprintf(stderr, "Failed to create peer connection\n");
        return 1;
    }

    WcSessionDescription* offer = wc_peer_connection_create_offer(pc);
    if (!offer) {
        fprintf(stderr, "Failed to create offer\n");
        wc_peer_connection_free(pc);
        return 1;
    }

    char* sdp = wc_session_description_get_sdp(offer);
    printf("Offer SDP: %s\n", sdp);

    wc_string_free(sdp);
    wc_session_description_free(offer);
    wc_peer_connection_free(pc);

    return 0;
}
```

### C++ Example

```cpp
#include "webrtc-core.h"
#include <iostream>

class PeerConnectionGuard {
    WcPeerConnection* pc;
public:
    explicit PeerConnectionGuard(const char* config = nullptr)
        : pc(wc_peer_connection_create(config)) {}
    ~PeerConnectionGuard() { if (pc) wc_peer_connection_free(pc); }
    WcPeerConnection* get() { return pc; }
    operator bool() const { return pc != nullptr; }
};

int main() {
    PeerConnectionGuard pc;
    if (!pc) {
        std::cerr << "Failed to create peer connection" << std::endl;
        return 1;
    }

    WcSessionDescription* offer = wc_peer_connection_create_offer(pc.get());
    if (!offer) {
        std::cerr << "Failed to create offer" << std::endl;
        return 1;
    }

    char* sdp = wc_session_description_get_sdp(offer);
    std::cout << "Offer SDP: " << sdp << std::endl;

    wc_string_free(sdp);
    wc_session_description_free(offer);

    return 0;
}
```

## Error Handling

Most functions return:

- `NULL` / `nullptr` on failure for pointer-returning functions
- `-1` on failure for integer-returning functions
- `0` on success for integer-returning functions

Always check return values before using returned pointers.

## Building

Add the following to your Cargo.toml:

```toml
[dependencies]
webrtc-core = "0.7"

[build-dependencies]
cbindgen = "0.24"
```

## Callback Safety

Callbacks MUST NOT:

- Panic or unwind into Rust code
- Access freed memory
- Call long-running operations that block the thread

Callbacks SHOULD:

- Complete quickly (avoid blocking operations)
- Use thread-safe mechanisms if sharing data
- Handle errors gracefully

## SDP Type Constants

- `0` - Offer
- `1` - Answer
- `2` - PrAnswer
- `3` - Rollback

## Media Kind Constants

- `0` - Audio
- `1` - Video

## PeerConnection State Constants

- `0` - New
- `1` - Connecting
- `2` - Connected
- `3` - Disconnected
- `4` - Failed
- `5` - Closed

## DataChannel State Constants

- `0` - Connecting
- `1` - Open
- `2` - Closing
- `3` - Closed

## ICE State Constants

- `0` - New
- `1` - Gathering
- `2` - Checking
- `3` - Connected
- `4` - Completed
- `5` - Failed
- `6` - Closed

## DTLS State Constants

- `0` - Closed
- `1` - Connecting
- `2` - Connected
- `3` - Failed

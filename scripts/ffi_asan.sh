#!/bin/bash
set -e

echo "Building FFI library with AddressSanitizer..."
cd ffi
RUSTFLAGS="-Z sanitizer=address" cargo build --release

echo "Running FFI tests with ASAN..."
ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 \
    ./target/release/libwebrtc_ffi_test.so

echo "✓ No memory errors detected by ASAN"

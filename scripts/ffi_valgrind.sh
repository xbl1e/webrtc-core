#!/bin/bash
set -e

echo "Building FFI library with debug symbols..."
cd ffi
cargo build --release

echo "Running Valgrind on test program..."
valgrind --leak-check=full --show-leak-kinds=all --track-origins=yes --verbose \
    --log-file=valgrind-out.txt \
    ./target/release/libwebrtc_ffi_test.so

echo "Checking Valgrind output..."
if grep -q "definitely lost:" valgrind-out.txt; then
    echo "ERROR: Memory leaks detected!"
    grep "definitely lost:" valgrind-out.txt
    exit 1
fi

if grep -q "Invalid" valgrind-out.txt; then
    echo "ERROR: Invalid memory access detected!"
    grep "Invalid" valgrind-out.txt
    exit 1
fi

echo "✓ No memory leaks or invalid accesses detected"

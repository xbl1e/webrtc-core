#!/bin/bash
set -e

TARGET=${1:-all}

echo "Checking for cargo-fuzz..."
if ! cargo fuzz --version > /dev/null 2>&1; then
    echo "Installing cargo-fuzz..."
    cargo install cargo-fuzz --version 0.11.0
fi

run_fuzz_target() {
    local target=$1
    echo "Running fuzz target: $target"
    cargo fuzz run "$target" -- -max_total_time=300
}

case "$TARGET" in
    slab)
        run_fuzz_target slab
        ;;
    byte_ring)
        run_fuzz_target byte_ring
        ;;
    index_ring)
        run_fuzz_target index_ring
        ;;
    latency_ring)
        run_fuzz_target latency_ring
        ;;
    rtcp)
        run_fuzz_target rtcp
        ;;
    sdp)
        run_fuzz_target sdp
        ;;
    all)
        for target in slab byte_ring index_ring latency_ring rtcp sdp; do
            run_fuzz_target "$target"
        done
        ;;
    *)
        echo "Unknown target: $TARGET"
        echo "Usage: $0 [slab|byte_ring|index_ring|latency_ring|rtcp|sdp|all]"
        exit 1
        ;;
esac

echo "✓ All fuzzing targets completed without crashes"

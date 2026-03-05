#!/bin/bash
set -e

echo "Checking for nightly Rust toolchain..."
if ! rustup +nightly show > /dev/null 2>&1; then
    echo "Installing nightly Rust toolchain..."
    rustup install nightly
fi

echo "Installing Miri..."
cargo +nightly install miri

echo "Running Miri on all tests..."
cargo +nightly miri test --all-features

echo "✓ All Miri checks passed - no undefined behavior detected"

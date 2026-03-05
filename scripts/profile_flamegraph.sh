#!/bin/bash
set -e

echo "Building for profiling..."
cargo flamegraph --bench throughput

echo "Flamegraph generated in flamegraph.svg"
echo "Open with: xdg-open flamegraph.svg or open flamegraph.svg (macOS)"

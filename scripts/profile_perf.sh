#!/bin/bash
set -e

echo "Building for perf profiling..."
cargo build --release --bench throughput

echo "Running perf record..."
sudo perf record -g --call-graph dwarf target/release/benches/throughput --bench

echo "Generating perf report..."
sudo perf report

echo "To generate flamegraph: sudo perf script | stackcollapse-perf.pl | flamegraph.pl > perf-flamegraph.svg"

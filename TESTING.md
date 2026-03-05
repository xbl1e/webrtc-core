# Testing Guide

This document describes the comprehensive testing strategy for webrtc-core.

## Test Coverage Goals

- Unit tests: >80% line coverage
- Integration tests: Critical paths
- Miri validation: All unsafe code
- Fuzzing: All data parsing and state machines

## Running Tests

### All Tests

```bash
cargo test --all-features
```

### Specific Test

```bash
cargo test test_slab_key_generation
cargo test tests::integration_test
```

### With Output

```bash
cargo test --all-features -- --nocapture
```

## Test Types

### Unit Tests

Located in each module's `#[cfg(test)]` blocks.

Run with:
```bash
cargo test --lib
```

### Integration Tests

Located in `tests/` directory.

Run with:
```bash
cargo test --test integration_test
```

### Miri Tests

Located in `tests/miri_test.rs`.

Run with:
```bash
cargo +nightly miri test
```

Note: Requires nightly Rust toolchain.

### Fuzz Tests

Located in `fuzz/fuzz_targets/`.

Run with:
```bash
./scripts/run_fuzz.sh all
```

Or run individual targets:
```bash
./scripts/run_fuzz.sh slab
./scripts/run_fuzz.sh byte_ring
```

## Critical Test Areas

### Memory Safety

- SlabKey generation and validation
- Ring buffer SPSC/MPSC invariants
- Use-after-free prevention
- Double-free handling

Tests:
- `test_slab_key_generation`
- `test_slab_double_free_safety`
- `test_slab_concurrent_allocation`

### Concurrency

- Race condition prevention
- Memory ordering correctness
- Thread-safe data structures

Tests:
- `test_concurrent_slab_allocation`
- `test_rtcp_queue_mpsc`
- SPSC stress tests in ring buffers

### Correctness

- Packet processing accuracy
- State machine transitions
- Data integrity

Tests:
- Engine lifecycle tests
- Integration tests
- Realistic scenario tests

## Fuzzing Targets

### Data Structures

- `slab` - SlabAllocator allocation patterns
- `byte_ring` - ByteRing read/write patterns
- `index_ring` - IndexRing push/pop patterns
- `latency_ring` - LatencyRing push/pop patterns
- `rtcp` - RtcpQueue message handling

### Protocols

- `sdp` - SDP parser robustness

## Continuous Testing

### Pre-commit

Run fast tests before committing:
```bash
cargo test --lib --all-features
```

### CI/CD

Run full test suite in CI:
```bash
cargo test --all-features
cargo clippy --all-features -- -D warnings
cargo fmt -- --check
```

### Nightly

Run Miri periodically:
```bash
cargo +nightly miri test --all-features
```

Run fuzzing overnight:
```bash
./scripts/run_fuzz.sh all
```

## Coverage

### Generating Coverage Report

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html --all-features
```

### Coverage Goals

- Core modules: >90%
- FFI layer: >85%
- Protocol parsers: >95%

## Test Writing Guidelines

### Unit Tests

1. Test public API surface
2. Test edge cases (empty, full, max values)
3. Test error paths
4. Use descriptive test names
5. Keep tests focused and independent

### Integration Tests

1. Test realistic workflows
2. Test module interactions
3. Test state transitions
4. Include cleanup

### Fuzz Tests

1. Define clear invariants
2. Check invariants after each operation
3. Handle all error cases
4. Limit iterations for speed

## Debugging Test Failures

### Enable Logging

```bash
RUST_LOG=debug cargo test --all-features
```

### Run Single Test

```bash
cargo test test_name -- --nocapture
```

### Use GDB/LLDB

```bash
cargo test --no-run
rust-gdb target/debug/deps/test_name-*
```

## Test Maintenance

### Updating Tests

When adding new features:

1. Add unit tests in feature module
2. Add integration tests in `tests/`
3. Add fuzz targets for parsers
4. Update this guide

### Removing Tests

Remove obsolete tests but keep:

1. Tests that found historical bugs
2. Tests that document edge cases
3. Tests that verify invariants

## Known Test Limitations

1. No network testing (requires mock networks)
2. Limited codec testing (FFI only)
3. No load testing (requires external tools)
4. Limited fuzzing corpus size (time constraints)

## Contributing Tests

When contributing:

1. Add tests for new functionality
2. Ensure all tests pass
3. Maintain or improve coverage
4. Document test purpose
5. Add to relevant test categories

## Security Testing

### Static Analysis

```bash
cargo audit
```

### Miri

```bash
cargo +nightly miri test --all-features
```

### Fuzzing

```bash
./scripts/run_fuzz.sh all
```

### Valgrind/ASan

```bash
./scripts/ffi_valgrind.sh
./scripts/ffi_asan.sh
```

## Performance Regression Testing

Compare benchmark results against baseline:

```bash
cargo bench -- --save-baseline main
cargo bench -- --baseline main
```

Report regressions >5% as issues.

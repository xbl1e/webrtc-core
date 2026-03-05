#![no_main]
use libfuzzer_sys::fuzz_target;
use webrtc_core::IndexRing;

fuzz_target!(|data: &[u8]| {
    let ring = IndexRing::new(64);
    let mut pushed = Vec::new();

    for &byte in data.iter().take(1000) {
        match byte % 2 {
            0 => {
                let value = byte as usize;
                if ring.push(value) {
                    pushed.push(value);
                }
            }
            1 => {
                if let Some(value) = ring.pop() {
                    if !pushed.is_empty() {
                        pushed.remove(0);
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    assert!(pushed.len() <= ring.capacity());
});

#![no_main]
use libfuzzer_sys::fuzz_target;
use webrtc_core::SlabAllocator;

fuzz_target!(|data: &[u8]| {
    let slab = SlabAllocator::new(16);
    let mut keys = Vec::new();

    for &byte in data.iter().take(100) {
        match byte % 3 {
            0 => {
                if let Some(key) = slab.allocate() {
                    keys.push(key);
                }
            }
            1 => {
                if let Some(key) = keys.pop() {
                    slab.free(key);
                }
            }
            2 => {
                if let Some(key) = keys.last() {
                    let _ = slab.get_mut(key);
                }
            }
            _ => unreachable!(),
        }
    }
});

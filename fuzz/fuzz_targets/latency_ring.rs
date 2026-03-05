#![no_main]
use libfuzzer_sys::fuzz_target;
use webrtc_core::LatencyRing;

fuzz_target!(|data: &[u8]| {
    let ring = LatencyRing::new(64);
    let mut read_buf = vec![0u64; 128];

    let mut total_pushed = 0;
    let mut total_read = 0;

    for (i, &byte) in data.iter().enumerate().take(1000) {
        match byte % 2 {
            0 => {
                let value = (i as u64) * 1000;
                if ring.push(value) {
                    total_pushed += 1;
                }
            }
            1 => {
                total_read += ring.pop_batch(&mut read_buf);
            }
            _ => unreachable!(),
        }
    }

    assert!(total_pushed <= ring.capacity());
    assert!(total_read <= total_pushed);
});

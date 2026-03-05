#![no_main]
use libfuzzer_sys::fuzz_target;
use webrtc_core::ByteRing;

fuzz_target!(|data: &[u8]| {
    let ring = ByteRing::new(64);
    let mut read_buf = vec![0u8; 128];

    let mut total_written = 0;
    let mut total_read = 0;

    for &byte in data.iter().take(1000) {
        match byte % 2 {
            0 => {
                let to_write = &data[total_written % data.len()..].min(&data[total_written % data.len()..][..32.min(data.len().saturating_sub(total_written % data.len()))];
                if !to_write.is_empty() {
                    total_written += ring.write(to_write);
                }
            }
            1 => {
                total_read += ring.read(&mut read_buf);
            }
            _ => unreachable!(),
        }
    }

    assert!(total_written <= ring.capacity());
    assert!(total_read <= total_written);
});

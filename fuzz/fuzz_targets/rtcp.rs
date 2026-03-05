#![no_main]
use libfuzzer_sys::fuzz_target;
use webrtc_core::RtcpSendQueue;

fuzz_target!(|data: &[u8]| {
    let queue = RtcpSendQueue::new(32);
    let mut read_buf = vec![0u8; 512];

    let mut total_written = 0;
    let mut total_read = 0;

    for (i, &byte) in data.iter().enumerate().take(1000) {
        match byte % 2 {
            0 => {
                let len = (byte as usize).min(512).min(data.len().saturating_sub(i));
                let packet = &data[i..][..len.min(data.len().saturating_sub(i))];
                if queue.push_drop_oldest(packet) {
                    total_written += packet.len();
                }
            }
            1 => {
                if let Some(n) = queue.pop(&mut read_buf) {
                    total_read += n;
                }
            }
            _ => unreachable!(),
        }
    }

    assert!(total_read <= total_written);
});

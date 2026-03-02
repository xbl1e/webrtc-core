use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use webrtc_core::RtcpSendQueue;
use webrtc_core::{index_ring::IndexRing, slab::SlabAllocator, MediaEngine};

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn main() {
    let slab = std::sync::Arc::new(SlabAllocator::new(4096));
    let idx_ring = std::sync::Arc::new(IndexRing::new(4096));
    let _rtcp_q = std::sync::Arc::new(RtcpSendQueue::new(256));
    let slab_net = slab.clone();
    let idx_net = idx_ring.clone();
    let engine = MediaEngine::new(1024);
    let mut engine = engine.start();

    let media_handle = thread::spawn(move || loop {
        while let Some(slot) = idx_ring.pop() {
            let arrival = now_ts();
            engine.handle_incoming_index(slot, arrival);
        }
        engine.poll_step();
        thread::sleep(Duration::from_millis(5));
    });

    for i in 0..100u32 {
        if let Some(guard) = SlabAllocator::allocate_guard(&slab_net) {
            unsafe {
                let pkt = guard.get_mut();
                let len = 160usize.min(pkt.data.len());
                for j in 0..len {
                    pkt.data[j] = (i & 0xff) as u8;
                }
                pkt.len = len;
                pkt.seq = i as u16;
            }
            let idx = guard.into_index();
            while !idx_net.push(idx) {
                thread::yield_now();
            }
        }
        thread::sleep(Duration::from_millis(10));
    }

    let _ = media_handle.join();
}

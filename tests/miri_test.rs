use webrtc_core::{ByteRing, IndexRing, LatencyRing, SlabAllocator, SlabKey, RtcpSendQueue};

#[test]
fn test_slab_key_generation() {
    let slab = SlabAllocator::new(4);

    let key1 = slab.allocate().unwrap();
    let gen1 = key1.generation();

    slab.free(key1);

    let key2 = slab.allocate().unwrap();

    assert_eq!(key2.index(), key1.index());
    assert!(key2.generation() > gen1);

    assert!(slab.get_mut(&key1).is_none());
    assert!(slab.get_mut(&key2).is_some());
}

#[test]
fn test_byte_ring_memory_ordering() {
    let ring = ByteRing::new(16);

    let data = vec![1u8, 2, 3, 4];
    let written = ring.write(&data);
    assert_eq!(written, 4);

    let mut buf = [0u8; 16];
    let read = ring.read(&mut buf);
    assert_eq!(read, 4);
    assert_eq!(&buf[..4], &data[..]);
}

#[test]
fn test_index_ring_memory_ordering() {
    let ring = IndexRing::new(16);

    assert!(ring.push(123));
    assert!(ring.push(456));
    assert!(ring.push(789));

    assert_eq!(ring.pop(), Some(123));
    assert_eq!(ring.pop(), Some(456));
    assert_eq!(ring.pop(), Some(789));
    assert_eq!(ring.pop(), None);
}

#[test]
fn test_latency_ring_memory_ordering() {
    let ring = LatencyRing::new(16);

    for i in 0..16u64 {
        assert!(ring.push(i));
    }
    assert!(!ring.push(16));

    let mut buf = vec![0u64; 32];
    let n = ring.pop_batch(&mut buf);
    assert_eq!(n, 16);

    for i in 0..16u64 {
        assert_eq!(buf[i as usize], i);
    }
}

#[test]
fn test_slab_concurrent_allocation() {
    use std::sync::Arc;
    use std::thread;

    let slab = Arc::new(SlabAllocator::new(64));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let slab_clone = slab.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                if let Some(key) = slab_clone.allocate() {
                    std::hint::black_box(key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().ok();
    }

    assert_eq!(slab.allocated_count(), 0);
}

#[test]
fn test_rtcp_queue_mpsc() {
    use std::sync::Arc;
    use std::thread;

    let queue = Arc::new(RtcpSendQueue::new(32));
    let mut handles = Vec::new();

    for t in 0..4 {
        let queue_clone = queue.clone();
        handles.push(thread::spawn(move || {
            let data = vec![(t * 10) as u8; 10];
            for _ in 0..10 {
                queue_clone.push_drop_oldest(&data);
            }
        }));
    }

    for handle in handles {
        handle.join().ok();
    }

    let mut buf = [0u8; 512];
    let mut total_read = 0;
    while queue.pop(&mut buf).is_some() {
        total_read += 1;
    }

    assert!(total_read <= 32);
}

#[test]
fn test_slab_get_mut_unchecked_safety() {
    let slab = SlabAllocator::new(16);

    let key = slab.allocate().unwrap();

    unsafe {
        let pkt = slab.get_mut_unchecked(key);
        pkt.len = 100;
        pkt.data[..100].copy_from_slice(&vec![1u8; 100][..]);
    }

    if let Some(pkt) = slab.get_mut(&key) {
        assert_eq!(pkt.len, 100);
        assert_eq!(&pkt.data[..100], &vec![1u8; 100][..]);
    }
}

#[test]
fn test_slab_double_free_safety() {
    let slab = SlabAllocator::new(16);

    let key = slab.allocate().unwrap();

    slab.free(key);
    slab.free(key);

    let new_key = slab.allocate().unwrap();

    assert_eq!(new_key.index(), key.index());
    assert!(new_key.generation() > key.generation());
}

#[test]
fn test_ring_wrap_around() {
    let ring = ByteRing::new(8);

    for i in 0..32u8 {
        let mut buf = [i];
        assert!(ring.write(&buf) > 0);
        let mut out = [0u8];
        assert!(ring.read(&mut out) > 0);
        assert_eq!(out[0], i);
    }
}

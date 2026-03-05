use webrtc_core::{EngineHandle, SlabAllocator};

#[test]
fn engine_lifecycle() {
    let handle = EngineHandle::builder().build();
    let payload = vec![0u8; 160];

    for i in 0..100u16 {
        assert!(handle.feed_packet(&payload, i, 0x1234).is_ok());
    }

    handle.shutdown();
}

#[test]
fn slab_allocation() {
    let slab = SlabAllocator::new(16);

    let mut keys = Vec::new();
    for _ in 0..16 {
        if let Some(key) = slab.allocate() {
            keys.push(key);
        }
    }

    assert_eq!(keys.len(), 16);
    assert!(slab.allocate().is_none());

    for key in keys {
        slab.free(key);
    }

    assert!(slab.allocate().is_some());
}

#[test]
fn slab_generation_safety() {
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
fn ring_buffer_operations() {
    use webrtc_core::{ByteRing, IndexRing};

    let byte_ring = ByteRing::new(16);
    let index_ring = IndexRing::new(16);

    assert!(byte_ring.write(&[1, 2, 3, 4]) > 0);
    assert!(index_ring.push(123));

    let mut out = [0u8; 16];
    assert!(byte_ring.read(&mut out) > 0);
    assert!(index_ring.pop().is_some());
}

#[test]
fn custom_configuration() {
    let handle = EngineHandle::builder()
        .jitter_capacity(2048)
        .slab_capacity(8192)
        .index_capacity(8192)
        .rtcp_capacity(512)
        .build();

    let payload = vec![0u8; 160];
    assert!(handle.feed_packet(&payload, 1u16, 0x1234).is_ok());

    handle.shutdown();
}

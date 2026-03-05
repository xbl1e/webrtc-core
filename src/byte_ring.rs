use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct ByteRing {
    buf: Box<[UnsafeCell<u8>]>,
    capacity: usize,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
}

unsafe impl Send for ByteRing {}
unsafe impl Sync for ByteRing {}

impl ByteRing {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(UnsafeCell::new(0u8));
        }
        Self {
            buf: v.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    pub fn write(&self, src: &[u8]) -> usize {
        let mut written = 0usize;
        while written < src.len() {
            let tail = self.tail.load(Ordering::Relaxed);
            let head = self.head.load(Ordering::Acquire);
            if tail.wrapping_sub(head) >= self.capacity {
                break;
            }
            let idx = tail & self.mask;

            unsafe {
                let slot = self.buf.get_unchecked(idx).get();
                ptr::write(slot, src[written]);
            }
            written += 1;
            self.tail.store(tail.wrapping_add(1), Ordering::Release);
        }
        written
    }

    pub fn try_write(&self, byte: u8) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= self.capacity {
            return false;
        }
        let idx = tail & self.mask;
        unsafe {
            let slot = self.buf.get_unchecked(idx).get();
            ptr::write(slot, byte);
        }
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn read(&self, dst: &mut [u8]) -> usize {
        let mut read = 0usize;
        while read < dst.len() {
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Acquire);
            if head == tail {
                break;
            }
            let idx = head & self.mask;

            unsafe {
                let slot = self.buf.get_unchecked(idx).get();
                dst[read] = ptr::read(slot);
            }
            read += 1;
            self.head.store(head.wrapping_add(1), Ordering::Release);
        }
        read
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> usize {
        self.len() >= self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn byte_ring_write_read() {
        let ring = ByteRing::new(8);
        let data = vec![1u8, 2, 3, 4, 5];
        assert_eq!(ring.write(&data), 5);
        let mut out = [0u8; 10];
        assert_eq!(ring.read(&mut out), 5);
        assert_eq!(&out[..5], &data[..]);
    }

    #[test]
    fn byte_ring_wrap_around() {
        let ring = ByteRing::new(8);
        for i in 0..16u8 {
            let mut buf = [i];
            assert_eq!(ring.write(&buf), 1);
            let mut out = [0u8];
            assert_eq!(ring.read(&mut out), 1);
            assert_eq!(out[0], i);
        }
    }

    #[test]
    fn byte_ring_try_write() {
        let ring = ByteRing::new(4);
        assert!(ring.try_write(1));
        assert!(ring.try_write(2));
        assert!(ring.try_write(3));
        assert!(ring.try_write(4));
        assert!(!ring.try_write(5));
        let mut out = [0u8; 4];
        assert_eq!(ring.read(&mut out), 4);
        assert_eq!(out, [1, 2, 3, 4]);
    }

    #[test]
    fn byte_ring_spsc_stress() {
        let ring = std::sync::Arc::new(ByteRing::new(1024));
        let rp = ring.clone();
        let producer = thread::spawn(move || {
            for i in 0..10000u8 {
                while !rp.try_write(i) {
                    thread::yield_now();
                }
            }
        });
        let rc = ring.clone();
        let consumer = thread::spawn(move || {
            let mut buf = [0u8; 10000];
            let mut total = 0usize;
            while total < 10000 {
                let n = rc.read(&mut buf[total..]);
                if n == 0 {
                    thread::yield_now();
                    continue;
                }
                total += n;
            }
            buf
        });
        producer.join().ok();
        let data = consumer.join().ok().unwrap();
        assert_eq!(data.len(), 10000);
        for (i, &b) in data.iter().enumerate() {
            assert_eq!(b, i as u8);
        }
    }
}

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
}

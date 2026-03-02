use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct LatencyRing {
    buf: Box<[UnsafeCell<MaybeUninit<u64>>]>,
    capacity: usize,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
}

unsafe impl Send for LatencyRing {}
unsafe impl Sync for LatencyRing {}

impl LatencyRing {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(UnsafeCell::new(MaybeUninit::uninit()));
        }
        Self {
            buf: v.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    pub fn push(&self, v: u64) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= self.capacity {
            return false;
        }
        let idx = tail & self.mask;
        unsafe {
            let slot = self.buf.get_unchecked(idx).get();
            ptr::write((*slot).as_mut_ptr(), v);
        }
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn pop_batch(&self, out: &mut [u64]) -> usize {
        let mut written = 0usize;
        while written < out.len() {
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Acquire);
            if head == tail {
                break;
            }
            let idx = head & self.mask;
            let v = unsafe {
                let slot = self.buf.get_unchecked(idx).get();
                ptr::read((*slot).as_ptr())
            };
            out[written] = v;
            written += 1;
            self.head.store(head.wrapping_add(1), Ordering::Release);
        }
        written
    }
}

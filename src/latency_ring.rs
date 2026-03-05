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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn latency_ring_basic() {
        let ring = LatencyRing::new(8);
        for i in 0..8u64 {
            assert!(ring.push(i));
        }
        assert!(!ring.push(8));

        let mut out = [0u64; 16];
        let n = ring.pop_batch(&mut out);
        assert_eq!(n, 8);
        for (i, &v) in out[..8].iter().enumerate() {
            assert_eq!(v, i as u64);
        }
    }

    #[test]
    fn latency_ring_wrap_around() {
        let ring = LatencyRing::new(4);
        for i in 0..16u64 {
            let mut out = [0u64; 1];
            assert!(ring.push(i));
            assert_eq!(ring.pop_batch(&mut out), 1);
            assert_eq!(out[0], i);
        }
    }

    #[test]
    fn latency_ring_spsc_stress() {
        let ring = std::sync::Arc::new(LatencyRing::new(1024));
        let rp = ring.clone();
        let producer = thread::spawn(move || {
            for i in 0..10000u64 {
                while !rp.push(i) {
                    thread::yield_now();
                }
            }
        });
        let rc = ring.clone();
        let consumer = thread::spawn(move || {
            let mut buf = [0u64; 10000];
            let mut total = 0usize;
            while total < 10000 {
                let n = rc.pop_batch(&mut buf[total..]);
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
        for (i, &v) in data.iter().enumerate() {
            assert_eq!(v, i as u64);
        }
    }
}

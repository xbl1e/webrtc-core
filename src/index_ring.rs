use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct IndexRing {
    buf: Box<[UnsafeCell<MaybeUninit<usize>>]>,
    capacity: usize,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
}

unsafe impl Send for IndexRing {}
unsafe impl Sync for IndexRing {}

impl IndexRing {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().saturating_mul(2);
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

    pub fn push(&self, v: usize) -> bool {
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

    pub fn pop(&self) -> Option<usize> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let idx = head & self.mask;

        let v = unsafe {
            let slot = self.buf.get_unchecked(idx).get();
            ptr::read((*slot).as_ptr())
        };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(v)
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
    fn wrap_around_push_pop() {
        let r = IndexRing::new(8);
        for i in 0..16usize {
            assert!(r.push(i));
        }
        let mut got = Vec::new();
        while let Some(v) = r.pop() {
            got.push(v);
            if got.len() > 16 {
                break;
            }
        }
        assert!(got.len() <= 16);
    }

    #[test]
    fn producer_consumer_stress() {
        let r = std::sync::Arc::new(IndexRing::new(1024));
        let rp = r.clone();
        let producer = thread::spawn(move || {
            for i in 0..10000usize {
                while !rp.push(i) {
                    thread::yield_now();
                }
            }
        });
        let rc = r.clone();
        let consumer = thread::spawn(move || {
            let mut cnt = 0usize;
            while cnt < 10000 {
                if let Some(_) = rc.pop() {
                    cnt += 1;
                }
            }
            cnt
        });
        producer.join().ok();
        let res = consumer.join().ok().unwrap();
        assert_eq!(res, 10000usize);
    }
}

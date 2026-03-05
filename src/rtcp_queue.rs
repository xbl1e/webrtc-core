use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::{atomic::{AtomicUsize, Ordering}, Mutex};

pub struct RtcpSlot {
    len: AtomicUsize,
    data: UnsafeCell<MaybeUninit<[u8; 512]>>,
}

unsafe impl Send for RtcpSlot {}
unsafe impl Sync for RtcpSlot {}

pub struct RtcpSendQueue {
    slots: Box<[RtcpSlot]>,
    capacity: usize,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
    write_lock: Mutex<()>,
}

unsafe impl Send for RtcpSendQueue {}
unsafe impl Sync for RtcpSendQueue {}

impl RtcpSendQueue {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(RtcpSlot {
                len: AtomicUsize::new(0),
                data: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }
        Self {
            slots: v.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            write_lock: Mutex::new(()),
        }
    }

    pub fn push_drop_oldest(&self, pkt: &[u8]) -> bool {
        if pkt.len() > 512 {
            return false;
        }
        let _guard = self.write_lock.lock().ok();
        if _guard.is_none() {
            return false;
        }
        loop {
            let tail = self.tail.load(Ordering::Relaxed);
            let head = self.head.load(Ordering::Acquire);
            let used = tail.wrapping_sub(head);
            if used >= self.capacity {
                let _ = self.head.compare_exchange(
                    head,
                    head.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                continue;
            }
            let idx = tail & self.mask;
            let slot = &self.slots[idx];

            unsafe {
                (*slot.data.get()).as_mut_ptr().write([0u8; 512]);
            }
            let dst = unsafe { &mut *slot.data.get() };
            let arr = unsafe { &mut *(&mut *dst.as_mut_ptr()) };
            arr[..pkt.len()].copy_from_slice(pkt);
            slot.len.store(pkt.len(), Ordering::Release);
            self.tail.store(tail.wrapping_add(1), Ordering::Release);
            return true;
        }
    }

    pub fn pop(&self, out: &mut [u8]) -> Option<usize> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let idx = head & self.mask;
        let slot = &self.slots[idx];
        let len = slot.len.load(Ordering::Acquire);
        if len == 0 {
            return None;
        }

        let src = unsafe { &*slot.data.get() };
        let arr = unsafe { &*(&*src.as_ptr()) };
        let copy_len = len.min(out.len());
        out[..copy_len].copy_from_slice(&arr[..copy_len]);
        slot.len.store(0, Ordering::Release);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(copy_len)
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
    use std::sync::Arc;

    #[test]
    fn rtcp_queue_basic() {
        let queue = RtcpSendQueue::new(8);
        let data = vec![1u8, 2, 3, 4];
        assert!(queue.push_drop_oldest(&data));
        let mut out = [0u8; 10];
        assert_eq!(queue.pop(&mut out), Some(4));
        assert_eq!(&out[..4], &data[..]);
    }

    #[test]
    fn rtcp_queue_drop_oldest() {
        let queue = RtcpSendQueue::new(4);
        for i in 0..8u8 {
            let data = vec![i];
            queue.push_drop_oldest(&data);
        }
        let mut out = [0u8; 10];
        let mut count = 0;
        while queue.pop(&mut out).is_some() {
            count += 1;
        }
        assert_eq!(count, 4);
    }

    #[test]
    fn rtcp_queue_mpsc() {
        let queue = Arc::new(RtcpSendQueue::new(64));
        let mut handles = Vec::new();

        for t in 0..4 {
            let q = queue.clone();
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    let data = vec![(t * 100 + i) as u8];
                    q.push_drop_oldest(&data);
                }
            }));
        }

        for h in handles {
            h.join().ok();
        }

        let mut total = 0usize;
        let mut out = [0u8; 512];
        while queue.pop(&mut out).is_some() {
            total += 1;
        }
        assert!(total <= 64);
    }
}

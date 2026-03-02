use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

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
        }
    }

    pub fn push_drop_oldest(&self, pkt: &[u8]) -> bool {
        if pkt.len() > 512 {
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
}

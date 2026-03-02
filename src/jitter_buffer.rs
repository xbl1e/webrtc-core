use crate::slab::SlabAllocator;
use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

pub struct AudioJitterBuffer {
    slots: Box<[UnsafeCell<MaybeUninit<usize>>]>,
    capacity: usize,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
    last_seq: AtomicU32,
    gap_deadline_ns: AtomicU64,
    ewma_delay_ns: AtomicU64,
}

unsafe impl Send for AudioJitterBuffer {}
unsafe impl Sync for AudioJitterBuffer {}

impl AudioJitterBuffer {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(UnsafeCell::new(MaybeUninit::uninit()));
        }
        Self {
            slots: v.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            last_seq: AtomicU32::new(0),
            gap_deadline_ns: AtomicU64::new(0),
            ewma_delay_ns: AtomicU64::new(0),
        }
    }

    pub fn push_index(&self, idx_val: usize) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= self.capacity {
            return false;
        }
        let idx = tail & self.mask;
        unsafe {
            let slot = self.slots.get_unchecked(idx).get();
            ptr::write((*slot).as_mut_ptr(), idx_val);
        }
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn pop_index(&self) -> Option<usize> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let idx = head & self.mask;



        let v = unsafe {
            let slot = self.slots.get_unchecked(idx).get();
            ptr::read((*slot).as_ptr())
        };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(v)
    }

    pub fn collect_missing(&self, slab: &SlabAllocator, out: &mut [u16]) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let window = self.capacity.min(1024);
        let mut written = 0usize;
        for i in 0..window {
            let idx = (tail.wrapping_sub(1).wrapping_sub(i)) & self.mask;
            let slot = unsafe {
                let s = self.slots.get_unchecked(idx).get();
                ptr::read((*s).as_ptr())
            };
            let seq = unsafe { slab.get_mut(slot).seq };
            if written < out.len() {
                out[written] = seq;
                written += 1;
            } else {
                break;
            }
        }
        written
    }

    pub fn twcc_summary(&self, slab: &SlabAllocator) -> (u16, u64) {
        let tail = self.tail.load(Ordering::Acquire);
        let mut mask: u64 = 0;
        let mut largest: u16 = 0;
        for i in 0..64usize {
            let idx = (tail.wrapping_sub(1).wrapping_sub(i)) & self.mask;
            let slot = unsafe {
                let s = self.slots.get_unchecked(idx).get();
                ptr::read((*s).as_ptr())
            };
            let seq = unsafe { slab.get_mut(slot).seq };
            if seq != 0 {
                let s = seq;
                if largest == 0 {
                    largest = s
                };
                let bit = (s.wrapping_sub(largest)) as i64;
                if bit <= 0 && bit >= -63 {
                    mask |= 1u64 << (-(bit) as u64);
                }
            }
        }
        (largest, mask)
    }

    pub fn push_index_with_seq(
        &self,
        idx_val: usize,
        arrival_ns: u64,
        slab: &SlabAllocator,
    ) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= self.capacity {
            return false;
        }
        let idx = tail & self.mask;
        unsafe {
            let slot = self.slots.get_unchecked(idx).get();
            ptr::write((*slot).as_mut_ptr(), idx_val);
        }
        let seq = unsafe { slab.get_mut(idx_val).seq };
        let sample = {
            let pkt = unsafe { slab.get_mut(idx_val) };
            arrival_ns.saturating_sub(pkt.timestamp)
        };
        let alpha_num: u128 = 1;
        let alpha_den: u128 = 10;
        loop {
            let old = self.ewma_delay_ns.load(Ordering::Acquire) as u128;
            let new = ((alpha_num * (sample as u128)) + (old * (alpha_den - alpha_num))) / alpha_den;
            if self
                .ewma_delay_ns
                .compare_exchange(old as u64, new as u64, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
        let prev = self.last_seq.load(Ordering::Acquire) as u16;
        let expected = prev.wrapping_add(1);
        if prev != 0 && seq != expected {
            let dd = arrival_ns + 20_000_000u64;
            let _ =
                self.gap_deadline_ns
                    .compare_exchange(0, dd, Ordering::AcqRel, Ordering::Acquire);
        } else {
            self.gap_deadline_ns.store(0, Ordering::Release);
        }
        self.last_seq.store(seq as u32, Ordering::Release);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn check_and_emit_nack(&self, now_ns: u64) -> bool {
        let d = self.gap_deadline_ns.load(Ordering::Acquire);
        if d != 0 && now_ns >= d {
            let _ =
                self.gap_deadline_ns
                    .compare_exchange(d, 0, Ordering::AcqRel, Ordering::Acquire);
            return true;
        }
        false
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn get_ewma_delay_ms(&self) -> u64 {
        self.ewma_delay_ns.load(Ordering::Relaxed) / 1_000_000u64
    }
}

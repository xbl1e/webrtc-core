use crate::packet::AudioPacket;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct SlabAllocator {
    slots: Box<[UnsafeCell<MaybeUninit<AudioPacket>>]>,
    next: Box<[AtomicUsize]>,
    head: Mutex<usize>,
    allocated: AtomicUsize,
    capacity: usize,
}

unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

pub struct SlabGuard {
    slab: Arc<SlabAllocator>,
    idx: usize,
    active: AtomicBool,
}

impl SlabAllocator {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(UnsafeCell::new(MaybeUninit::uninit()));
        }
        let mut n = Vec::with_capacity(cap);
        for i in 0..cap {
            n.push(AtomicUsize::new(i + 1));
        }
        n[cap - 1].store(usize::MAX, Ordering::Relaxed);
        Self {
            slots: v.into_boxed_slice(),
            next: n.into_boxed_slice(),
            head: Mutex::new(0),
            allocated: AtomicUsize::new(0),
            capacity: cap,
        }
    }

    pub fn allocate(&self) -> Option<usize> {

        let res = self.allocated.fetch_update(Ordering::AcqRel, Ordering::Acquire, |cur| {
            if cur < self.capacity {
                Some(cur + 1)
            } else {
                None
            }
        });
        if res.is_err() {
            return None;
        }

        let mut guard = self.head.lock().unwrap();
        let head = *guard;
        if head == usize::MAX {

            self.allocated.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        let next = self.next[head].load(std::sync::atomic::Ordering::Acquire);
        *guard = next;
        Some(head)
    }

    pub fn allocate_guard(arc: &Arc<Self>) -> Option<SlabGuard> {
        let idx = arc.allocate()?;
        Some(SlabGuard {
            slab: arc.clone(),
            idx,
            active: AtomicBool::new(true),
        })
    }

    pub fn free(&self, idx: usize) {
        let mut guard = self.head.lock().unwrap();
        let head = *guard;
        self.next[idx].store(head, std::sync::atomic::Ordering::Release);
        *guard = idx;

        self.allocated.fetch_sub(1, Ordering::AcqRel);
    }

    pub unsafe fn get_mut(&self, idx: usize) -> &mut AudioPacket {

        &mut *((*self.slots.get_unchecked(idx).get()).as_mut_ptr())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl SlabGuard {
    pub fn index(&self) -> usize {
        self.idx
    }
    pub unsafe fn get_mut(&self) -> &mut AudioPacket {
        self.slab.get_mut(self.idx)
    }
    pub fn into_index(self) -> usize {
        let idx = self.idx;
        std::mem::forget(self);
        idx
    }
}

impl Drop for SlabGuard {
    fn drop(&mut self) {
        if self.active.load(Ordering::Acquire) {
            self.slab.free(self.idx);
        }
    }
}

impl std::fmt::Debug for SlabGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SlabGuard({})", self.idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn slab_basic_allocate_free() {
        let slab = Arc::new(SlabAllocator::new(8));
        let mut guards = Vec::new();
        for _ in 0..8 {
            guards.push(SlabAllocator::allocate_guard(&slab).unwrap());
        }
        assert!(SlabAllocator::allocate_guard(&slab).is_none());
        drop(guards.pop());
        assert!(SlabAllocator::allocate_guard(&slab).is_some());
    }

    #[test]
    fn slab_threaded_stress() {
        let slab = Arc::new(SlabAllocator::new(128));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let s = slab.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    if let Some(g) = SlabAllocator::allocate_guard(&s) {
                        unsafe {
                            let p = g.get_mut();
                            p.len = 0;
                        }
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let mut guards = Vec::new();
        while let Some(g) = SlabAllocator::allocate_guard(&slab) {
            guards.push(g);
            if guards.len() > 1000 { break; }
        }
        assert!(guards.len() <= 128);
    }
}

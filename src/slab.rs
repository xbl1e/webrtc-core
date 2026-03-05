use crate::packet::AudioPacket;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlabKey {
    index: usize,
    generation: u64,
}

impl SlabKey {
    pub const fn new(index: usize, generation: u64) -> Self {
        Self { index, generation }
    }

    pub const fn index(&self) -> usize {
        self.index
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

pub struct SlabAllocator {
    slots: Box<[UnsafeCell<MaybeUninit<AudioPacket>>]>,
    generations: Box<[AtomicU64]>,
    next: Box<[AtomicUsize]>,
    head: Mutex<usize>,
    allocated: AtomicUsize,
    capacity: usize,
}

unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

pub struct SlabGuard {
    slab: Arc<SlabAllocator>,
    key: SlabKey,
    active: AtomicBool,
}

impl SlabAllocator {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two();
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(UnsafeCell::new(MaybeUninit::uninit()));
        }
        let mut g = Vec::with_capacity(cap);
        for _ in 0..cap {
            g.push(AtomicU64::new(1));
        }
        let mut n = Vec::with_capacity(cap);
        for i in 0..cap {
            n.push(AtomicUsize::new(i + 1));
        }
        n[cap - 1].store(usize::MAX, Ordering::Relaxed);
        Self {
            slots: v.into_boxed_slice(),
            generations: g.into_boxed_slice(),
            next: n.into_boxed_slice(),
            head: Mutex::new(0),
            allocated: AtomicUsize::new(0),
            capacity: cap,
        }
    }

    pub fn allocate(&self) -> Option<SlabKey> {
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

        let mut guard = self.head.lock().ok()?;
        let head = *guard;
        if head == usize::MAX {
            self.allocated.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        let next = self.next[head].load(Ordering::Acquire);
        *guard = next;
        let generation = self.generations[head].load(Ordering::Acquire);
        Some(SlabKey::new(head, generation))
    }

    pub fn allocate_guard(arc: &Arc<Self>) -> Option<SlabGuard> {
        let key = arc.allocate()?;
        Some(SlabGuard {
            slab: arc.clone(),
            key,
            active: AtomicBool::new(true),
        })
    }

    pub fn free(&self, key: SlabKey) {
        if key.index() >= self.capacity {
            return;
        }
        let mut guard = self.head.lock().ok();
        if guard.is_none() {
            return;
        }
        let mut guard = guard.unwrap();
        let head = *guard;
        self.generations[key.index()].fetch_add(1, Ordering::AcqRel);
        self.next[key.index()].store(head, Ordering::Release);
        *guard = key.index();
        self.allocated.fetch_sub(1, Ordering::AcqRel);
    }

    pub fn get_mut(&self, key: &SlabKey) -> Option<&mut AudioPacket> {
        if key.index() >= self.capacity {
            return None;
        }
        let current_gen = self.generations[key.index()].load(Ordering::Acquire);
        if current_gen != key.generation() {
            return None;
        }
        Some(unsafe { &mut *((*self.slots.get_unchecked(key.index()).get()).as_mut_ptr()) })
    }

    pub unsafe fn get_mut_unchecked(&self, key: SlabKey) -> &mut AudioPacket {
        &mut *((*self.slots.get_unchecked(key.index()).get()).as_mut_ptr())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn allocated_count(&self) -> usize {
        self.allocated.load(Ordering::Acquire)
    }
}

impl SlabGuard {
    pub fn key(&self) -> SlabKey {
        self.key
    }

    pub fn get_mut(&self) -> Option<&mut AudioPacket> {
        self.slab.get_mut(&self.key)
    }

    pub unsafe fn get_mut_unchecked(&self) -> &mut AudioPacket {
        self.slab.get_mut_unchecked(self.key)
    }

    pub fn into_key(self) -> SlabKey {
        let key = self.key;
        std::mem::forget(self);
        key
    }
}

impl Drop for SlabGuard {
    fn drop(&mut self) {
        if self.active.load(Ordering::Acquire) {
            self.slab.free(self.key);
        }
    }
}

impl std::fmt::Debug for SlabGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SlabGuard({:?})", self.key)
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
                        if let Some(p) = g.get_mut() {
                            p.len = 0;
                        }
                    }
                }
            }));
        }
        for h in handles {
            h.join().ok();
        }

        let mut guards = Vec::new();
        while let Some(g) = SlabAllocator::allocate_guard(&slab) {
            guards.push(g);
            if guards.len() > 1000 {
                break;
            }
        }
        assert!(guards.len() <= 128);
    }

    #[test]
    fn slab_generation_prevents_use_after_free() {
        let slab = SlabAllocator::new(4);
        let key1 = slab.allocate().unwrap();
        let key1_gen = key1.generation();

        slab.free(key1);

        let key2 = slab.allocate().unwrap();
        assert_eq!(key2.index(), key1.index());
        assert!(key2.generation() > key1_gen);

        assert!(slab.get_mut(&key1).is_none());
        assert!(slab.get_mut(&key2).is_some());
    }

    #[test]
    fn slab_double_free_handling() {
        let slab = SlabAllocator::new(4);
        let key = slab.allocate().unwrap();

        slab.free(key);
        slab.free(key);

        let new_key = slab.allocate().unwrap();
        assert_eq!(new_key.index(), key.index());
        assert!(new_key.generation() > key.generation());
    }
}

use crossbeam_utils::CachePadded;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use super::frame::{VideoCodec, VideoFrame, VideoFrameType};

const MAX_VIDEO_FRAME_SIZE: usize = 262144;
const FRAME_POOL_CAPACITY: usize = 64;

pub struct VideoSlot {
    occupied: AtomicBool,
    frame_meta: UnsafeCell<VideoFrame>,
    data: UnsafeCell<[u8; MAX_VIDEO_FRAME_SIZE]>,
    data_len: AtomicUsize,
}

unsafe impl Send for VideoSlot {}
unsafe impl Sync for VideoSlot {}

impl VideoSlot {
    fn new() -> Self {
        Self {
            occupied: AtomicBool::new(false),
            frame_meta: UnsafeCell::new(VideoFrame::default()),
            data: UnsafeCell::new([0u8; MAX_VIDEO_FRAME_SIZE]),
            data_len: AtomicUsize::new(0),
        }
    }
}

pub struct VideoFrameBuffer {
    slots: Box<[VideoSlot]>,
    capacity: usize,
    mask: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
    max_decode_queue: usize,
}

unsafe impl Send for VideoFrameBuffer {}
unsafe impl Sync for VideoFrameBuffer {}

impl VideoFrameBuffer {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().min(FRAME_POOL_CAPACITY);
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(VideoSlot::new());
        }
        Self {
            slots: slots.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            max_decode_queue: cap,
        }
    }

    pub fn push_frame(&self, frame: VideoFrame, data: &[u8]) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= self.max_decode_queue {
            return false;
        }
        let slot_idx = tail & self.mask;
        let slot = &self.slots[slot_idx];
        if slot.occupied.load(Ordering::Acquire) {
            return false;
        }
        let copy_len = data.len().min(MAX_VIDEO_FRAME_SIZE);
        unsafe {
            let dst = &mut *slot.data.get();
            dst[..copy_len].copy_from_slice(&data[..copy_len]);
            *slot.frame_meta.get() = frame;
        }
        slot.data_len.store(copy_len, Ordering::Relaxed);
        slot.occupied.store(true, Ordering::Release);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn pop_frame<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&VideoFrame, &[u8]),
    {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head == tail {
            return false;
        }
        let slot_idx = head & self.mask;
        let slot = &self.slots[slot_idx];
        if !slot.occupied.load(Ordering::Acquire) {
            return false;
        }
        let len = slot.data_len.load(Ordering::Relaxed);
        unsafe {
            let frame = &*slot.frame_meta.get();
            let data = std::slice::from_raw_parts((*slot.data.get()).as_ptr(), len);
            f(frame, data);
        }
        slot.occupied.store(false, Ordering::Release);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Relaxed);
        tail.wrapping_sub(head)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

pub struct RtpFragmentSlot {
    pub seq: u16,
    pub timestamp: u32,
    pub marker: bool,
    pub len: usize,
    pub data: [u8; 1500],
}

pub struct FrameAssembler {
    fragments: Box<[UnsafeCell<RtpFragmentSlot>]>,
    fragment_valid: Box<[AtomicBool]>,
    capacity: usize,
    mask: usize,
    current_ts: AtomicU32,
    fragment_count: AtomicUsize,
    codec: VideoCodec,
}

unsafe impl Send for FrameAssembler {}
unsafe impl Sync for FrameAssembler {}

impl FrameAssembler {
    pub fn new(capacity: usize, codec: VideoCodec) -> Self {
        let cap = capacity.next_power_of_two().min(512);
        let mut frags = Vec::with_capacity(cap);
        let mut valid = Vec::with_capacity(cap);
        for _ in 0..cap {
            frags.push(UnsafeCell::new(RtpFragmentSlot {
                seq: 0,
                timestamp: 0,
                marker: false,
                len: 0,
                data: [0u8; 1500],
            }));
            valid.push(AtomicBool::new(false));
        }
        Self {
            fragments: frags.into_boxed_slice(),
            fragment_valid: valid.into_boxed_slice(),
            capacity: cap,
            mask: cap - 1,
            current_ts: AtomicU32::new(0),
            fragment_count: AtomicUsize::new(0),
            codec,
        }
    }

    pub fn push_rtp_fragment(&self, seq: u16, timestamp: u32, marker: bool, payload: &[u8]) -> bool {
        let cur_ts = self.current_ts.load(Ordering::Acquire);
        if cur_ts != 0 && timestamp != cur_ts {
            return false;
        }
        if cur_ts == 0 {
            let _ = self.current_ts.compare_exchange(0, timestamp, Ordering::AcqRel, Ordering::Acquire);
        }
        let slot_idx = (seq as usize) & self.mask;
        let slot = &self.fragment_valid[slot_idx];
        if slot.load(Ordering::Acquire) {
            return false;
        }
        let copy_len = payload.len().min(1500);
        unsafe {
            let s = &mut *self.fragments[slot_idx].get();
            s.seq = seq;
            s.timestamp = timestamp;
            s.marker = marker;
            s.len = copy_len;
            s.data[..copy_len].copy_from_slice(&payload[..copy_len]);
        }
        slot.store(true, Ordering::Release);
        self.fragment_count.fetch_add(1, Ordering::Relaxed);
        true
    }

    pub fn is_complete(&self) -> bool {
        for slot in self.fragment_valid.iter() {
            if !slot.load(Ordering::Acquire) {
                continue;
            }
            let idx = self.fragment_valid.iter().position(|s| {
                if let Ok(raw_ptr) = std::panic::catch_unwind(|| s as *const AtomicBool) {
                    raw_ptr == slot as *const AtomicBool
                } else {
                    false
                }
            });
            let _ = idx;
        }
        for i in 0..self.capacity {
            if self.fragment_valid[i].load(Ordering::Acquire) {
                let s = unsafe { &*self.fragments[i].get() };
                if s.marker {
                    return true;
                }
            }
        }
        false
    }

    pub fn assemble_into(&self, out: &mut [u8]) -> Option<(VideoFrame, usize)> {
        if !self.is_complete() {
            return None;
        }
        let mut seqs: Vec<u16> = Vec::new();
        for i in 0..self.capacity {
            if self.fragment_valid[i].load(Ordering::Acquire) {
                let s = unsafe { &*self.fragments[i].get() };
                seqs.push(s.seq);
            }
        }
        seqs.sort();

        let mut total = 0usize;
        for &seq in &seqs {
            let slot_idx = (seq as usize) & self.mask;
            let s = unsafe { &*self.fragments[slot_idx].get() };
            let hdr_skip = if self.codec == VideoCodec::Vp8 { 1 } else { 0 };
            let payload_start = hdr_skip.min(s.len);
            let plen = s.len - payload_start;
            if total + plen > out.len() {
                return None;
            }
            out[total..total + plen].copy_from_slice(&s.data[payload_start..s.len]);
            total += plen;
        }

        let ts = self.current_ts.load(Ordering::Relaxed);
        let frame = VideoFrame {
            codec: self.codec,
            rtp_timestamp: ts,
            is_complete: true,
            payload_len: total,
            ..Default::default()
        };
        Some((frame, total))
    }

    pub fn reset(&self) {
        for slot in self.fragment_valid.iter() {
            slot.store(false, Ordering::Relaxed);
        }
        self.current_ts.store(0, Ordering::Relaxed);
        self.fragment_count.store(0, Ordering::Relaxed);
    }

    pub fn detect_keyframe_vp8(&self) -> bool {
        for i in 0..self.capacity {
            if self.fragment_valid[i].load(Ordering::Acquire) {
                let s = unsafe { &*self.fragments[i].get() };
                if s.len >= 4 {
                    let s_bit = (s.data[0] >> 4) & 0x1;
                    if s_bit == 0 {
                        let p_bit = s.data[3] & 0x01;
                        return p_bit == 0;
                    }
                }
            }
        }
        false
    }
}

impl VideoFrameType {
    pub fn from_vp8_payload(data: &[u8]) -> Self {
        if data.len() >= 4 {
            let partition_zero = (data[0] >> 4) & 0x1 == 0;
            if partition_zero {
                let p_bit = data[3] & 0x01;
                if p_bit == 0 {
                    return VideoFrameType::Key;
                }
            }
        }
        VideoFrameType::Delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::frame::VideoFrame;

    #[test]
    fn frame_buffer_push_pop() {
        let buf = VideoFrameBuffer::new(16);
        let frame = VideoFrame::new(VideoCodec::Vp8, 90000);
        let data = vec![0xAAu8; 100];
        assert!(buf.push_frame(frame, &data));
        assert_eq!(buf.len(), 1);
        let mut called = false;
        buf.pop_frame(|f, d| {
            called = true;
            assert_eq!(f.rtp_timestamp, 90000);
            assert_eq!(d.len(), 100);
        });
        assert!(called);
        assert!(buf.is_empty());
    }

    #[test]
    fn frame_buffer_capacity_limit() {
        let buf = VideoFrameBuffer::new(4);
        let frame = VideoFrame::default();
        let data = &[0u8; 16];
        let mut count = 0;
        for _ in 0..8 {
            if buf.push_frame(frame, data) {
                count += 1;
            }
        }
        assert!(count <= buf.capacity());
    }
}

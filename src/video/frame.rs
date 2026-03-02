use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    Vp8,
    Vp9,
    H264,
    H265,
    Av1,
}

impl Default for VideoCodec {
    fn default() -> Self {
        VideoCodec::Vp8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFrameType {
    Key,
    Delta,
    B,
}

impl Default for VideoFrameType {
    fn default() -> Self {
        VideoFrameType::Delta
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VideoFrame {
    pub codec: VideoCodec,
    pub frame_type: VideoFrameType,
    pub resolution: VideoResolution,
    pub rtp_timestamp: u32,
    pub capture_time_ns: u64,
    pub spatial_layer: u8,
    pub temporal_layer: u8,
    pub ssrc: u32,
    pub seq_first: u16,
    pub seq_last: u16,
    pub is_complete: bool,
    pub payload_offset: usize,
    pub payload_len: usize,
}

impl VideoFrame {
    pub fn new(codec: VideoCodec, rtp_timestamp: u32) -> Self {
        Self {
            codec,
            rtp_timestamp,
            ..Default::default()
        }
    }

    pub fn is_keyframe(&self) -> bool {
        self.frame_type == VideoFrameType::Key
    }

    pub fn set_resolution(&mut self, width: u32, height: u32) {
        self.resolution = VideoResolution { width, height };
    }
}

pub struct FrameCounter {
    keyframes: AtomicU64,
    delta_frames: AtomicU64,
    total_bytes: AtomicU64,
}

impl FrameCounter {
    pub const fn new() -> Self {
        Self {
            keyframes: AtomicU64::new(0),
            delta_frames: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
        }
    }

    pub fn record(&self, frame: &VideoFrame, bytes: u64) {
        match frame.frame_type {
            VideoFrameType::Key => { self.keyframes.fetch_add(1, Ordering::Relaxed); }
            _ => { self.delta_frames.fetch_add(1, Ordering::Relaxed); }
        }
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn keyframe_count(&self) -> u64 {
        self.keyframes.load(Ordering::Relaxed)
    }

    pub fn delta_frame_count(&self) -> u64 {
        self.delta_frames.load(Ordering::Relaxed)
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }
}

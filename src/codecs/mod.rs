use std::sync::Arc;
use thiserror::Error;
use bytes::Bytes;

#[derive(Error, Debug)]
pub enum CodecError {
    #[error("encoding failed")]
    EncodeError,
    #[error("decoding failed")]
    DecodeError,
    #[error("unsupported codec")]
    UnsupportedCodec,
    #[error("invalid parameters")]
    InvalidParameter,
    #[error("not initialized")]
    NotInitialized,
}

pub trait VideoEncoder: Send + Sync {
    fn encode(&self, frame: &VideoFrame, force_keyframe: bool) -> Result<Vec<EncodedFrame>, CodecError>;
    fn set_bitrate(&self, bps: u32);
    fn set_frame_rate(&self, fps: f32);
    fn request_keyframe(&self);
    fn get_codec_type(&self) -> CodecType;
}

pub trait VideoDecoder: Send + Sync {
    fn decode(&self, frame: &EncodedFrame) -> Result<Option<VideoFrame>, CodecError>;
    fn get_codec_type(&self) -> CodecType;
    fn get_implementation_name(&self) -> &'static str;
}

pub trait AudioEncoder: Send + Sync {
    fn encode(&self, samples: &[i16]) -> Result<Vec<u8>, CodecError>;
    fn encode_float(&self, samples: &[f32]) -> Result<Vec<u8>, CodecError>;
    fn set_bitrate(&self, bps: i32);
    fn set_application(&self, app: AudioApplication);
    fn get_codec_type(&self) -> CodecType;
}

pub trait AudioDecoder: Send + Sync {
    fn decode(&self, packet: &[u8]) -> Result<Vec<i16>, CodecError>;
    fn decode_float(&self, packet: &[u8]) -> Result<Vec<f32>, CodecError>;
    fn get_codec_type(&self) -> CodecType;
    fn get_implementation_name(&self) -> &'static str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodecType {
    Opus,
    G722,
    Pcmu,
    Pcma,
    VP8,
    VP9,
    H264,
    AV1,
    Unknown,
}

impl CodecType {
    pub fn from_mime(mime: &str) -> Self {
        match mime.to_lowercase().as_str() {
            "audio/opus" | "opus" => CodecType::Opus,
            "audio/g722" | "g722" => CodecType::G722,
            "audio/pcmu" | "pcmu" | "audio/PCMU" => CodecType::Pcmu,
            "audio/pcma" | "pcma" | "audio/PCMA" => CodecType::Pcma,
            "video/vp8" | "vp8" => CodecType::VP8,
            "video/vp9" | "vp9" => CodecType::VP9,
            "video/h264" | "h264" | "video/H264" => CodecType::H264,
            "video/av1" | "av1" => CodecType::AV1,
            _ => CodecType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CodecType::Opus => "opus",
            CodecType::G722 => "G722",
            CodecType::Pcmu => "PCMU",
            CodecType::Pcma => "PCMA",
            CodecType::VP8 => "VP8",
            CodecType::VP9 => "VP9",
            CodecType::H264 => "H264",
            CodecType::AV1 => "AV1",
            CodecType::Unknown => "unknown",
        }
    }

    pub fn is_audio(&self) -> bool {
        matches!(self, CodecType::Opus | CodecType::G722 | CodecType::Pcmu | CodecType::Pcma)
    }

    pub fn is_video(&self) -> bool {
        matches!(self, CodecType::VP8 | CodecType::VP9 | CodecType::H264 | CodecType::AV1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioApplication {
    VoIP,
    Audiobook,
    LowDelay,
}

impl AudioApplication {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "voip" => AudioApplication::VoIP,
            "audiobook" => AudioApplication::Audiobook,
            "lowdelay" | "restricted-lowdelay" => AudioApplication::LowDelay,
            _ => AudioApplication::VoIP,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioEncoderConfig {
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate: i32,
    pub application: AudioApplication,
    pub frame_size_ms: u32,
    pub use_vbr: bool,
    pub use_constrained_vbr: bool,
    pub use_dtx: bool,
    pub use_fec: bool,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            bitrate: 64000,
            application: AudioApplication::VoIP,
            frame_size_ms: 20,
            use_vbr: true,
            use_constrained_vbr: true,
            use_dtx: false,
            use_fec: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VideoEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
    pub bitrate: u32,
    pub max_bitrate: u32,
    pub min_bitrate: u32,
    pub target_bitrate: u32,
    pub keyframe_interval_ms: u32,
    pub num_temporal_layers: u8,
    pub num_spatial_layers: u8,
    pub denoising: bool,
    pub automatic_restore: bool,
    pub complexity: VideoComplexity,
}

impl Default for VideoEncoderConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            frame_rate: 30.0,
            bitrate: 2500000,
            max_bitrate: 5000000,
            min_bitrate: 300000,
            target_bitrate: 2500000,
            keyframe_interval_ms: 0,
            num_temporal_layers: 1,
            num_spatial_layers: 1,
            denoising: false,
            automatic_restore: true,
            complexity: VideoComplexity::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoComplexity {
    Low,
    Normal,
    High,
    VeryHigh,
}

#[derive(Clone, Debug)]
pub struct VideoDecoderConfig {
    pub width: u32,
    pub height: u32,
    pub num_temporal_layers: u8,
    pub num_spatial_layers: u8,
    pub hardware_acceleration: bool,
}

impl Default for VideoDecoderConfig {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            num_temporal_layers: 1,
            num_spatial_layers: 1,
            hardware_acceleration: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub data: Bytes,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: u64,
    pub rotation: VideoRotation,
    pub color_space: Option<ColorSpace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoRotation {
    Rotation0,
    Rotation90,
    Rotation180,
    Rotation270,
}

impl VideoRotation {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => VideoRotation::Rotation0,
            90 => VideoRotation::Rotation90,
            180 => VideoRotation::Rotation180,
            270 => VideoRotation::Rotation270,
            _ => VideoRotation::Rotation0,
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            VideoRotation::Rotation0 => 0,
            VideoRotation::Rotation90 => 90,
            VideoRotation::Rotation180 => 180,
            VideoRotation::Rotation270 => 270,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ColorSpace {
    pub primaries: ColorPrimaries,
    pub transfer: ColorTransfer,
    pub matrix: ColorMatrix,
    pub range: ColorRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPrimaries {
    BT709,
    BT601,
    BT2020,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTransfer {
    BT709,
    BT601,
    SMPTE240M,
    Linear,
    Log,
    LogSqrt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMatrix {
    BT709,
    BT601,
    BT2020,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRange {
    Full,
    Limited,
}

#[derive(Clone, Debug)]
pub struct EncodedFrame {
    pub data: Bytes,
    pub keyframe: bool,
    pub timestamp: u32,
    pub codec_type: CodecType,
    pub width: u32,
    pub height: u32,
    pub temporal_layer: u8,
    pub spatial_layer: u8,
    pub qp: Option<u32>,
    pub ntp_time_ms: Option<i64>,
    pub packet_info: PacketInfo,
}

#[derive(Clone, Debug, Default)]
pub struct PacketInfo {
    pub is_retransmission: bool,
    pub is_fec: bool,
    pub simulated: bool,
}

impl VideoFrame {
    pub fn new_i420(data: Bytes, width: u32, height: u32) -> Self {
        Self {
            data,
            width,
            height,
            timestamp_us: 0,
            rotation: VideoRotation::Rotation0,
            color_space: None,
        }
    }

    pub fn new_argb(data: Bytes, width: u32, height: u32) -> Self {
        Self {
            data,
            width,
            height,
            timestamp_us: 0,
            rotation: VideoRotation::Rotation0,
            color_space: None,
        }
    }
}

impl EncodedFrame {
    pub fn new_vp8(data: Bytes, keyframe: bool, timestamp: u32) -> Self {
        Self {
            data,
            keyframe,
            timestamp,
            codec_type: CodecType::VP8,
            width: 0,
            height: 0,
            temporal_layer: 0,
            spatial_layer: 0,
            qp: None,
            ntp_time_ms: None,
            packet_info: PacketInfo::default(),
        }
    }

    pub fn new_vp9(data: Bytes, keyframe: bool, timestamp: u32, spatial: u8, temporal: u8) -> Self {
        Self {
            data,
            keyframe,
            timestamp,
            codec_type: CodecType::VP9,
            width: 0,
            height: 0,
            temporal_layer: temporal,
            spatial_layer: spatial,
            qp: None,
            ntp_time_ms: None,
            packet_info: PacketInfo::default(),
        }
    }

    pub fn new_h264(data: Bytes, keyframe: bool, timestamp: u32) -> Self {
        Self {
            data,
            keyframe,
            timestamp,
            codec_type: CodecType::H264,
            width: 0,
            height: 0,
            temporal_layer: 0,
            spatial_layer: 0,
            qp: None,
            ntp_time_ms: None,
            packet_info: PacketInfo::default(),
        }
    }
}

pub struct CodecRegistry {
    video_encoders: parking_lot::Mutex<Vec<(CodecType, fn() -> Box<dyn VideoEncoder>)>>,
    video_decoders: parking_lot::Mutex<Vec<(CodecType, fn() -> Box<dyn VideoDecoder>)>>,
    audio_encoders: parking_lot::Mutex<Vec<(CodecType, fn() -> Box<dyn AudioEncoder>)>>,
    audio_decoders: parking_lot::Mutex<Vec<(CodecType, fn() -> Box<dyn AudioDecoder>)>>,
}

impl CodecRegistry {
    pub fn new() -> Self {
        Self {
            video_encoders: parking_lot::Mutex::new(Vec::new()),
            video_decoders: parking_lot::Mutex::new(Vec::new()),
            audio_encoders: parking_lot::Mutex::new(Vec::new()),
            audio_decoders: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn register_video_encoder(&self, codec: CodecType, factory: fn() -> Box<dyn VideoEncoder>) {
        self.video_encoders.lock().push((codec, factory));
    }

    pub fn register_video_decoder(&self, codec: CodecType, factory: fn() -> Box<dyn VideoDecoder>) {
        self.video_decoders.lock().push((codec, factory));
    }

    pub fn register_audio_encoder(&self, codec: CodecType, factory: fn() -> Box<dyn AudioEncoder>) {
        self.audio_encoders.lock().push((codec, factory));
    }

    pub fn register_audio_decoder(&self, codec: CodecType, factory: fn() -> Box<dyn AudioDecoder>) {
        self.audio_decoders.lock().push((codec, factory));
    }

    pub fn create_video_encoder(&self, codec: CodecType) -> Option<Box<dyn VideoEncoder>> {
        let encoders = self.video_encoders.lock();
        encoders.iter()
            .find(|(c, _)| *c == codec)
            .map(|(_, f)| f())
    }

    pub fn create_video_decoder(&self, codec: CodecType) -> Option<Box<dyn VideoDecoder>> {
        let decoders = self.video_decoders.lock();
        decoders.iter()
            .find(|(c, _)| *c == codec)
            .map(|(_, f)| f())
    }

    pub fn create_audio_encoder(&self, codec: CodecType) -> Option<Box<dyn AudioEncoder>> {
        let encoders = self.audio_encoders.lock();
        encoders.iter()
            .find(|(c, _)| *c == codec)
            .map(|(_, f)| f())
    }

    pub fn create_audio_decoder(&self, codec: CodecType) -> Option<Box<dyn AudioDecoder>> {
        let decoders = self.audio_decoders.lock();
        decoders.iter()
            .find(|(c, _)| *c == codec)
            .map(|(_, f)| f())
    }

    pub fn supported_video_codecs(&self) -> Vec<CodecType> {
        self.video_encoders.lock().iter().map(|(c, _)| *c).collect()
    }

    pub fn supported_audio_codecs(&self) -> Vec<CodecType> {
        self.audio_encoders.lock().iter().map(|(c, _)| *c).collect()
    }
}

impl Default for CodecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn global_registry() -> &'static CodecRegistry {
    static REGISTRY: std::sync::OnceLock<CodecRegistry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let reg = CodecRegistry::new();
        reg
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_type_from_mime() {
        assert_eq!(CodecType::from_mime("opus"), CodecType::Opus);
        assert_eq!(CodecType::from_mime("VP8"), CodecType::VP8);
        assert_eq!(CodecType::from_mime("h264"), CodecType::H264);
    }

    #[test]
    fn audio_encoder_config_default() {
        let config = AudioEncoderConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 2);
    }

    #[test]
    fn video_encoder_config_default() {
        let config = VideoEncoderConfig::default();
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
    }

    #[test]
    fn codec_registry() {
        let registry = CodecRegistry::new();
        let codecs = registry.supported_video_codecs();
        assert!(codecs.is_empty());
    }
}

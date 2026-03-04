pub mod sframe;

/// End-to-end encryption using SFrame (Secure Frame).
///
/// NOTE: This module implements the SFrame encryption but is not yet
/// integrated into the RTP pipeline. To use, integrate with the
/// packetization/depacketization layer.
pub use sframe::{SFrameContext, SFrameError, SFrameConfig, KeyStore};

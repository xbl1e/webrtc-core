use std::sync::Arc;
use parking_lot::Mutex;
use thiserror::Error;
use bytes::Bytes;

#[derive(Error, Debug)]
pub enum AudioProcessingError {
    #[error("processing failed")]
    ProcessingFailed,
    #[error("not initialized")]
    NotInitialized,
    #[error("invalid parameter")]
    InvalidParameter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AecMode {
    Disabled,
    Mild,
    Moderate,
    Aggressive,
    VeryAggressive,
}

impl AecMode {
    pub fn from_level(level: u32) -> Self {
        match level {
            0 => AecMode::Disabled,
            1 => AecMode::Mild,
            2 => AecMode::Moderate,
            3 => AecMode::Aggressive,
            _ => AecMode::VeryAggressive,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseSuppressionLevel {
    Disabled,
    Low,
    Moderate,
    High,
}

impl NoiseSuppressionLevel {
    pub fn from_level(level: u32) -> Self {
        match level {
            0 => NoiseSuppressionLevel::Disabled,
            1 => NoiseSuppressionLevel::Low,
            2 => NoiseSuppressionLevel::Moderate,
            _ => NoiseSuppressionLevel::High,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgcMode {
    Disabled,
    AdaptiveAnalog,
    AdaptiveDigital,
    FixedDigital,
}

impl AgcMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "adaptive-analog" | "adaptive_analog" => AgcMode::AdaptiveAnalog,
            "adaptive-digital" | "adaptive_digital" => AgcMode::AdaptiveDigital,
            "fixed-digital" | "fixed_digital" => AgcMode::FixedDigital,
            _ => AgcMode::Disabled,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioProcessingConfig {
    pub aec_enabled: bool,
    pub aec_mode: AecMode,
    pub aec_suppression_level: u32,
    pub noise_suppression_enabled: bool,
    pub noise_suppression_level: NoiseSuppressionLevel,
    pub agc_enabled: bool,
    pub agc_mode: AgcMode,
    pub agc_target_level: i32,
    pub agc_compression_gain: i32,
    pub high_pass_filter_enabled: bool,
    pub voice_detection_enabled: bool,
    pub transient_suppression_enabled: bool,
    pub echo_controllers_enabled: bool,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            aec_enabled: true,
            aec_mode: AecMode::Moderate,
            aec_suppression_level: 2,
            noise_suppression_enabled: true,
            noise_suppression_level: NoiseSuppressionLevel::Moderate,
            agc_enabled: true,
            agc_mode: AgcMode::AdaptiveDigital,
            agc_target_level: 3,
            agc_compression_gain: 9,
            high_pass_filter_enabled: true,
            voice_detection_enabled: false,
            transient_suppression_enabled: false,
            echo_controllers_enabled: true,
        }
    }
}

pub struct AudioFrame {
    pub data: Bytes,
    pub sample_rate: u32,
    pub channels: u32,
    pub samples_per_channel: usize,
    pub timestamp_us: u64,
}

impl AudioFrame {
    pub fn new(samples: Vec<i16>, sample_rate: u32, channels: u32) -> Self {
        let samples_per_channel = samples.len() / channels as usize;
        let data = Bytes::from(samples);
        Self {
            data,
            sample_rate,
            channels,
            samples_per_channel,
            timestamp_us: 0,
        }
    }

    pub fn from_float(samples: &[f32], sample_rate: u32, channels: u32) -> Self {
        let samples_i16: Vec<i16> = samples.iter()
            .map(|s| (*s * 32767.0) as i16)
            .collect();
        Self::new(samples_i16, sample_rate, channels)
    }

    pub fn to_float(&self) -> Vec<f32> {
        let samples: &[i16] = unsafe {
            std::slice::from_raw_parts(
                self.data.as_ptr() as *const i16,
                self.data.len() / 2,
            )
        };
        samples.iter().map(|s| *s as f32 / 32768.0).collect()
    }

    pub fn duration_ms(&self) -> f64 {
        (self.samples_per_channel as f64) / (self.sample_rate as f64) * 1000.0
    }
}

pub struct AudioProcessingPipeline {
    config: Mutex<AudioProcessingConfig>,
    stream_delay_ms: Mutex<i32>,
    sample_rate: u32,
    channels: u32,
    frame_size_ms: u32,
    aec_state: Mutex<Option<AecState>>,
    ns_state: Mutex<Option<NoiseSuppressionState>>,
    agc_state: Mutex<Option<AgcState>>,
    hpf_state: Mutex<HighPassFilterState>,
}

struct AecState {
    buffer_capture: Vec<f32>,
    buffer_render: Vec<f32>,
    delay_estimate: i32,
}

struct NoiseSuppressionState {
    level: NoiseSuppressionLevel,
    buffer: Vec<f32>,
}

struct AgcState {
    mode: AgcMode,
    target_level: i32,
    compression_gain: i32,
    level_estimate: f32,
}

struct HighPassFilterState {
    enabled: bool,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl AudioProcessingPipeline {
    pub fn new(sample_rate: u32, channels: u32) -> Self {
        let frame_size_ms = 10;
        
        Self {
            config: Mutex::new(AudioProcessingConfig::default()),
            stream_delay_ms: Mutex::new(0),
            sample_rate,
            channels,
            frame_size_ms,
            aec_state: Mutex::new(None),
            ns_state: Mutex::new(None),
            agc_state: Mutex::new(None),
            hpf_state: Mutex::new(HighPassFilterState::new(sample_rate)),
        }
    }

    pub fn set_config(&self, config: AudioProcessingConfig) {
        *self.config.lock() = config;
    }

    pub fn config(&self) -> AudioProcessingConfig {
        self.config.lock().clone()
    }

    pub fn set_stream_delay_ms(&self, delay_ms: i32) {
        *self.stream_delay_ms.lock() = delay_ms;
    }

    pub fn set_sample_rate(&self, sample_rate: u32) {
        self.hpf_state.lock().update_sample_rate(sample_rate);
    }

    pub fn process_capture(&self, frame: &mut AudioFrame) -> Result<(), AudioProcessingError> {
        let config = self.config.lock().clone();
        
        if config.high_pass_filter_enabled {
            let mut hpf = self.hpf_state.lock();
            hpf.process_frame(frame);
        }
        
        if config.noise_suppression_enabled {
            self.process_noise_suppression(frame, &config.noise_suppression_level);
        }
        
        if config.agc_enabled {
            self.process_agc(frame, &config.agc_mode, config.agc_target_level);
        }
        
        Ok(())
    }

    pub fn process_render(&self, frame: &AudioFrame) -> Result<(), AudioProcessingError> {
        let config = self.config.lock().clone();
        
        if config.aec_enabled {
            let mut aec = self.aec_state.lock();
            if aec.is_none() {
                *aec = Some(AecState::new(self.sample_rate, self.channels));
            }
            if let Some(ref mut state) = *aec {
                state.buffer_render = frame.to_float();
            }
        }
        
        Ok(())
    }

    pub fn analyze_reverse(&self, frame: &AudioFrame) -> Result<(), AudioProcessingError> {
        let config = self.config.lock().clone();
        
        if config.aec_enabled {
            let mut aec = self.aec_state.lock();
            if let Some(ref mut state) = *aec {
                state.buffer_render = frame.to_float();
            }
        }
        
        Ok(())
    }

    fn process_noise_suppression(&self, frame: &mut AudioFrame, level: &NoiseSuppressionLevel) {
        if *level == NoiseSuppressionLevel::Disabled {
            return;
        }
        
        let mut samples = frame.to_float();
        
        let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        let db = if energy > 0.0 { 10.0 * energy.log10() } else { -100.0 };
        
        match level {
            NoiseSuppressionLevel::Low => {}
            NoiseSuppressionLevel::Moderate => {
                if db < -25.0 {
                    for s in samples.iter_mut() {
                        *s *= 0.5;
                    }
                }
            }
            NoiseSuppressionLevel::High => {
                if db < -20.0 {
                    for s in samples.iter_mut() {
                        *s *= 0.3;
                    }
                }
            }
            _ => {}
        }
        
        let new_i16: Vec<i16> = samples.iter().map(|s| (*s * 32767.0) as i16).collect();
        frame.data = Bytes::from(new_i16);
    }

    fn process_agc(&self, frame: &mut AudioFrame, mode: &AgcMode, target_level: i32) {
        if *mode == AgcMode::Disabled {
            return;
        }
        
        let mut samples = frame.to_float();
        
        let peak: f32 = samples.iter().map(|s| s.abs()).fold(0.0f32, |a, b| a.max(b));
        
        if peak > 0.0 {
            let target = match target_level {
                0 => 0.125,
                1 => 0.25,
                2 => 0.5,
                3 => 1.0,
                _ => 1.0,
            };
            
            let gain = if mode == &AgcMode::FixedDigital {
                target / peak
            } else {
                (target / peak).min(12.0)
            };
            
            for s in samples.iter_mut() {
                *s = (*s * gain).clamp(-1.0, 1.0);
            }
        }
        
        let new_i16: Vec<i16> = samples.iter().map(|s| (*s * 32767.0) as i16).collect();
        frame.data = Bytes::from(new_i16);
    }
}

impl HighPassFilterState {
    fn new(sample_rate: u32) -> Self {
        let cutoff = 80.0;
        let w = 2.0 * std::f32::consts::PI * cutoff / sample_rate as f32;
        let alpha = (w.sin()) / (2.0 * 0.707 + w.cos());
        
        let b0 = (1.0 + w.cos()) / 2.0;
        let b1 = -(1.0 + w.cos());
        let b2 = (1.0 + w.cos()) / 2.0;
        let a1 = -2.0 * w.cos();
        let a2 = 1.0 - alpha;
        
        Self {
            enabled: true,
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn update_sample_rate(&mut self, sample_rate: u32) {
        *self = Self::new(sample_rate);
    }

    fn process_frame(&mut self, frame: &mut AudioFrame) {
        if !self.enabled {
            return;
        }
        
        let mut samples = frame.to_float();
        
        let filtered: Vec<f32> = samples.iter().enumerate().map(|(i, &x)| {
            let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 
                  - self.a1 * self.y1 - self.a2 * self.y2;
            
            self.x2 = self.x1;
            self.x1 = x;
            self.y2 = self.y1;
            self.y1 = y;
            
            y
        }).collect();
        
        frame.data = Bytes::from(filtered.iter().map(|s| (*s * 32767.0) as i16).collect::<Vec<_>>());
    }
}

pub struct AudioCapture {
    sample_rate: u32,
    channels: u32,
    processing: Option<Arc<AudioProcessingPipeline>>,
}

impl AudioCapture {
    pub fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            sample_rate,
            channels,
            processing: None,
        }
    }

    pub fn enable_processing(&self) -> Arc<AudioProcessingPipeline> {
        if let Some(ref p) = self.processing {
            return p.clone();
        }
        
        let pipeline = Arc::new(AudioProcessingPipeline::new(self.sample_rate, self.channels));
        pipeline
    }

    pub fn set_processing(&mut self, pipeline: Arc<AudioProcessingPipeline>) {
        self.processing = Some(pipeline);
    }
}

pub struct AudioRender {
    sample_rate: u32,
    channels: u32,
    processing: Option<Arc<AudioProcessingPipeline>>,
}

impl AudioRender {
    pub fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            sample_rate,
            channels,
            processing: None,
        }
    }

    pub fn set_processing(&mut self, pipeline: Arc<AudioProcessingPipeline>) {
        self.processing = Some(pipeline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_processing_pipeline_creation() {
        let pipeline = AudioProcessingPipeline::new(48000, 2);
        let config = pipeline.config();
        assert!(config.aec_enabled);
    }

    #[test]
    fn audio_frame_creation() {
        let samples = vec![0i16; 480];
        let frame = AudioFrame::new(samples, 48000, 2);
        assert_eq!(frame.sample_rate, 48000);
        assert_eq!(frame.channels, 2);
    }

    #[test]
    fn audio_processing_config_default() {
        let config = AudioProcessingConfig::default();
        assert_eq!(config.aec_mode, AecMode::Moderate);
        assert_eq!(config.noise_suppression_level, NoiseSuppressionLevel::Moderate);
    }

    #[test]
    fn high_pass_filter() {
        let mut hpf = HighPassFilterState::new(48000);
        let samples = vec![0i16; 480];
        let mut frame = AudioFrame::new(samples, 48000, 2);
        hpf.process_frame(&mut frame);
    }
}

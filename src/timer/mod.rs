//! Timer wheel implementation for efficient timer management.
//!
//! This module provides a hierarchical timing wheel (hash wheel) for
//! O(1) timer operations, suitable for ICE retransmissions, DTLS handshakes,
//! and other time-based events in WebRTC.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::sync::Arc;
use parking_lot::Mutex;

/// A handle to a scheduled timer that can be used to cancel it.
#[derive(Clone, Debug)]
pub struct TimerHandle {
    id: u64,
    wheel_index: usize,
}

impl TimerHandle {
    pub(crate) fn new(id: u64, wheel_index: usize) -> Self {
        Self { id, wheel_index }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

/// Entry in the timer wheel.
struct TimerEntry {
    id: u64,
    deadline: Instant,
    callback: Box<dyn FnOnce() + Send + 'static>,
}

impl TimerEntry {
    fn new(id: u64, deadline: Instant, callback: Box<dyn FnOnce() + Send + 'static>) -> Self {
        Self { id, deadline, callback }
    }
}

/// A hierarchical timing wheel for O(1) timer operations.
///
/// Uses multiple wheels at different granularities to handle timers
/// with varying delays efficiently.
pub struct TimerWheel {
    tick_duration: Duration,
    wheels: Vec<Vec<VecDeque<TimerEntry>>>,
    wheel_durations: Vec<Duration>,
    current_time: Instant,
    current_tick: u64,
    next_id: u64,
    pending_callbacks: Mutex<Vec<Box<dyn FnOnce() + Send + 'static>>>,
}

impl TimerWheel {
    /// Create a new timer wheel with the given tick duration.
    ///
    /// # Arguments
    /// * `tick_duration` - Duration of each tick (typically 1-100ms)
    /// * `wheel_count` - Number of wheels (typically 4-6)
    /// * `wheel_sizes` - Size of each wheel
    ///
    /// # Example
    /// ```
    /// use std::time::Duration;
    /// use webrtc_core::timer::TimerWheel;
    ///
    /// let wheel = TimerWheel::new(
    ///     Duration::from_millis(10),  // 10ms tick
    ///     4,                          // 4 wheels
    ///     &[256, 64, 64, 64],         // wheel sizes
    /// );
    /// ```
    pub fn new(tick_duration: Duration, wheel_count: usize, wheel_sizes: &[usize]) -> Self {
        let mut wheels = Vec::with_capacity(wheel_count);
        let mut wheel_durations = Vec::with_capacity(wheel_count);

        let mut duration = tick_duration;
        for size in wheel_sizes.iter().take(wheel_count) {
            wheels.push(vec![VecDeque::new(); *size]);
            wheel_durations.push(duration);
            duration = duration.mul_f64(*wheel_sizes[wheel_sizes.len().max(1) - 1] as f64);
        }

        Self {
            tick_duration,
            wheels,
            wheel_durations,
            current_time: Instant::now(),
            current_tick: 0,
            next_id: 0,
            pending_callbacks: Mutex::new(Vec::new()),
        }
    }

    /// Create a default timer wheel suitable for WebRTC use cases.
    pub fn default_wheel() -> Self {
        Self::new(
            Duration::from_millis(10), // 10ms granularity
            4,
            &[256, 64, 64, 64],
        )
    }

    /// Schedule a callback to be called after the given delay.
    ///
    /// Returns a handle that can be used to cancel the timer.
    pub fn schedule(&mut self, delay: Duration, callback: impl FnOnce() + Send + 'static) -> TimerHandle {
        let id = self.next_id;
        self.next_id += 1;

        let deadline = self.current_time + delay;
        let ticks_until_deadline = (delay.as_nanos() / self.tick_duration.as_nanos() as u128) as u64;

        let (wheel_index, tick_offset) = self.compute_wheel_position(ticks_until_deadline);

        let entry = TimerEntry::new(id, deadline, Box::new(callback));
        self.wheels[wheel_index][tick_offset].push_back(entry);

        TimerHandle::new(id, wheel_index)
    }

    /// Cancel a previously scheduled timer.
    ///
    /// Returns true if the timer was found and removed.
    pub fn cancel(&mut self, handle: TimerHandle) -> bool {
        let wheel_index = handle.wheel_index;
        if wheel_index >= self.wheels.len() {
            return false;
        }

        let wheel = &mut self.wheels[wheel_index];
        for bucket in wheel.iter_mut() {
            let original_len = bucket.len();
            bucket.retain(|entry| entry.id != handle.id);
            if bucket.len() < original_len {
                return true;
            }
        }
        false
    }

    /// Advance the timer wheel and collect expired callbacks.
    ///
    /// This should be called periodically (e.g., in the timer tick interval).
    pub fn tick(&mut self) -> Vec<Box<dyn FnOnce() + Send + 'static>> {
        self.current_time = Instant::now();
        self.current_tick += 1;

        let mut callbacks = self.pending_callbacks.lock();

        // Process the first wheel (finest granularity)
        let wheel_index = 0;
        let bucket_index = (self.current_tick as usize) % self.wheels[wheel_index].len();
        let bucket = &mut self.wheels[wheel_index][bucket_index];

        while let Some(entry) = bucket.pop_front() {
            if entry.deadline <= self.current_time {
                callbacks.push(entry.callback);
            } else {
                // Re-insert with updated position
                let delay = entry.deadline.saturating_duration_since(self.current_time);
                let ticks = (delay.as_nanos() / self.tick_duration.as_nanos() as u128) as u64;
                let (new_wheel, new_offset) = self.compute_wheel_position(ticks);
                self.wheels[new_wheel][new_offset].push_back(entry);
            }
        }

        // For coarser wheels, just rotate
        for (i, wheel) in self.wheels.iter_mut().enumerate().skip(1) {
            if self.current_tick % self.wheel_durations[i].div_duration_floor(self.tick_duration) as u64 == 0 {
                let idx = ((self.current_tick / self.wheel_durations[i].div_duration_floor(self.tick_duration) as u64) as usize) % wheel.len();
                let bucket = &mut wheel[idx];
                while let Some(entry) = bucket.pop_front() {
                    if entry.deadline <= self.current_time {
                        callbacks.push(entry.callback);
                    } else {
                        let delay = entry.deadline.saturating_duration_since(self.current_time);
                        let ticks = (delay.as_nanos() / self.tick_duration.as_nanos() as u128) as u64;
                        let (new_wheel, new_offset) = self.compute_wheel_position(ticks);
                        self.wheels[new_wheel][new_offset].push_back(entry);
                    }
                }
            }
        }

        std::mem::take(&mut *callbacks)
    }

    /// Get the number of pending timers.
    pub fn pending_count(&self) -> usize {
        self.wheels.iter().flat_map(|w| w.iter()).map(|b| b.len()).sum()
    }

    fn compute_wheel_position(&self, ticks: u64) -> (usize, usize) {
        let mut remaining_ticks = ticks;
        
        for (i, duration) in self.wheel_durations.iter().enumerate() {
            let wheel_ticks = duration.as_nanos() as u64 / self.tick_duration.as_nanos() as u64;
            if remaining_ticks < wheel_ticks {
                let wheel_size = self.wheels[i].len();
                let offset = (self.current_tick + remaining_ticks) as usize % wheel_size;
                return (i, offset);
            }
            remaining_ticks /= wheel_ticks;
        }
        
        // If beyond all wheels, use the last wheel
        let last_idx = self.wheels.len() - 1;
        (last_idx, (self.current_tick as usize) % self.wheels[last_idx].len())
    }
}

impl Default for TimerWheel {
    fn default() -> Self {
        Self::default_wheel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn timer_wheel_schedule_and_tick() {
        let mut wheel = TimerWheel::default_wheel();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        wheel.schedule(Duration::from_millis(50), move || {
            called_clone.store(true, Ordering::SeqCst);
        });

        // Tick multiple times
        for _ in 0..10 {
            let callbacks = wheel.tick();
            for cb in callbacks {
                cb();
            }
        }

        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn timer_wheel_cancel() {
        let mut wheel = TimerWheel::default_wheel();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let handle = wheel.schedule(Duration::from_millis(50), move || {
            called_clone.store(true, Ordering::SeqCst);
        });

        // Cancel the timer
        assert!(wheel.cancel(handle));

        // Tick
        let callbacks = wheel.tick();
        for cb in callbacks {
            cb();
        }

        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn timer_wheel_pending_count() {
        let mut wheel = TimerWheel::default_wheel();
        
        wheel.schedule(Duration::from_millis(10), || {});
        wheel.schedule(Duration::from_millis(20), || {});
        wheel.schedule(Duration::from_millis(30), || {});

        assert_eq!(wheel.pending_count(), 3);
    }
}

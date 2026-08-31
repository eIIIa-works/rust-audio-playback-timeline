use core::fmt;
use std::collections::VecDeque;

use crate::{Generation, OutputFrame, SeekEpoch};

/// Arbitrary metadata associated with an output-frame boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedFrame<T> {
    pub generation: Generation,
    pub seek_epoch: SeekEpoch,
    pub output_frame_end: OutputFrame,
    pub payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateError {
    OutOfOrder {
        previous: OutputFrame,
        next: OutputFrame,
    },
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfOrder { previous, next } => write!(
                f,
                "timed frame moved backwards within one playback timeline: {previous} -> {next}"
            ),
        }
    }
}

impl std::error::Error for GateError {}

/// Holds metadata computed ahead of playback and releases it only when its
/// output-frame boundary has become audible.
pub struct ClockGate<T> {
    pending: VecDeque<TimedFrame<T>>,
}

impl<T> Default for ClockGate<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ClockGate<T> {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    pub fn push(&mut self, frame: TimedFrame<T>) -> Result<(), GateError> {
        if let Some(previous) = self.pending.iter().rev().find(|previous| {
            previous.generation == frame.generation && previous.seek_epoch == frame.seek_epoch
        }) {
            if frame.output_frame_end < previous.output_frame_end {
                return Err(GateError::OutOfOrder {
                    previous: previous.output_frame_end,
                    next: frame.output_frame_end,
                });
            }
        }
        self.pending.push_back(frame);
        Ok(())
    }

    /// Drop obsolete timelines and return only the newest frame that is ready.
    /// Future frames remain queued; multiple ready frames collapse to the latest
    /// so a delayed consumer does not replay a burst of stale visual state.
    pub fn take_latest_ready(
        &mut self,
        generation: Generation,
        seek_epoch: SeekEpoch,
        audible_frame: OutputFrame,
    ) -> Option<TimedFrame<T>> {
        let mut latest = None;
        loop {
            let Some(front) = self.pending.front() else {
                break;
            };

            if front.generation != generation || front.seek_epoch != seek_epoch {
                self.pending.pop_front();
                continue;
            }
            if front.output_frame_end > audible_frame {
                break;
            }
            latest = self.pending.pop_front();
        }
        latest
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

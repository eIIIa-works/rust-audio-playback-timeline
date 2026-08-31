//! Precise audible playback timeline primitives for Rust audio engines.
//!
//! This crate is intentionally not an audio engine or media player. It maps
//! backend playback timing to a monotonic audible output-frame timeline and
//! gates arbitrary timestamped metadata against that timeline.

mod gate;
mod timeline;
mod types;

pub use gate::{ClockGate, GateError, TimedFrame};
pub use timeline::{AudibleClock, ClockError, ObservationOutcome, PlaybackObservation};
pub use types::{BackendTime, Generation, OutputFrame, SeekEpoch};

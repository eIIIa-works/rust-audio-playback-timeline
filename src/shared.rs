use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use crate::timeline::{frame_for_spans, CallbackSpan};
use crate::{
    BackendTime, ClockError, Generation, ObservationOutcome, OutputFrame, PlaybackObservation,
    SeekEpoch,
};

const SNAPSHOT_ATTEMPTS: usize = 8;

/// Builder for a lock-free single-writer/multi-reader audible clock.
///
/// Call [`SharedAudibleClock::split`] once to obtain the non-cloneable writer
/// intended for the audio callback and a cloneable reader for control/UI
/// threads. The shared path uses only atomics; it never takes a mutex.
pub struct SharedAudibleClock {
    inner: Arc<SharedClockInner>,
}

impl SharedAudibleClock {
    pub fn new(
        sample_rate: u32,
        generation: Generation,
        seek_epoch: SeekEpoch,
    ) -> Result<Self, ClockError> {
        if sample_rate == 0 {
            return Err(ClockError::InvalidSampleRate);
        }

        Ok(Self {
            inner: Arc::new(SharedClockInner::new(sample_rate, generation, seek_epoch)),
        })
    }

    /// Consume the builder and create exactly one callback writer plus one
    /// cloneable reader handle.
    pub fn split(self) -> (SharedAudibleClockWriter, SharedAudibleClockReader) {
        let reader = SharedAudibleClockReader {
            inner: Arc::clone(&self.inner),
        };
        let writer = SharedAudibleClockWriter { inner: self.inner };
        (writer, reader)
    }
}

/// Single callback-side writer. This type intentionally does not implement
/// `Clone`; `observe_callback` also requires `&mut self`.
pub struct SharedAudibleClockWriter {
    inner: Arc<SharedClockInner>,
}

impl SharedAudibleClockWriter {
    /// Publish one callback's scheduled playback timestamp and contiguous valid
    /// PCM prefix. This method performs no allocation and never blocks.
    pub fn observe_callback(
        &mut self,
        observation: PlaybackObservation,
    ) -> Result<ObservationOutcome, ClockError> {
        if observation.generation != self.inner.generation
            || observation.seek_epoch != self.inner.seek_epoch
        {
            return Ok(ObservationOutcome::IgnoredStale);
        }
        if observation.output_frame_end < observation.output_frame_start {
            return Err(ClockError::InvalidSpan {
                start: observation.output_frame_start,
                end: observation.output_frame_end,
            });
        }

        let had_current = self.inner.current_valid.load(Ordering::Relaxed);
        if had_current {
            let current_playback = BackendTime(self.inner.current_playback_ns.load(Ordering::Relaxed));
            if observation.playback_time < current_playback {
                return Ok(ObservationOutcome::IgnoredRegressiveTimestamp);
            }
        }

        // Odd sequence = writer active, even sequence = coherent snapshot.
        // This handle is the sole writer, so no writer-side CAS or lock is needed.
        self.inner.sequence.fetch_add(1, Ordering::AcqRel);

        if had_current {
            self.inner.previous_playback_ns.store(
                self.inner.current_playback_ns.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.inner.previous_frame_start.store(
                self.inner.current_frame_start.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.inner.previous_frame_end.store(
                self.inner.current_frame_end.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.inner.previous_playing.store(
                self.inner.current_playing.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.inner.previous_valid.store(true, Ordering::Relaxed);
        } else {
            self.inner.previous_valid.store(false, Ordering::Relaxed);
        }

        self.inner
            .current_playback_ns
            .store(observation.playback_time.0, Ordering::Relaxed);
        self.inner
            .current_frame_start
            .store(observation.output_frame_start.0, Ordering::Relaxed);
        self.inner
            .current_frame_end
            .store(observation.output_frame_end.0, Ordering::Relaxed);
        self.inner
            .current_playing
            .store(observation.playing, Ordering::Relaxed);
        self.inner.current_valid.store(true, Ordering::Relaxed);

        self.inner.sequence.fetch_add(1, Ordering::Release);
        Ok(ObservationOutcome::Accepted)
    }
}

/// Cloneable control-side view of the callback clock.
#[derive(Clone)]
pub struct SharedAudibleClockReader {
    inner: Arc<SharedClockInner>,
}

impl SharedAudibleClockReader {
    /// Return the monotonic logical output frame audible at `now`.
    ///
    /// If a reader races the callback writer repeatedly, it falls back to the
    /// last published audible frame instead of waiting for the writer.
    pub fn audible_frame_at(&self, now: BackendTime) -> OutputFrame {
        let previous_report = OutputFrame(self.inner.reported.load(Ordering::Acquire));
        let Some(snapshot) = self.inner.snapshot() else {
            return previous_report;
        };

        let candidate = frame_for_spans(
            snapshot.current,
            snapshot.previous,
            now,
            self.inner.sample_rate,
        )
        .unwrap_or(previous_report);

        let old = self.inner.reported.fetch_max(candidate.0, Ordering::AcqRel);
        OutputFrame(old.max(candidate.0))
    }
}

struct SharedClockInner {
    sample_rate: u32,
    generation: Generation,
    seek_epoch: SeekEpoch,
    sequence: AtomicU64,

    current_valid: AtomicBool,
    current_playback_ns: AtomicU64,
    current_frame_start: AtomicU64,
    current_frame_end: AtomicU64,
    current_playing: AtomicBool,

    previous_valid: AtomicBool,
    previous_playback_ns: AtomicU64,
    previous_frame_start: AtomicU64,
    previous_frame_end: AtomicU64,
    previous_playing: AtomicBool,

    reported: AtomicU64,
}

impl SharedClockInner {
    fn new(sample_rate: u32, generation: Generation, seek_epoch: SeekEpoch) -> Self {
        Self {
            sample_rate,
            generation,
            seek_epoch,
            sequence: AtomicU64::new(0),
            current_valid: AtomicBool::new(false),
            current_playback_ns: AtomicU64::new(0),
            current_frame_start: AtomicU64::new(0),
            current_frame_end: AtomicU64::new(0),
            current_playing: AtomicBool::new(false),
            previous_valid: AtomicBool::new(false),
            previous_playback_ns: AtomicU64::new(0),
            previous_frame_start: AtomicU64::new(0),
            previous_frame_end: AtomicU64::new(0),
            previous_playing: AtomicBool::new(false),
            reported: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> Option<SharedClockSnapshot> {
        for _ in 0..SNAPSHOT_ATTEMPTS {
            let before = self.sequence.load(Ordering::Acquire);
            if before == 0 || before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }

            let current = if self.current_valid.load(Ordering::Relaxed) {
                Some(CallbackSpan {
                    playback_time: BackendTime(self.current_playback_ns.load(Ordering::Relaxed)),
                    output_frame_start: OutputFrame(
                        self.current_frame_start.load(Ordering::Relaxed),
                    ),
                    output_frame_end: OutputFrame(self.current_frame_end.load(Ordering::Relaxed)),
                    playing: self.current_playing.load(Ordering::Relaxed),
                })
            } else {
                None
            };

            let previous = if self.previous_valid.load(Ordering::Relaxed) {
                Some(CallbackSpan {
                    playback_time: BackendTime(self.previous_playback_ns.load(Ordering::Relaxed)),
                    output_frame_start: OutputFrame(
                        self.previous_frame_start.load(Ordering::Relaxed),
                    ),
                    output_frame_end: OutputFrame(self.previous_frame_end.load(Ordering::Relaxed)),
                    playing: self.previous_playing.load(Ordering::Relaxed),
                })
            } else {
                None
            };

            let after = self.sequence.load(Ordering::Acquire);
            if before == after && after & 1 == 0 {
                return Some(SharedClockSnapshot { current, previous });
            }
            std::hint::spin_loop();
        }

        None
    }
}

#[derive(Clone, Copy)]
struct SharedClockSnapshot {
    current: Option<CallbackSpan>,
    previous: Option<CallbackSpan>,
}

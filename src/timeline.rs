use core::fmt;

use crate::{BackendTime, Generation, OutputFrame, SeekEpoch};

pub(crate) const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Timing and valid-PCM extent reported for one output callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackObservation {
    pub generation: Generation,
    pub seek_epoch: SeekEpoch,
    pub callback_time: BackendTime,
    pub playback_time: BackendTime,
    /// First logical output frame represented by this callback.
    pub output_frame_start: OutputFrame,
    /// Exclusive end of the contiguous valid PCM prefix.
    /// Any hardware-buffer frames after this point are treated as silence.
    pub output_frame_end: OutputFrame,
    pub playing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationOutcome {
    Accepted,
    IgnoredStale,
    /// The backend reported a playback timestamp earlier than the last accepted
    /// callback. Ignoring the whole observation is deliberately conservative:
    /// an invalid timestamp must never make the logical audible position lead.
    IgnoredRegressiveTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    InvalidSampleRate,
    InvalidSpan {
        start: OutputFrame,
        end: OutputFrame,
    },
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => write!(f, "sample rate must be greater than zero"),
            Self::InvalidSpan { start, end } => {
                write!(f, "output frame span is reversed: {start}..{end}")
            }
        }
    }
}

impl std::error::Error for ClockError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallbackSpan {
    pub(crate) playback_time: BackendTime,
    pub(crate) output_frame_start: OutputFrame,
    pub(crate) output_frame_end: OutputFrame,
    pub(crate) playing: bool,
}

impl From<PlaybackObservation> for CallbackSpan {
    fn from(value: PlaybackObservation) -> Self {
        Self {
            playback_time: value.playback_time,
            output_frame_start: value.output_frame_start,
            output_frame_end: value.output_frame_end,
            playing: value.playing,
        }
    }
}

/// Maps backend output timing observations to a monotonic logical audible frame.
///
/// The clock deliberately keeps the current and previous callback spans. This
/// allows a query that occurs before the current callback's scheduled playback
/// instant to fall back to the previous span instead of reporting future audio.
#[derive(Debug, Clone)]
pub struct AudibleClock {
    sample_rate: u32,
    generation: Generation,
    seek_epoch: SeekEpoch,
    current: Option<CallbackSpan>,
    previous: Option<CallbackSpan>,
    reported: OutputFrame,
}

impl AudibleClock {
    pub fn new(
        sample_rate: u32,
        generation: Generation,
        seek_epoch: SeekEpoch,
    ) -> Result<Self, ClockError> {
        if sample_rate == 0 {
            return Err(ClockError::InvalidSampleRate);
        }
        Ok(Self {
            sample_rate,
            generation,
            seek_epoch,
            current: None,
            previous: None,
            reported: OutputFrame(0),
        })
    }

    /// Start a new logical playback timeline and invalidate all prior spans.
    pub fn reset(&mut self, generation: Generation, seek_epoch: SeekEpoch) {
        self.generation = generation;
        self.seek_epoch = seek_epoch;
        self.current = None;
        self.previous = None;
        self.reported = OutputFrame(0);
    }

    /// Record one callback's scheduled playback time and contiguous valid PCM span.
    pub fn observe_callback(
        &mut self,
        observation: PlaybackObservation,
    ) -> Result<ObservationOutcome, ClockError> {
        validate_observation_identity_and_span(
            observation,
            self.generation,
            self.seek_epoch,
        )?;
        if observation.generation != self.generation || observation.seek_epoch != self.seek_epoch {
            return Ok(ObservationOutcome::IgnoredStale);
        }
        if let Some(current) = self.current {
            if observation.playback_time < current.playback_time {
                return Ok(ObservationOutcome::IgnoredRegressiveTimestamp);
            }
        }

        self.previous = self.current;
        self.current = Some(observation.into());
        Ok(ObservationOutcome::Accepted)
    }

    /// Return the logical output frame audible at `now` according to the backend
    /// playback time base. The returned value never moves backwards within one
    /// generation/seek epoch.
    pub fn audible_frame_at(&mut self, now: BackendTime) -> OutputFrame {
        let candidate = frame_for_spans(self.current, self.previous, now, self.sample_rate)
            .unwrap_or(self.reported);

        if candidate > self.reported {
            self.reported = candidate;
        }
        self.reported
    }
}

pub(crate) fn validate_observation_identity_and_span(
    observation: PlaybackObservation,
    generation: Generation,
    seek_epoch: SeekEpoch,
) -> Result<(), ClockError> {
    if observation.generation != generation || observation.seek_epoch != seek_epoch {
        return Ok(());
    }
    if observation.output_frame_end < observation.output_frame_start {
        return Err(ClockError::InvalidSpan {
            start: observation.output_frame_start,
            end: observation.output_frame_end,
        });
    }
    Ok(())
}

pub(crate) fn frame_for_spans(
    current: Option<CallbackSpan>,
    previous: Option<CallbackSpan>,
    now: BackendTime,
    sample_rate: u32,
) -> Option<OutputFrame> {
    current
        .and_then(|span| frame_for_span(span, now, sample_rate))
        .or_else(|| previous.and_then(|span| frame_for_span(span, now, sample_rate)))
}

pub(crate) fn frame_for_span(
    span: CallbackSpan,
    now: BackendTime,
    sample_rate: u32,
) -> Option<OutputFrame> {
    if now < span.playback_time {
        return None;
    }
    if !span.playing || span.output_frame_end <= span.output_frame_start {
        return Some(span.output_frame_start);
    }

    let elapsed_ns = now.0.saturating_sub(span.playback_time.0);
    let elapsed_frames = ((elapsed_ns as u128 * sample_rate as u128) / NANOS_PER_SECOND)
        .min(u64::MAX as u128) as u64;
    let valid_frames = span.output_frame_end.0 - span.output_frame_start.0;

    Some(OutputFrame(
        span.output_frame_start
            .0
            .saturating_add(elapsed_frames.min(valid_frames)),
    ))
}

use cpal::{OutputCallbackInfo, StreamInstant};

use crate::{BackendTime, Generation, OutputFrame, PlaybackObservation, SeekEpoch};

/// Convert CPAL's stream-local timestamp to the backend-neutral nanosecond time base.
#[inline]
pub fn backend_time_from_stream_instant(instant: StreamInstant) -> BackendTime {
    BackendTime(instant.as_nanos().min(u64::MAX as u128) as u64)
}

/// Convert one CPAL output callback timestamp plus the embedding engine's
/// contiguous valid-PCM prefix into a backend-neutral playback observation.
///
/// `output_frame_end` is exclusive. Frames in the hardware callback after that
/// boundary are intentionally outside the logical audible timeline (typically
/// because the embedding engine filled the remainder with silence on underrun).
#[inline]
pub fn observation_from_callback(
    info: &OutputCallbackInfo,
    generation: Generation,
    seek_epoch: SeekEpoch,
    output_frame_start: OutputFrame,
    output_frame_end: OutputFrame,
    playing: bool,
) -> PlaybackObservation {
    let timestamp = info.timestamp();
    PlaybackObservation {
        generation,
        seek_epoch,
        callback_time: backend_time_from_stream_instant(timestamp.callback),
        playback_time: backend_time_from_stream_instant(timestamp.playback),
        output_frame_start,
        output_frame_end,
        playing,
    }
}

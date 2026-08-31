#![cfg(feature = "cpal")]

use audio_playback_timeline::{
    cpal_adapter::observation_from_callback, BackendTime, Generation, OutputFrame, SeekEpoch,
};
use cpal::{OutputCallbackInfo, OutputStreamTimestamp, StreamInstant};

#[test]
fn cpal_callback_timestamp_is_preserved_in_backend_neutral_observation() {
    let info = OutputCallbackInfo::new(OutputStreamTimestamp {
        callback: StreamInstant::from_nanos(10_000),
        playback: StreamInstant::from_nanos(25_000),
    });

    let observation = observation_from_callback(
        &info,
        Generation(9),
        SeekEpoch(4),
        OutputFrame(100),
        OutputFrame(124),
        true,
    );

    assert_eq!(observation.generation, Generation(9));
    assert_eq!(observation.seek_epoch, SeekEpoch(4));
    assert_eq!(observation.callback_time, BackendTime(10_000));
    assert_eq!(observation.playback_time, BackendTime(25_000));
    assert_eq!(observation.output_frame_start, OutputFrame(100));
    assert_eq!(observation.output_frame_end, OutputFrame(124));
    assert!(observation.playing);
}

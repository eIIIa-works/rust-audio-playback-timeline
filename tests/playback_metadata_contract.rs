use audio_playback_timeline::{
    BackendTime, ClockGate, Generation, OutputFrame, PlaybackObservation, SeekEpoch,
    SharedAudibleClock, TimedFrame,
};

fn observation(
    generation: Generation,
    seek_epoch: SeekEpoch,
    playback_ns: u64,
    output_frame_start: u64,
    output_frame_end: u64,
) -> PlaybackObservation {
    PlaybackObservation {
        generation,
        seek_epoch,
        callback_time: BackendTime(playback_ns.saturating_sub(100_000)),
        playback_time: BackendTime(playback_ns),
        output_frame_start: OutputFrame(output_frame_start),
        output_frame_end: OutputFrame(output_frame_end),
        playing: true,
    }
}

fn timed<T>(
    generation: Generation,
    seek_epoch: SeekEpoch,
    output_frame_end: u64,
    payload: T,
) -> TimedFrame<T> {
    TimedFrame {
        generation,
        seek_epoch,
        output_frame_end: OutputFrame(output_frame_end),
        payload,
    }
}

#[test]
fn metadata_waits_for_the_dac_even_when_the_next_callback_is_already_scheduled() {
    let generation = Generation(4);
    let seek_epoch = SeekEpoch(2);
    let (mut writer, reader) = SharedAudibleClock::new(48_000, generation, seek_epoch)
        .unwrap()
        .split();
    let mut gate = ClockGate::new();

    gate.push(timed(generation, seek_epoch, 148, "first"))
        .unwrap();
    gate.push(timed(generation, seek_epoch, 196, "second"))
        .unwrap();

    writer
        .observe_callback(observation(generation, seek_epoch, 1_000_000_000, 100, 148))
        .unwrap();
    writer
        .observe_callback(observation(generation, seek_epoch, 1_003_000_000, 148, 196))
        .unwrap();

    let before_current_callback = reader.audible_frame_at(BackendTime(1_002_000_000));
    assert_eq!(before_current_callback, OutputFrame(148));
    assert_eq!(
        gate.take_latest_ready(generation, seek_epoch, before_current_callback)
            .map(|frame| frame.payload),
        Some("first")
    );
    assert_eq!(gate.len(), 1);

    let current_callback_is_audible = reader.audible_frame_at(BackendTime(1_004_000_000));
    assert_eq!(current_callback_is_audible, OutputFrame(196));
    assert_eq!(
        gate.take_latest_ready(generation, seek_epoch, current_callback_is_audible)
            .map(|frame| frame.payload),
        Some("second")
    );
    assert!(gate.is_empty());
}

#[test]
fn underrun_silence_cannot_release_metadata_for_audio_that_was_not_heard() {
    let generation = Generation(8);
    let seek_epoch = SeekEpoch(5);
    let (mut writer, reader) = SharedAudibleClock::new(48_000, generation, seek_epoch)
        .unwrap()
        .split();
    let mut gate = ClockGate::new();

    gate.push(timed(generation, seek_epoch, 224, "valid-prefix"))
        .unwrap();
    gate.push(timed(generation, seek_epoch, 248, "after-underrun"))
        .unwrap();

    // The hardware callback may be larger, but only [200, 224) contained a
    // contiguous prefix of real PCM. The remainder is silence after underrun.
    writer
        .observe_callback(observation(generation, seek_epoch, 1_000_000_000, 200, 224))
        .unwrap();

    let long_after_callback = reader.audible_frame_at(BackendTime(2_000_000_000));
    assert_eq!(long_after_callback, OutputFrame(224));
    assert_eq!(
        gate.take_latest_ready(generation, seek_epoch, long_after_callback)
            .map(|frame| frame.payload),
        Some("valid-prefix")
    );
    assert_eq!(gate.len(), 1);
    assert_eq!(
        gate.take_latest_ready(generation, seek_epoch, long_after_callback),
        None
    );
}

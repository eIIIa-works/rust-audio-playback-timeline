use audio_playback_timeline::{
    AudibleClock, BackendTime, ClockError, Generation, OutputFrame, PlaybackObservation, SeekEpoch,
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

#[test]
fn deterministic_clock_rejects_a_gap_between_accepted_callback_spans() {
    let generation = Generation(2);
    let seek_epoch = SeekEpoch(7);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();

    clock
        .observe_callback(observation(generation, seek_epoch, 1_000_000_000, 100, 148))
        .unwrap();

    let error = clock
        .observe_callback(observation(generation, seek_epoch, 1_003_000_000, 149, 197))
        .unwrap_err();

    assert_eq!(
        error,
        ClockError::DiscontinuousSpan {
            expected_start: OutputFrame(148),
            actual_start: OutputFrame(149),
        }
    );
    assert_eq!(
        clock.audible_frame_at(BackendTime(2_000_000_000)),
        OutputFrame(148)
    );
}

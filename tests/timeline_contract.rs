use audio_playback_timeline::{
    AudibleClock, BackendTime, Generation, ObservationOutcome, OutputFrame, PlaybackObservation,
    SeekEpoch,
};

fn observation(
    generation: Generation,
    seek_epoch: SeekEpoch,
    playback_ns: u64,
    output_frame_start: u64,
    output_frame_end: u64,
    playing: bool,
) -> PlaybackObservation {
    PlaybackObservation {
        generation,
        seek_epoch,
        callback_time: BackendTime(playback_ns.saturating_sub(100_000)),
        playback_time: BackendTime(playback_ns),
        output_frame_start: OutputFrame(output_frame_start),
        output_frame_end: OutputFrame(output_frame_end),
        playing,
    }
}

#[test]
fn future_callback_does_not_make_audible_clock_lead() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            100,
            148,
            true,
        ))
        .unwrap();

    assert_eq!(
        clock.audible_frame_at(BackendTime(999_999_999)),
        OutputFrame(0)
    );
}

#[test]
fn audible_clock_interpolates_inside_valid_pcm_span() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            100,
            148,
            true,
        ))
        .unwrap();

    assert_eq!(
        clock.audible_frame_at(BackendTime(1_000_500_000)),
        OutputFrame(124)
    );
}

#[test]
fn audible_clock_freezes_at_end_of_contiguous_pcm_prefix() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            200,
            224,
            true,
        ))
        .unwrap();

    assert_eq!(
        clock.audible_frame_at(BackendTime(1_002_000_000)),
        OutputFrame(224)
    );
}

#[test]
fn future_current_callback_falls_back_to_previous_span() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            100,
            148,
            true,
        ))
        .unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_003_000_000,
            148,
            196,
            true,
        ))
        .unwrap();

    assert_eq!(
        clock.audible_frame_at(BackendTime(1_002_000_000)),
        OutputFrame(148)
    );
}

#[test]
fn paused_callback_holds_logical_output_frame() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            350,
            350,
            false,
        ))
        .unwrap();

    assert_eq!(
        clock.audible_frame_at(BackendTime(1_005_000_000)),
        OutputFrame(350)
    );
}

#[test]
fn reported_audible_frame_is_monotonic_even_if_backend_time_moves_backwards() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            100,
            148,
            true,
        ))
        .unwrap();

    assert_eq!(
        clock.audible_frame_at(BackendTime(1_000_500_000)),
        OutputFrame(124)
    );
    assert_eq!(
        clock.audible_frame_at(BackendTime(1_000_250_000)),
        OutputFrame(124)
    );
}

#[test]
fn reset_invalidates_old_generation_and_seek_epoch_observations() {
    let generation = Generation(1);
    let seek_epoch = SeekEpoch(1);
    let mut clock = AudibleClock::new(48_000, generation, seek_epoch).unwrap();
    clock
        .observe_callback(observation(
            generation,
            seek_epoch,
            1_000_000_000,
            100,
            148,
            true,
        ))
        .unwrap();
    assert_eq!(
        clock.audible_frame_at(BackendTime(1_001_000_000)),
        OutputFrame(148)
    );

    clock.reset(Generation(2), SeekEpoch(4));
    let outcome = clock
        .observe_callback(observation(
            Generation(1),
            SeekEpoch(1),
            2_000_000_000,
            999,
            1_100,
            true,
        ))
        .unwrap();

    assert_eq!(outcome, ObservationOutcome::IgnoredStale);
    assert_eq!(
        clock.audible_frame_at(BackendTime(2_001_000_000)),
        OutputFrame(0)
    );
}

#[test]
fn invalid_sample_rate_and_reversed_span_are_rejected() {
    assert!(AudibleClock::new(0, Generation(1), SeekEpoch(1)).is_err());

    let mut clock = AudibleClock::new(48_000, Generation(1), SeekEpoch(1)).unwrap();
    assert!(clock
        .observe_callback(observation(
            Generation(1),
            SeekEpoch(1),
            1_000_000_000,
            200,
            199,
            true,
        ))
        .is_err());
}

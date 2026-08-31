use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;

use audio_playback_timeline::{
    BackendTime, Generation, ObservationOutcome, OutputFrame, PlaybackObservation, SeekEpoch,
    SharedAudibleClock,
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
fn shared_clock_keeps_previous_span_for_future_current_callback() {
    let generation = Generation(3);
    let seek_epoch = SeekEpoch(2);
    let (mut writer, reader) = SharedAudibleClock::new(48_000, generation, seek_epoch)
        .unwrap()
        .split();

    writer
        .observe_callback(observation(generation, seek_epoch, 1_000_000_000, 100, 148))
        .unwrap();
    writer
        .observe_callback(observation(generation, seek_epoch, 1_003_000_000, 148, 196))
        .unwrap();

    assert_eq!(
        reader.audible_frame_at(BackendTime(1_002_000_000)),
        OutputFrame(148)
    );
}

#[test]
fn shared_clock_ignores_stale_and_regressive_observations() {
    let generation = Generation(3);
    let seek_epoch = SeekEpoch(2);
    let (mut writer, reader) = SharedAudibleClock::new(48_000, generation, seek_epoch)
        .unwrap()
        .split();

    assert_eq!(
        writer
            .observe_callback(observation(generation, seek_epoch, 1_000_000_000, 100, 148,))
            .unwrap(),
        ObservationOutcome::Accepted
    );
    assert_eq!(
        writer
            .observe_callback(observation(
                Generation(99),
                seek_epoch,
                2_000_000_000,
                148,
                196,
            ))
            .unwrap(),
        ObservationOutcome::IgnoredStale
    );
    assert_eq!(
        writer
            .observe_callback(observation(generation, seek_epoch, 999_000_000, 148, 196,))
            .unwrap(),
        ObservationOutcome::IgnoredRegressiveTimestamp
    );

    assert_eq!(
        reader.audible_frame_at(BackendTime(1_000_750_000)),
        OutputFrame(136)
    );
}

#[test]
fn one_writer_and_cloneable_readers_can_run_concurrently_without_locks() {
    const CALLBACKS: u64 = 20_000;
    let generation = Generation(7);
    let seek_epoch = SeekEpoch(4);
    let (mut writer_handle, reader_handle) = SharedAudibleClock::new(1_000, generation, seek_epoch)
        .unwrap()
        .split();
    let done = Arc::new(AtomicBool::new(false));

    let writer_done = Arc::clone(&done);
    let writer = thread::spawn(move || {
        for frame in 0..CALLBACKS {
            writer_handle
                .observe_callback(observation(
                    generation,
                    seek_epoch,
                    frame.saturating_mul(1_000_000),
                    frame,
                    frame + 1,
                ))
                .unwrap();
        }
        writer_done.store(true, Ordering::Release);
    });

    let reader_clock = reader_handle.clone();
    let reader_done = Arc::clone(&done);
    let reader = thread::spawn(move || {
        let mut last = OutputFrame(0);
        while !reader_done.load(Ordering::Acquire) {
            let current = reader_clock.audible_frame_at(BackendTime(u64::MAX));
            assert!(current >= last, "shared audible frame moved backwards");
            last = current;
            std::hint::spin_loop();
        }
        last
    });

    writer.join().unwrap();
    let last_seen = reader.join().unwrap();
    let final_frame = reader_handle.audible_frame_at(BackendTime(u64::MAX));

    assert!(final_frame >= last_seen);
    assert_eq!(final_frame, OutputFrame(CALLBACKS));
}

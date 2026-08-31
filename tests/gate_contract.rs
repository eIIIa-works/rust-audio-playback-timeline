use audio_playback_timeline::{ClockGate, Generation, OutputFrame, SeekEpoch, TimedFrame};

fn frame(
    generation: Generation,
    seek_epoch: SeekEpoch,
    output_frame_end: u64,
    payload: u8,
) -> TimedFrame<u8> {
    TimedFrame {
        generation,
        seek_epoch,
        output_frame_end: OutputFrame(output_frame_end),
        payload,
    }
}

#[test]
fn future_frame_is_retained_until_audible_clock_reaches_it() {
    let generation = Generation(7);
    let seek_epoch = SeekEpoch(3);
    let mut gate = ClockGate::new();
    gate.push(frame(generation, seek_epoch, 512, 42)).unwrap();

    assert_eq!(
        gate.take_latest_ready(generation, seek_epoch, OutputFrame(511)),
        None
    );
    assert_eq!(gate.len(), 1);
    assert_eq!(
        gate.take_latest_ready(generation, seek_epoch, OutputFrame(512))
            .map(|f| f.payload),
        Some(42)
    );
    assert!(gate.is_empty());
}

#[test]
fn stale_generation_and_seek_epoch_are_discarded() {
    let mut gate = ClockGate::new();
    gate.push(frame(Generation(1), SeekEpoch(1), 100, 1))
        .unwrap();
    gate.push(frame(Generation(2), SeekEpoch(1), 110, 2))
        .unwrap();
    gate.push(frame(Generation(2), SeekEpoch(2), 120, 3))
        .unwrap();

    assert_eq!(
        gate.take_latest_ready(Generation(2), SeekEpoch(2), OutputFrame(119)),
        None
    );
    assert_eq!(gate.len(), 1);
    assert_eq!(
        gate.take_latest_ready(Generation(2), SeekEpoch(2), OutputFrame(120))
            .map(|f| f.payload),
        Some(3)
    );
}

#[test]
fn clock_jump_returns_only_newest_ready_frame() {
    let generation = Generation(5);
    let seek_epoch = SeekEpoch(9);
    let mut gate = ClockGate::new();
    for (end, payload) in [(100, 1), (200, 2), (300, 3), (400, 4)] {
        gate.push(frame(generation, seek_epoch, end, payload))
            .unwrap();
    }

    let latest = gate
        .take_latest_ready(generation, seek_epoch, OutputFrame(350))
        .unwrap();
    assert_eq!(latest.payload, 3);
    assert_eq!(gate.len(), 1);
}

#[test]
fn out_of_order_frames_within_one_timeline_are_rejected() {
    let generation = Generation(5);
    let seek_epoch = SeekEpoch(9);
    let mut gate = ClockGate::new();
    gate.push(frame(generation, seek_epoch, 200, 1)).unwrap();

    assert!(gate.push(frame(generation, seek_epoch, 199, 2)).is_err());
    assert_eq!(gate.len(), 1);
}

#[test]
fn clear_discards_all_pending_frames() {
    let mut gate = ClockGate::new();
    gate.push(frame(Generation(1), SeekEpoch(1), 100, 1))
        .unwrap();
    gate.clear();
    assert!(gate.is_empty());
}

# rust-audio-playback-timeline

Low-level Rust primitives for mapping backend output timing to the audio frame that is actually reaching the DAC, then releasing timestamped metadata against that same audible timeline.

The crate is deliberately small. It is **not** a decoder, media player, resampler, time-stretch engine, audio graph, FFT library, device manager, or UI framework.

## Why this exists

Audio engines usually know several different positions at once:

- how far decoding has progressed;
- how much DSP has produced;
- how many frames have been copied into an output ring;
- how many frames the audio callback has dequeued;
- which frame is actually audible at the device now.

Those positions are not interchangeable. Buffering, callback scheduling, device latency and underruns can make a UI driven from produced/dequeued frames visibly lead the sound. Analysis can also be computed ahead of playback and arrive too early.

`audio-playback-timeline` models the last item explicitly: the logical output frame audible according to the backend playback clock.

## Core invariants

- Audible position is monotonic within one generation/seek epoch.
- A callback whose scheduled playback instant is still in the future cannot move the timeline forward.
- The previous callback span is retained so queries can resolve correctly while the newest callback is still scheduled ahead of `now`.
- Only the contiguous valid PCM prefix of a hardware callback advances media time. Silence after an underrun does not.
- Stale generation/seek observations are ignored.
- Regressive backend playback timestamps are ignored conservatively rather than allowing the logical timeline to lead.
- Metadata is released only when its output-frame boundary becomes audible.
- If several metadata frames become ready together, consumers receive only the newest ready frame instead of a burst of stale visual state.

## Components

### `AudibleClock`

Deterministic single-threaded clock core. Useful for tests, offline simulation and engines that already provide their own synchronization around timing observations.

### `SharedAudibleClock`

Realtime-oriented single-writer/multi-reader clock. `split()` produces one non-cloneable callback writer and a cloneable reader:

```rust
use audio_playback_timeline::{Generation, SeekEpoch, SharedAudibleClock};

let (mut callback_clock, control_clock) =
    SharedAudibleClock::new(48_000, Generation(1), SeekEpoch(1))?.split();
```

The callback writer uses bounded atomic operations only. It does not allocate or take a mutex. Readers obtain coherent current/previous callback snapshots through a bounded seqlock read and fall back to the last published audible frame if they repeatedly race the writer.

A `SharedAudibleClock` instance represents one generation/seek identity. Create a new instance when the embedding engine establishes a new playback timeline.

### `ClockGate<T>`

Control-side queue for metadata computed ahead of playback: spectrum frames, waveform state, meters, markers, captions, automation previews, or any other payload stamped with an output-frame boundary.

```rust
use audio_playback_timeline::{
    ClockGate, Generation, OutputFrame, SeekEpoch, TimedFrame,
};

let generation = Generation(1);
let seek_epoch = SeekEpoch(1);
let mut gate = ClockGate::new();

gate.push(TimedFrame {
    generation,
    seek_epoch,
    output_frame_end: OutputFrame(24_000),
    payload: vec![1_u8, 2, 3],
})?;

let ready = gate.take_latest_ready(
    generation,
    seek_epoch,
    OutputFrame(24_000),
);
```

`ClockGate<T>` is intentionally **not** a realtime callback data structure. Feed it from a control/analysis thread after crossing your own bounded SPSC or equivalent handoff.

## CPAL adapter

CPAL support is optional:

```toml
[dependencies]
audio-playback-timeline = {
    git = "https://github.com/eIIIa-works/rust-audio-playback-timeline",
    features = ["cpal"]
}
```

The adapter only translates CPAL's stream-local timestamps into backend-neutral `PlaybackObservation` values. It does not own the device or stream.

A typical output callback integration looks like this conceptually:

```rust
use audio_playback_timeline::{Generation, OutputFrame, SeekEpoch, SharedAudibleClock};
use audio_playback_timeline::cpal_adapter::observation_from_callback;

let generation = Generation(1);
let seek_epoch = SeekEpoch(1);
let (mut clock_writer, clock_reader) =
    SharedAudibleClock::new(sample_rate, generation, seek_epoch)?.split();

// Inside the CPAL output callback, after filling `data`:
let output_frame_start = OutputFrame(first_frame_in_this_callback);
let output_frame_end = OutputFrame(first_frame_after_the_contiguous_valid_pcm_prefix);

let observation = observation_from_callback(
    info,
    generation,
    seek_epoch,
    output_frame_start,
    output_frame_end,
    playing,
);

// With a valid engine-produced span this is a bounded, non-blocking operation.
let _ = clock_writer.observe_callback(observation);
```

On the control side, query the same CPAL stream-local time base used by the callback timestamp and map it to an audible frame:

```rust
use audio_playback_timeline::cpal_adapter::backend_time_from_stream_instant;
use cpal::traits::StreamTrait;

let now = backend_time_from_stream_instant(stream.now());
let audible_output_frame = clock_reader.audible_frame_at(now);
```

The embedding engine remains responsible for determining the exact valid PCM prefix. A useful rule is: after the first underrun inside a hardware callback, keep the remainder of that callback silent even if a producer refills concurrently. That makes the timeline mapping unambiguous.

## Realtime boundary

The crate does not make arbitrary application code realtime-safe. The intended boundary is:

```text
decoder / DSP / analysis workers
        |
        | bounded handoff
        v
output callback ----> SharedAudibleClockWriter
                           |
                           | atomics / coherent snapshot
                           v
                    SharedAudibleClockReader
                           |
                           +----> transport position
                           +----> ClockGate<T>
                           +----> UI / IPC publisher
```

Do not decode, allocate, perform FFT, log, access files, send IPC, or otherwise do unbounded work in the audio callback merely because the clock write itself is realtime-safe.

## Backend neutrality

The core has no mandatory backend dependency. CPAL is behind the optional `cpal` feature. Other backends can construct `PlaybackObservation` directly as long as callback/playback timestamps and control-side `now` use the same monotonic time base.

This is especially important for adapters to higher-level engines: a processed-sample counter is not automatically equivalent to a device playback clock.

## Status

The API is still pre-1.0 and may change. The current implementation is tested on Linux, macOS and Windows. CI verifies the backend-neutral core, the lock-free shared clock contract and the optional CPAL adapter on all three platforms.

The crate is not published to crates.io yet (`publish = false`).

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.

# rust-audio-playback-timeline

A low-level Rust library for precise audible playback timelines and clock-gated metadata.

This repository is intentionally not a general-purpose audio engine or media-player framework. Its core responsibility is to map backend output timing observations to a monotonic audible-frame timeline, including underrun-aware semantics, generation/seek invalidation, and synchronization of timestamped analysis or other metadata with the frames that are actually audible.

The project is currently under active extraction from a production Tauri/Rust audio path. The public API is not stable yet.

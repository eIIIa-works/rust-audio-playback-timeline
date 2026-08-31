use core::fmt;

/// Logical identity of one loaded playback source/session.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

/// Logical identity of the current seek position within a generation.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeekEpoch(pub u64);

/// Frame index in the output-device sample-rate domain.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputFrame(pub u64);

/// Timestamp in a backend-provided stream-local time base, expressed in nanoseconds.
///
/// The timeline never compares this value with wall-clock time. An adapter must
/// ensure that callback, playback, and query timestamps use the same time base.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendTime(pub u64);

impl fmt::Display for OutputFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

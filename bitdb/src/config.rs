#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum CorruptionPolicy {
    #[default]
    Fail,
    SkipCorruptedTail,
}

/// Controls how many threads may be used for parallel operations such as
/// startup rebuild and merge.  `Auto` lets rayon choose based on the number
/// of logical CPUs.  `Fixed(n)` caps the pool at exactly `n` threads.
/// `Serial` forces the single-threaded code path regardless of CPU count.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum Parallelism {
    /// Use all available logical CPUs (rayon global pool default).
    #[default]
    Auto,
    /// Use exactly `n` worker threads.
    Fixed(usize),
    /// Disable all parallelism; execute every operation serially.
    Serial,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub create_if_missing: bool,
    pub max_data_file_size_bytes: u64,
    pub corruption_policy: CorruptionPolicy,
    /// Parallelism strategy used during startup rebuild and merge.
    pub parallelism: Parallelism,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            max_data_file_size_bytes: 1024 * 1024,
            corruption_policy: CorruptionPolicy::Fail,
            parallelism: Parallelism::default(),
        }
    }
}

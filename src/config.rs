#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum CorruptionPolicy {
    #[default]
    Fail,
    SkipCorruptedTail,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub create_if_missing: bool,
    pub max_data_file_size_bytes: u64,
    pub corruption_policy: CorruptionPolicy,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            max_data_file_size_bytes: 1024 * 1024,
            corruption_policy: CorruptionPolicy::Fail,
        }
    }
}

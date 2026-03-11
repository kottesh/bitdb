use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct KeyDirEntry {
    pub file_id: u32,
    pub offset: u64,
    pub size_bytes: u32,
    pub timestamp: u64,
    pub is_tombstone: bool,
}

#[derive(Clone, Debug, Default)]
pub struct KeyDir {
    entries: HashMap<Vec<u8>, KeyDirEntry>,
}

impl KeyDir {
    pub fn insert(&mut self, key: Vec<u8>, entry: KeyDirEntry) {
        self.entries.insert(key, entry);
    }

    pub fn get(&self, key: &[u8]) -> Option<&KeyDirEntry> {
        self.entries.get(key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &KeyDirEntry> {
        self.entries.values()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &KeyDirEntry)> {
        self.entries.iter().map(|(k, v)| (k.as_slice(), v))
    }
}

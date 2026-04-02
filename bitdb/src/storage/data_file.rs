use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::record::{self, DecodeResult, Record};

#[derive(Debug)]
pub struct DataFile {
    id: u32,
    path: PathBuf,
    writer: File,
    len: u64,
}

impl DataFile {
    pub fn open_append(dir: &Path, id: u32) -> Result<Self> {
        let path = data_file_path(dir, id);
        let writer = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;
        let len = writer.metadata()?.len();

        Ok(Self {
            id,
            path,
            writer,
            len,
        })
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&mut self, record: &Record) -> Result<(u64, usize)> {
        let encoded = record::encode(record);
        let offset = self.len;
        self.writer.write_all(&encoded)?;
        self.len += encoded.len() as u64;
        Ok((offset, encoded.len()))
    }

    pub fn sync(&self) -> Result<()> {
        self.writer.sync_data()?;
        Ok(())
    }

    pub fn read_at(path: &Path, offset: u64) -> Result<DecodeResult> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        record::decode_one(&buf)
    }
}

pub fn data_file_path(dir: &Path, id: u32) -> PathBuf {
    dir.join(format!("{id:08}.data"))
}

pub fn parse_data_file_id(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".data")?;
    stem.parse::<u32>().ok()
}

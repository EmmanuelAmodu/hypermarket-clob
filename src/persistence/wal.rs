use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::models::EventEnvelope;

#[derive(Debug)]
pub struct Wal {
    file: File,
}

impl Wal {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).read(true).open(path)?;
        Ok(Self { file })
    }

    pub fn append(&mut self, event: &EventEnvelope) -> anyhow::Result<()> {
        let bytes = bincode::serialize(event)?;
        let len = bytes.len() as u32;
        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(&bytes)?;
        self.file.flush()?;
        Ok(())
    }

    pub fn load(path: &Path) -> anyhow::Result<Vec<EventEnvelope>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut file = File::open(path)?;
        let mut events = Vec::new();
        loop {
            let mut len_bytes = [0u8; 4];
            if file.read_exact(&mut len_bytes).is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_bytes) as usize;
            let mut buf = vec![0u8; len];
            file.read_exact(&mut buf)?;
            let event: EventEnvelope = bincode::deserialize(&buf)?;
            events.push(event);
        }
        Ok(events)
    }

    pub fn truncate(&mut self) -> anyhow::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(())
    }
}

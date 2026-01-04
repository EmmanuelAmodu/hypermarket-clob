use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::EngineState;

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub version: u32,
    pub shard_id: usize,
    pub last_seq: u64,
    pub checksum: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub state: EngineState,
}

pub struct SnapshotStore;

impl SnapshotStore {
    pub fn save(path: &Path, snapshot: &Snapshot) -> anyhow::Result<()> {
        let bytes = bincode::serialize(snapshot)?;
        let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
        file.write_all(&bytes)?;
        Ok(())
    }

    pub fn load(path: &Path) -> anyhow::Result<Option<Snapshot>> {
        if !path.exists() {
            return Ok(None);
        }
        let mut file = File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let snapshot: Snapshot = bincode::deserialize(&buf)?;
        Ok(Some(snapshot))
    }

    pub fn build(shard_id: usize, last_seq: u64, state: EngineState) -> Snapshot {
        let checksum = blake3::hash(&bincode::serialize(&state).unwrap_or_default()).to_hex().to_string();
        Snapshot {
            meta: SnapshotMeta {
                version: 1,
                shard_id,
                last_seq,
                checksum,
            },
            state,
        }
    }
}

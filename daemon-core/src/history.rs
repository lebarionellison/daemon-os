use crate::intelligence::IntelligenceResult;
use crate::telemetry::SystemSnapshot;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub timestamp: u64,
    pub telemetry: SystemSnapshot,
    pub intelligence: IntelligenceResult,
}

pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn append(
        &self,
        telemetry: SystemSnapshot,
        intelligence: IntelligenceResult,
    ) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent)?;
            }
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let record = HistoryRecord {
            timestamp,
            telemetry,
            intelligence,
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let mut writer = BufWriter::new(file);

        serde_json::to_writer(&mut writer, &record).map_err(std::io::Error::other)?;

        writer.write_all(b"\n")?;
        writer.flush()
    }

    pub fn read_recent(&self, limit: usize) -> std::io::Result<Vec<HistoryRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);

        let mut records = Vec::new();

        for line in reader.lines() {
            let line = line?;

            if line.trim().is_empty() {
                continue;
            }

            let record: HistoryRecord =
                serde_json::from_str(&line).map_err(std::io::Error::other)?;

            records.push(record);
        }

        records.reverse();
        records.truncate(limit);

        Ok(records)
    }
}

use crate::events::DaemonEvent;
use serde_json;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct EventStore {
    path: PathBuf,
}

impl EventStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn append(&self, event: &DaemonEvent) -> std::io::Result<()> {
        let mut events = self.read_all()?;

        if let Some(existing) = events.iter_mut().find(|item| item.id == event.id) {
            *existing = event.clone();
        } else {
            events.push(event.clone());
        }

        self.write_all(&events)
    }

    pub fn upsert(&self, event: &DaemonEvent) -> std::io::Result<()> {
        self.append(event)
    }

    pub fn get(&self, id: &str) -> std::io::Result<Option<DaemonEvent>> {
        let events = self.read_all()?;

        Ok(events.into_iter().find(|event| event.id == id))
    }

    pub fn read_all(&self) -> std::io::Result<Vec<DaemonEvent>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();

        for line in reader.lines() {
            let line = line?;

            if line.trim().is_empty() {
                continue;
            }

            let event: DaemonEvent = serde_json::from_str(&line).map_err(std::io::Error::other)?;

            events.push(event);
        }

        Ok(events)
    }

    pub fn read_recent(&self, limit: usize) -> std::io::Result<Vec<DaemonEvent>> {
        let mut events = self.read_all()?;

        events.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        events.truncate(limit);

        Ok(events)
    }

    pub fn delete(&self, id: &str) -> std::io::Result<bool> {
        let mut events = self.read_all()?;
        let original_len = events.len();

        events.retain(|event| event.id != id);

        if events.len() == original_len {
            return Ok(false);
        }

        self.write_all(&events)?;

        Ok(true)
    }

    fn write_all(&self, events: &[DaemonEvent]) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent)?;
            }
        }

        let temp_path = self.path.with_extension("jsonl.tmp");

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)?;

        let mut writer = BufWriter::new(file);

        for event in events {
            serde_json::to_writer(&mut writer, event).map_err(std::io::Error::other)?;

            writer.write_all(b"\n")?;
        }

        writer.flush()?;

        std::fs::rename(&temp_path, &self.path)?;

        Ok(())
    }
}

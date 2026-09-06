use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub monitoring_enabled: bool,
    pub intelligence_enabled: bool,
    pub history_enabled: bool,
    pub history_interval_seconds: u64,
    pub cpu_warning_percent: f32,
    pub cpu_critical_percent: f32,
    pub memory_warning_percent: f32,
    pub memory_critical_percent: f32,
    pub disk_warning_percent: f32,
    pub disk_critical_percent: f32,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            monitoring_enabled: true,
            intelligence_enabled: true,
            history_enabled: true,
            history_interval_seconds: 60,
            cpu_warning_percent: 75.0,
            cpu_critical_percent: 90.0,
            memory_warning_percent: 80.0,
            memory_critical_percent: 90.0,
            disk_warning_percent: 85.0,
            disk_critical_percent: 95.0,
        }
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self) -> std::io::Result<DaemonConfig> {
        if !self.path.exists() {
            let config = DaemonConfig::default();
            self.save(&config)?;
            return Ok(config);
        }

        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        serde_json::from_str(&contents).map_err(std::io::Error::other)
    }

    pub fn save(&self, config: &DaemonConfig) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent)?;
            }
        }

        let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;

        let mut file = File::create(&self.path)?;
        file.write_all(json.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()
    }
}

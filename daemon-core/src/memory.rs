use crate::history::HistoryRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvent {
    pub timestamp: u64,
    pub event_type: String,
    pub severity: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    pub observations: usize,
    pub events: Vec<MemoryEvent>,
    pub current_state: String,
    pub dominant_signal: String,
    pub recovery_detected: bool,
    pub recurring_issue: bool,
}

pub struct MemoryEngine;

impl MemoryEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, records: &[HistoryRecord]) -> MemorySummary {
        if records.is_empty() {
            return MemorySummary {
                observations: 0,
                events: Vec::new(),
                current_state: "NO_HISTORY".to_string(),
                dominant_signal: "NONE".to_string(),
                recovery_detected: false,
                recurring_issue: false,
            };
        }

        let mut events = Vec::new();

        let current = &records[0];

        let current_state = current.intelligence.status.clone();

        let dominant_signal = if current.intelligence.disk_status == "CRITICAL" {
            "DISK"
        } else if current.intelligence.memory_status == "CRITICAL" {
            "MEMORY"
        } else if current.intelligence.cpu_status == "CRITICAL" {
            "CPU"
        } else if current.intelligence.disk_status == "WARNING" {
            "DISK"
        } else if current.intelligence.memory_status == "WARNING" {
            "MEMORY"
        } else if current.intelligence.cpu_status == "WARNING" {
            "CPU"
        } else {
            "NONE"
        }
        .to_string();

        for record in records.iter().take(20) {
            if record.intelligence.anomaly_detected {
                events.push(MemoryEvent {
                    timestamp: record.timestamp,
                    event_type: "ANOMALY".to_string(),
                    severity: record.intelligence.status.clone(),
                    summary: "Daemon detected a significant change in system behavior.".to_string(),
                });
            }

            if record.intelligence.trend == "WORSENING" {
                events.push(MemoryEvent {
                    timestamp: record.timestamp,
                    event_type: "TREND_WORSENING".to_string(),
                    severity: "WARNING".to_string(),
                    summary: "System conditions are moving in a worsening direction.".to_string(),
                });
            }

            if record.intelligence.trend == "IMPROVING" {
                events.push(MemoryEvent {
                    timestamp: record.timestamp,
                    event_type: "TREND_IMPROVING".to_string(),
                    severity: "INFO".to_string(),
                    summary: "System conditions are improving.".to_string(),
                });
            }

            if record.intelligence.disk_status == "CRITICAL" {
                events.push(MemoryEvent {
                    timestamp: record.timestamp,
                    event_type: "DISK_PRESSURE".to_string(),
                    severity: "CRITICAL".to_string(),
                    summary: "Disk capacity reached a critical level.".to_string(),
                });
            }

            if record.intelligence.memory_status == "CRITICAL" {
                events.push(MemoryEvent {
                    timestamp: record.timestamp,
                    event_type: "MEMORY_PRESSURE".to_string(),
                    severity: "CRITICAL".to_string(),
                    summary: "Memory utilization reached a critical level.".to_string(),
                });
            }

            if record.intelligence.cpu_status == "CRITICAL" {
                events.push(MemoryEvent {
                    timestamp: record.timestamp,
                    event_type: "CPU_PRESSURE".to_string(),
                    severity: "CRITICAL".to_string(),
                    summary: "CPU utilization reached a critical level.".to_string(),
                });
            }
        }

        let recovery_detected = if records.len() >= 2 {
            let previous = &records[1];

            let previous_bad = previous.intelligence.status == "CRITICAL"
                || previous.intelligence.status == "AT_RISK"
                || previous.intelligence.anomaly_detected;

            let current_better = current.intelligence.health_score
                > previous.intelligence.health_score
                || current.intelligence.trend == "IMPROVING";

            previous_bad && current_better
        } else {
            false
        };

        if recovery_detected {
            events.push(MemoryEvent {
                timestamp: current.timestamp,
                event_type: "RECOVERY".to_string(),
                severity: "INFO".to_string(),
                summary: "Daemon detected recovery toward a healthier system state.".to_string(),
            });
        }

        let recurring_issue = self.detect_recurring_issue(records);

        if recurring_issue {
            events.push(MemoryEvent {
                timestamp: current.timestamp,
                event_type: "RECURRING_ISSUE".to_string(),
                severity: "WARNING".to_string(),
                summary: "Daemon detected a recurring resource pressure pattern in system history."
                    .to_string(),
            });
        }

        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(50);

        MemorySummary {
            observations: records.len(),
            events,
            current_state,
            dominant_signal,
            recovery_detected,
            recurring_issue,
        }
    }

    fn detect_recurring_issue(&self, records: &[HistoryRecord]) -> bool {
        if records.len() < 4 {
            return false;
        }

        let mut disk_events = 0;
        let mut memory_events = 0;
        let mut cpu_events = 0;

        for record in records.iter().take(20) {
            if record.intelligence.disk_status != "HEALTHY" {
                disk_events += 1;
            }

            if record.intelligence.memory_status != "HEALTHY" {
                memory_events += 1;
            }

            if record.intelligence.cpu_status != "HEALTHY" {
                cpu_events += 1;
            }
        }

        disk_events >= 3 || memory_events >= 3 || cpu_events >= 3
    }
}

use crate::events::{DaemonEvent, EventSource, EventStatus};
use crate::telemetry::{SystemSnapshot, Telemetry};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DaemonAgent {
    telemetry: Telemetry,
    previous: Option<SystemSnapshot>,
}

impl DaemonAgent {
    pub fn new() -> Self {
        Self {
            telemetry: Telemetry::new(),
            previous: None,
        }
    }

    pub fn observe(&mut self) -> (SystemSnapshot, Vec<DaemonEvent>) {
        let snapshot = self.telemetry.snapshot();
        let now = unix_timestamp();
        let mut events = Vec::new();

        if self.previous.is_none() {
            if snapshot.disk_used >= snapshot.disk_total.saturating_mul(90) / 100 {
                events.push(self.event(
                    "Disk capacity warning",
                    format!(
                        "Disk utilization is already above 90% at {:.1}%.",
                        (snapshot.disk_used as f64 / snapshot.disk_total.max(1) as f64) * 100.0
                    ),
                    now,
                    0.96,
                ));
            }

            if snapshot.cpu_usage >= 80.0 {
                events.push(self.event(
                    "CPU utilization warning",
                    format!(
                        "CPU utilization is already elevated at {:.1}%.",
                        snapshot.cpu_usage
                    ),
                    now,
                    0.92,
                ));
            }

            if snapshot.memory_used >= snapshot.memory_total.saturating_mul(80) / 100 {
                events.push(self.event(
                    "Memory utilization warning",
                    format!(
                        "Memory utilization is already above 80% at {:.1}%.",
                        (snapshot.memory_used as f64 / snapshot.memory_total.max(1) as f64) * 100.0
                    ),
                    now,
                    0.90,
                ));
            }
        }

        if let Some(previous) = &self.previous {
            if snapshot.cpu_usage >= 80.0 && previous.cpu_usage < 80.0 {
                events.push(self.event(
                    "CPU utilization spike",
                    format!(
                        "CPU crossed 80%: {:.1}% -> {:.1}%",
                        previous.cpu_usage, snapshot.cpu_usage
                    ),
                    now,
                    0.92,
                ));
            }

            if snapshot.memory_used >= snapshot.memory_total.saturating_mul(80) / 100
                && previous.memory_used < previous.memory_total.saturating_mul(80) / 100
            {
                events.push(self.event(
                    "Memory utilization spike",
                    "Memory crossed the 80% utilization threshold.".to_string(),
                    now,
                    0.90,
                ));
            }

            if snapshot.disk_used >= snapshot.disk_total.saturating_mul(90) / 100
                && previous.disk_used < previous.disk_total.saturating_mul(90) / 100
            {
                events.push(self.event(
                    "Disk capacity warning",
                    "Disk utilization crossed the 90% threshold.".to_string(),
                    now,
                    0.96,
                ));
            }

            if snapshot.cpu_usage > previous.cpu_usage + 15.0 {
                events.push(self.event(
                    "Significant CPU change",
                    format!(
                        "CPU increased by {:.1} percentage points.",
                        snapshot.cpu_usage - previous.cpu_usage
                    ),
                    now,
                    0.86,
                ));
            }
        }

        for event in &mut events {
            let condition = if event.name.to_lowercase().contains("cpu") {
                "cpu"
            } else if event.name.to_lowercase().contains("memory") {
                "memory"
            } else if event.name.to_lowercase().contains("disk") {
                "disk"
            } else {
                "other"
            };

            if let Some(summary) = self.investigation_summary(condition, &snapshot, &event.evidence) {
                event.evidence.push(summary.clone());
                let explanation = summary.strip_prefix("cause=").unwrap_or(&summary);
                event.description = format!("{} Investigation: {}", event.description, explanation); let recommendation = match condition { "cpu" => "recommendation=Review the identified high-CPU process and determine whether it should be closed, restarted, or throttled", "memory" => "recommendation=Review the highest-memory processes and reclaim unnecessary memory pressure", "disk" => "recommendation=Free disk capacity by removing unnecessary files or expanding available storage", _ => "recommendation=Continue monitoring the detected system condition" }; event.evidence.push(recommendation.to_string()); event.description = format!("{} Recommendation: {}", event.description, recommendation.strip_prefix("recommendation=").unwrap_or(recommendation));
            }
        }
        self.previous = Some(snapshot.clone());

        (snapshot, events)
    }

    fn condition_evidence(&self, condition: &str) -> Vec<String> {
        match condition {
            "cpu" => self.top_process_evidence(),
            "memory" => self.top_process_evidence(),
            "disk" => vec![
                "signal=disk-capacity".to_string(),
                "source=live-telemetry".to_string(),
            ],
            _ => Vec::new(),
        }
    }
    fn top_process_evidence(&self) -> Vec<String> {
        let mut evidence = Vec::new();

        #[cfg(target_os = "windows")]
        {
            if let Ok(output) = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Get-Process | Sort-Object CPU -Descending | Select-Object -First 5 ProcessName,Id,CPU | ForEach-Object { \"process=$($_.ProcessName) pid=$($_.Id) cpu_seconds=$([math]::Round($_.CPU,2))\" }",
                ])
                .output()
            {
                if output.status.success() {
                    if let Ok(text) = String::from_utf8(output.stdout) {
                        for line in text.lines().filter(|line| !line.trim().is_empty()) {
                            evidence.push(line.trim().to_string());
                        }
                    }
                }
            }
        }

        evidence
    }
    fn investigation_summary(&self, condition: &str, snapshot: &SystemSnapshot, evidence: &[String]) -> Option<String> {
        match condition {
            "cpu" => {
                let process = evidence.iter()
                    .find(|item| item.starts_with("process="))
                    .map(|item| item.trim_start_matches("process="))
                    .unwrap_or("unknown process");

                if let Some(previous) = &self.previous {
                    let delta = snapshot.cpu_usage - previous.cpu_usage;
                    Some(format!(
                        "cause=CPU pressure is primarily associated with {process}; CPU changed by {delta:.1} percentage points"
                    ))
                } else {
                    Some(format!(
                        "cause=CPU pressure is primarily associated with {process}; no previous CPU baseline available"
                    ))
                }
            }
            "memory" => {
                Some("cause=memory pressure detected from live system telemetry".to_string())
            }
            "disk" => {
                Some("cause=disk capacity is the dominant system constraint".to_string())
            }
            _ => None,
        }
    }
    fn investigate_evidence(&self, condition: &str, evidence: &[String]) -> Vec<String> {
        let mut findings = Vec::new();

        if matches!(condition, "cpu" | "memory") {
            if let Some(process) = evidence.iter().find(|item| item.starts_with("process=")) {
                findings.push(format!("investigation={process}"));
            }
        }

        if condition == "disk" {
            findings.push("investigation=disk-capacity-is-the-dominant-signal".to_string());
        }

        findings
    }
    fn event(
        &self,
        name: &str,
        description: String,
        now: u64,
        confidence: f32,
    ) -> DaemonEvent {
        DaemonEvent {
            id: format!("agent-{}-{}", now, name.replace(' ', "-").to_lowercase()),
            name: name.to_string(),
            description,
            source: EventSource::System,
            status: EventStatus::Monitoring,
            scheduled_at: None,
            started_at: Some(now),
            expected_completion_at: None,
            completed_at: None,
            updated_at: now,
            expected_duration_seconds: None,
            monitoring_enabled: true,
            signals_monitored: 1,
            abnormal_signals: 1,
            outcome: None,
            evidence: { let condition = if name.to_lowercase().contains("cpu") { "cpu" } else if name.to_lowercase().contains("memory") { "memory" } else if name.to_lowercase().contains("disk") { "disk" } else { "other" }; let mut e = vec!["daemon-agent".to_string(), "live-telemetry".to_string()]; e.extend(self.condition_evidence(condition)); let findings = self.investigate_evidence(condition, &e); e.extend(findings); e },
            confidence,
        }
    }
}

impl Default for DaemonAgent {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
















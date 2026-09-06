pub mod bloom;

use crate::config::DaemonConfig;
use crate::telemetry::SystemSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ThreatResult {
    pub detected: bool,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceResult {
    pub health_score: u8,
    pub status: String,
    pub cpu_status: String,
    pub memory_status: String,
    pub disk_status: String,
    pub recommendations: Vec<String>,
    #[serde(default = "default_trend")]
    pub trend: String,
    #[serde(default)]
    pub anomaly_detected: bool,
    #[serde(default)]
    pub anomaly_signals: Vec<String>,
    #[serde(default)]
    pub cpu_change_percent: f32,
    #[serde(default)]
    pub memory_change_percent: f32,
    #[serde(default)]
    pub disk_change_percent: f32,
}

fn default_trend() -> String {
    "STABLE".to_string()
}

pub struct IntelligenceEngine;

impl IntelligenceEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(
        &self,
        snapshot: &SystemSnapshot,
        previous: Option<&SystemSnapshot>,
        config: &DaemonConfig,
    ) -> IntelligenceResult {
        let memory_percent = percent(snapshot.memory_used, snapshot.memory_total);
        let disk_percent = percent(snapshot.disk_used, snapshot.disk_total);

        let cpu_status = threshold_status(
            snapshot.cpu_usage,
            config.cpu_warning_percent,
            config.cpu_critical_percent,
        );

        let memory_status = threshold_status(
            memory_percent,
            config.memory_warning_percent as f32,
            config.memory_critical_percent as f32,
        );

        let disk_status = threshold_status(
            disk_percent,
            config.disk_warning_percent as f32,
            config.disk_critical_percent as f32,
        );

        let mut health_score: i32 = 100;

        for status in [cpu_status, memory_status, disk_status] {
            match status {
                "CRITICAL" => health_score -= 30,
                "WARNING" => health_score -= 15,
                _ => {}
            }
        }

        let health_score = health_score.clamp(0, 100) as u8;

        let status = if health_score >= 90 {
            "HEALTHY"
        } else if health_score >= 70 {
            "DEGRADED"
        } else if health_score >= 40 {
            "AT_RISK"
        } else {
            "CRITICAL"
        };

        let mut recommendations = Vec::new();

        add_resource_recommendation(
            &mut recommendations,
            "CPU",
            cpu_status,
            "Investigate the processes consuming the most CPU.",
            "Monitor active workloads for sustained CPU pressure.",
        );

        add_resource_recommendation(
            &mut recommendations,
            "Memory",
            memory_status,
            "Investigate memory-heavy processes and reclaim available memory.",
            "Monitor workloads for increasing memory pressure.",
        );

        add_resource_recommendation(
            &mut recommendations,
            "Disk",
            disk_status,
            "Free space or expand storage immediately.",
            "Plan cleanup or additional storage.",
        );

        let mut cpu_change_percent = 0.0;
        let mut memory_change_percent = 0.0;
        let mut disk_change_percent = 0.0;

        let mut anomaly_signals = Vec::new();
        let mut trend = "STABLE".to_string();

        if let Some(previous) = previous {
            cpu_change_percent = snapshot.cpu_usage - previous.cpu_usage;

            memory_change_percent = percentage_point_change(
                snapshot.memory_used,
                snapshot.memory_total,
                previous.memory_used,
                previous.memory_total,
            );

            disk_change_percent = percentage_point_change(
                snapshot.disk_used,
                snapshot.disk_total,
                previous.disk_used,
                previous.disk_total,
            );

            let worsening_signals = (cpu_change_percent >= 10.0) as u8
                + (memory_change_percent >= 5.0) as u8
                + (disk_change_percent >= 2.0) as u8;

            let improving_signals = (cpu_change_percent <= -10.0) as u8
                + (memory_change_percent <= -5.0) as u8
                + (disk_change_percent <= -2.0) as u8;

            trend = if worsening_signals >= 2 {
                "WORSENING".to_string()
            } else if improving_signals >= 2 {
                "IMPROVING".to_string()
            } else {
                "STABLE".to_string()
            };

            if cpu_change_percent.abs() >= 20.0 {
                anomaly_signals.push(format!(
                    "CPU changed by {:.1} percentage points.",
                    cpu_change_percent
                ));
            }

            if memory_change_percent.abs() >= 10.0 {
                anomaly_signals.push(format!(
                    "Memory utilization changed by {:.1} percentage points.",
                    memory_change_percent
                ));
            }

            if disk_change_percent.abs() >= 5.0 {
                anomaly_signals.push(format!(
                    "Disk utilization changed by {:.1} percentage points.",
                    disk_change_percent
                ));
            }

            if !anomaly_signals.is_empty() {
                recommendations.push(
                    "A significant resource change was detected. Investigate the earliest abnormal signal before it becomes sustained pressure."
                        .to_string(),
                );
            } else if trend == "WORSENING" {
                recommendations.push(
                    "Multiple infrastructure signals are worsening. Investigate the underlying cause before service degradation occurs."
                        .to_string(),
                );
            }
        }

        IntelligenceResult {
            health_score,
            status: status.to_string(),
            cpu_status: cpu_status.to_string(),
            memory_status: memory_status.to_string(),
            disk_status: disk_status.to_string(),
            recommendations,
            trend,
            anomaly_detected: !anomaly_signals.is_empty(),
            anomaly_signals,
            cpu_change_percent,
            memory_change_percent,
            disk_change_percent,
        }
    }
}

fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f32 / total as f32) * 100.0
    }
}

fn percentage_point_change(
    current_used: u64,
    current_total: u64,
    previous_used: u64,
    previous_total: u64,
) -> f32 {
    percent(current_used, current_total) - percent(previous_used, previous_total)
}

fn threshold_status(value: f32, warning: f32, critical: f32) -> &'static str {
    if value >= critical {
        "CRITICAL"
    } else if value >= warning {
        "WARNING"
    } else {
        "HEALTHY"
    }
}

fn add_resource_recommendation(
    recommendations: &mut Vec<String>,
    resource: &str,
    status: &str,
    critical_action: &str,
    warning_action: &str,
) {
    match status {
        "CRITICAL" => recommendations.push(format!(
            "{resource} utilization is critically high. {critical_action}"
        )),
        "WARNING" => recommendations.push(format!(
            "{resource} utilization is elevated. {warning_action}"
        )),
        _ => {}
    }
}

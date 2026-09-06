use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventStatus {
    Scheduled,
    Countdown,
    Started,
    Monitoring,
    ExpectedCompletion,
    Verified,
    Updated,
    Missed,
    Failed,
    Cancelled,
}

impl Default for EventStatus {
    fn default() -> Self {
        Self::Scheduled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventSource {
    User,
    System,
    Kubernetes,
    Cloud,
    CiCd,
    Security,
    Database,
    Network,
    Device,
    Integration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonEvent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: EventSource,

    pub status: EventStatus,

    pub scheduled_at: Option<u64>,
    pub started_at: Option<u64>,
    pub expected_completion_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub updated_at: u64,

    pub expected_duration_seconds: Option<u64>,

    pub monitoring_enabled: bool,

    pub signals_monitored: u32,
    pub abnormal_signals: u32,

    pub outcome: Option<String>,
    pub evidence: Vec<String>,

    pub confidence: f32,
}

impl DaemonEvent {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        source: EventSource,
        scheduled_at: Option<u64>,
        expected_duration_seconds: Option<u64>,
    ) -> Self {
        let now = unix_timestamp();

        let expected_completion_at = match (scheduled_at, expected_duration_seconds) {
            (Some(start), Some(duration)) => Some(start.saturating_add(duration)),
            _ => None,
        };

        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            source,
            status: EventStatus::Scheduled,
            scheduled_at,
            started_at: None,
            expected_completion_at,
            completed_at: None,
            updated_at: now,
            expected_duration_seconds,
            monitoring_enabled: true,
            signals_monitored: 0,
            abnormal_signals: 0,
            outcome: None,
            evidence: Vec::new(),
            confidence: 0.0,
        }
    }

    pub fn start(&mut self) {
        let now = unix_timestamp();

        self.started_at = Some(now);
        self.status = EventStatus::Started;
        self.updated_at = now;
    }

    pub fn begin_monitoring(&mut self, signals: u32) {
        self.monitoring_enabled = true;
        self.signals_monitored = signals;
        self.status = EventStatus::Monitoring;
        self.updated_at = unix_timestamp();
    }

    pub fn mark_expected_completion(&mut self) {
        self.status = EventStatus::ExpectedCompletion;
        self.updated_at = unix_timestamp();
    }

    pub fn verify(&mut self, outcome: impl Into<String>, confidence: f32) {
        let now = unix_timestamp();

        self.completed_at = Some(now);
        self.status = EventStatus::Verified;
        self.outcome = Some(outcome.into());
        self.confidence = confidence.clamp(0.0, 1.0);
        self.updated_at = now;
    }

    pub fn update(&mut self, message: impl Into<String>) {
        let now = unix_timestamp();

        self.status = EventStatus::Updated;
        self.outcome = Some(message.into());
        self.updated_at = now;
    }

    pub fn mark_missed(&mut self) {
        self.status = EventStatus::Missed;
        self.updated_at = unix_timestamp();
    }

    pub fn mark_failed(&mut self, reason: impl Into<String>) {
        let now = unix_timestamp();

        self.status = EventStatus::Failed;
        self.outcome = Some(reason.into());
        self.updated_at = now;
    }

    pub fn add_evidence(&mut self, evidence: impl Into<String>) {
        self.evidence.push(evidence.into());
        self.updated_at = unix_timestamp();
    }

    pub fn add_abnormal_signal(&mut self) {
        self.abnormal_signals = self.abnormal_signals.saturating_add(1);
        self.updated_at = unix_timestamp();
    }

    pub fn is_overdue(&self, now: u64) -> bool {
        match self.expected_completion_at {
            Some(expected) => {
                now > expected
                    && self.completed_at.is_none()
                    && !matches!(
                        self.status,
                        EventStatus::Verified
                            | EventStatus::Updated
                            | EventStatus::Cancelled
                            | EventStatus::Failed
                    )
            }
            None => false,
        }
    }

    pub fn expected_update_seconds_remaining(&self, now: u64) -> Option<i64> {
        self.expected_completion_at
            .map(|expected| expected as i64 - now as i64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventSummary {
    pub total: usize,
    pub scheduled: usize,
    pub active: usize,
    pub completed: usize,
    pub missed: usize,
    pub failed: usize,
    pub abnormal: usize,
}

pub fn summarize(events: &[DaemonEvent]) -> EventSummary {
    let mut summary = EventSummary {
        total: events.len(),
        ..EventSummary::default()
    };

    for event in events {
        match event.status {
            EventStatus::Scheduled | EventStatus::Countdown => {
                summary.scheduled += 1;
            }

            EventStatus::Started | EventStatus::Monitoring | EventStatus::ExpectedCompletion => {
                summary.active += 1;
            }

            EventStatus::Verified | EventStatus::Updated => {
                summary.completed += 1;
            }

            EventStatus::Missed => {
                summary.missed += 1;
            }

            EventStatus::Failed => {
                summary.failed += 1;
            }

            EventStatus::Cancelled => {}
        }

        if event.abnormal_signals > 0 {
            summary.abnormal += 1;
        }
    }

    summary
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

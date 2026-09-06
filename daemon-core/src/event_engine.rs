use crate::events::{DaemonEvent, EventStatus};

/// Evaluates persisted events and advances their lifecycle based on real time.
pub struct EventEngine;

impl EventEngine {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate one event against the current Unix timestamp.
    /// Returns true when the event was changed.
    pub fn evaluate(&self, event: &mut DaemonEvent, now: u64) -> bool {
        let mut changed = false;

        if matches!(event.status, EventStatus::Scheduled) {
            if let Some(scheduled) = event.scheduled_at {
                if now >= scheduled {
                    event.start();
                    changed = true;
                } else if scheduled.saturating_sub(now) <= 300 {
                    event.status = EventStatus::Countdown;
                    event.updated_at = now;
                    changed = true;
                }
            }
        }

        if matches!(event.status, EventStatus::Started)
            && event.monitoring_enabled
            && event.signals_monitored == 0
        {
            event.begin_monitoring(0);
            changed = true;
        }

        if matches!(event.status, EventStatus::Started | EventStatus::Monitoring) {
            if let Some(expected) = event.expected_completion_at {
                if now >= expected {
                    event.mark_expected_completion();
                    changed = true;
                }
            }
        }

        if event.is_overdue(now)
            && !matches!(
                event.status,
                EventStatus::Missed
                    | EventStatus::Verified
                    | EventStatus::Updated
                    | EventStatus::Failed
                    | EventStatus::Cancelled
            )
        {
            event.mark_missed();
            changed = true;
        }

        changed
    }

    /// Evaluate all events and return the number that changed.
    pub fn evaluate_all(&self, events: &mut [DaemonEvent], now: u64) -> usize {
        let mut changed = 0;

        for event in events {
            if self.evaluate(event, now) {
                changed += 1;
            }
        }

        changed
    }
}

impl Default for EventEngine {
    fn default() -> Self {
        Self::new()
    }
}

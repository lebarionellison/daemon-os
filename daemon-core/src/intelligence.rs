pub mod bloom;

use bloom::ThreatShield;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ThreatResult {
    pub detected: bool,
    pub severity: String,
}

pub struct IntelligenceEngine {
    shield: ThreatShield,
}

impl IntelligenceEngine {
    pub fn new() -> Self {
        Self {
            shield: ThreatShield::new(10_000, 0.01),
        }
    }

    pub fn ingest_rule(&mut self, hashed_signature: &str) {
        self.shield.ingest_hashed_rule(hashed_signature);
    }

    pub fn evaluate(&self, signature: &str) -> ThreatResult {
        let detected = self.shield.evaluate_threat(signature);

        ThreatResult {
            detected,
            severity: if detected {
                "HIGH".to_string()
            } else {
                "NONE".to_string()
            },
        }
    }
}

impl Default for IntelligenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

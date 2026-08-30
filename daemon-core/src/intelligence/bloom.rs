use bloomfilter::Bloom;

pub struct ThreatShield {
    filter: Bloom<String>,
}

impl ThreatShield {
    pub fn new(items_capacity: usize, fp_rate: f64) -> Self {
        let filter = Bloom::new_for_fp_rate(items_capacity, fp_rate);
        Self { filter }
    }

    /// Ingests salted cryptographic hashes instead of raw plaintext signatures.
    pub fn ingest_hashed_rule(&mut self, hashed_signature: &str) {
        self.filter.set(&hashed_signature.to_string());
    }

    pub fn evaluate_threat(&self, signature: &str) -> bool {
        self.filter.check(&signature.to_string())
    }
}

use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
pub struct SecuritySnapshot {
    pub security_score: u8,
    pub processes: usize,
    pub high_cpu_processes: usize,
    pub high_memory_processes: usize,
    pub status: String,
}

pub fn snapshot() -> SecuritySnapshot {
    let mut system = System::new_all();
    system.refresh_all();

    let processes = system.processes();

    let high_cpu_processes = processes
        .values()
        .filter(|process| process.cpu_usage() >= 80.0)
        .count();

    let high_memory_processes = processes
        .values()
        .filter(|process| process.memory() >= 500 * 1024 * 1024)
        .count();

    let mut score: i32 = 100;

    score -= (high_cpu_processes as i32 * 5).min(30);
    score -= (high_memory_processes as i32 * 3).min(20);

    let security_score = score.clamp(0, 100) as u8;

    let status = if security_score >= 80 {
        "SECURE"
    } else if security_score >= 60 {
        "ELEVATED"
    } else {
        "AT RISK"
    };

    SecuritySnapshot {
        security_score,
        processes: processes.len(),
        high_cpu_processes,
        high_memory_processes,
        status: status.to_string(),
    }
}

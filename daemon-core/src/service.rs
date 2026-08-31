use crate::telemetry::Telemetry;
use std::thread;
use std::time::Duration;

pub fn run() {
    println!("Daemon OS background service starting...");

    let mut telemetry = Telemetry::new();

    loop {
        let snapshot = telemetry.snapshot();

        println!(
            "[DAEMON] CPU {:.1}% | MEMORY {:.1}% | DISK {:.1}% | UPTIME {}s",
            snapshot.cpu_usage,
            memory_percent(snapshot.memory_used, snapshot.memory_total),
            disk_percent(snapshot.disk_used, snapshot.disk_total),
            snapshot.uptime
        );

        thread::sleep(Duration::from_secs(5));
    }
}

fn memory_percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}

fn disk_percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}
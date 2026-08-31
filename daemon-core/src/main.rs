use daemon_core::telemetry::Telemetry;
use std::thread;
use std::time::Duration;

fn main() {
    println!("Daemon OS Core starting...");

    let mut telemetry = Telemetry::new();

    loop {
        let snapshot = telemetry.snapshot();

        println!(
            "CPU: {:.1}% | Memory: {:.1}% | Disk: {:.1}% | Uptime: {}s",
            snapshot.cpu_usage,
            memory_percent(snapshot.memory_used, snapshot.memory_total),
            disk_percent(snapshot.disk_used, snapshot.disk_total),
            snapshot.uptime
        );

        thread::sleep(Duration::from_secs(1));
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
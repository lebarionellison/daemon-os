use crate::{
    config::ConfigStore, history::HistoryStore, intelligence::IntelligenceEngine,
    telemetry::Telemetry,
};
use std::ffi::OsString;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

const SERVICE_NAME: &str = "DaemonOS";
const SERVICE_DISPLAY_NAME: &str = "Daemon OS";

define_windows_service!(ffi_service_main, service_main);

pub fn run() {
    if let Err(error) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
        eprintln!("Failed to start {SERVICE_DISPLAY_NAME} Windows service: {error}");
    }
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("{SERVICE_DISPLAY_NAME} service error: {error}");
    }
}

fn run_service() -> Result<(), windows_service::Error> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let status_handle =
        service_control_handler::register(
            SERVICE_NAME,
            move |control_event| match control_event {
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    let _ = shutdown_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    run_agent_loop(&shutdown_rx);

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

fn run_agent_loop(shutdown_rx: &mpsc::Receiver<()>) {
    let mut telemetry = Telemetry::new();
    let history = HistoryStore::new("data/history.jsonl");
    let config_store = ConfigStore::new("data/config.json");
    let engine = IntelligenceEngine::new();

    let mut last_history_write = 0_u64;

    loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        let config = match config_store.load() {
            Ok(config) => config,
            Err(error) => {
                eprintln!("[DAEMON] Configuration error: {error}");
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        if !config.monitoring_enabled {
            thread::sleep(Duration::from_secs(5));
            continue;
        }

        let snapshot = telemetry.snapshot();

        let intelligence = if config.intelligence_enabled {
            Some(engine.analyze(&snapshot, None, &config))
        } else {
            None
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if config.history_enabled
            && intelligence.is_some()
            && now.saturating_sub(last_history_write) >= config.history_interval_seconds
        {
            if let Some(result) = intelligence.clone() {
                if let Err(error) = history.append(snapshot.clone(), result) {
                    eprintln!("[DAEMON] History persistence error: {error}");
                } else {
                    last_history_write = now;
                }
            }
        }

        match intelligence {
            Some(result) => {
                println!(
                    "[DAEMON] CPU {:.1}% | MEMORY {:.1}% | DISK {:.1}% | HEALTH {} | STATUS {}",
                    snapshot.cpu_usage,
                    memory_percent(snapshot.memory_used, snapshot.memory_total),
                    disk_percent(snapshot.disk_used, snapshot.disk_total),
                    result.health_score,
                    result.status
                );
            }
            None => {
                println!(
                    "[DAEMON] CPU {:.1}% | MEMORY {:.1}% | DISK {:.1}% | INTELLIGENCE OFF",
                    snapshot.cpu_usage,
                    memory_percent(snapshot.memory_used, snapshot.memory_total),
                    disk_percent(snapshot.disk_used, snapshot.disk_total)
                );
            }
        }

        thread::sleep(Duration::from_secs(5));
    }
}

pub fn run_console() {
    println!("Daemon OS intelligence agent starting...");
    println!("Press Ctrl+C to stop.");

    let (_shutdown_tx, shutdown_rx) = mpsc::channel();

    run_agent_loop(&shutdown_rx);
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

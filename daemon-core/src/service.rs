use crate::telemetry::Telemetry;
use std::ffi::OsString;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
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
        eprintln!("Failed to start Daemon OS Windows service: {error}");
    }
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("Daemon OS service error: {error}");
    }
}

fn run_service() -> Result<(), windows_service::Error> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let status_handle = service_control_handler::register(
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

    let mut telemetry = Telemetry::new();

    loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

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
pub fn run_console() {
    println!("Daemon OS console service test starting...");

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
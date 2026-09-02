fn main() {
    if std::env::args().any(|arg| arg == "--console") {
        daemon_core::service::run_console();
    } else {
        daemon_core::service::run();
    }
}
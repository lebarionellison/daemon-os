use serde::Serialize;
use sysinfo::{Disks, System};

#[derive(Clone, Debug, Serialize)]
pub struct SystemSnapshot {
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub uptime: u64,
}

pub struct Telemetry {
    system: System,
    disks: Disks,
}

impl Telemetry {
    pub fn new() -> Self {
        let mut system = System::new_all();

        system.refresh_cpu_usage();
        system.refresh_memory();

        let disks = Disks::new_with_refreshed_list();

        Self { system, disks }
    }

    pub fn snapshot(&mut self) -> SystemSnapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.disks.refresh(true);

        let cpu_usage = self.system.global_cpu_usage();

        let memory_used = self.system.used_memory();
        let memory_total = self.system.total_memory();

        let (disk_used, disk_total) = self
            .disks
            .list()
            .iter()
            .fold((0u64, 0u64), |(used, total), disk| {
                let total_space = disk.total_space();
                let available_space = disk.available_space();
                let used_space = total_space.saturating_sub(available_space);

                (used + used_space, total + total_space)
            });

        SystemSnapshot {
            cpu_usage,
            memory_used,
            memory_total,
            disk_used,
            disk_total,
            uptime: System::uptime(),
        }
    }
}
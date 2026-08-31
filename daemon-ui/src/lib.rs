use daemon_core::telemetry::Telemetry;
use dioxus::prelude::*;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
struct DashboardData {
    cpu: f32,
    memory_used: u64,
    memory_total: u64,
    disk_used: u64,
    disk_total: u64,
    uptime: u64,
}

impl DashboardData {
    fn memory_percent(&self) -> f64 {
        if self.memory_total == 0 {
            0.0
        } else {
            (self.memory_used as f64 / self.memory_total as f64) * 100.0
        }
    }

    fn disk_percent(&self) -> f64 {
        if self.disk_total == 0 {
            0.0
        } else {
            (self.disk_used as f64 / self.disk_total as f64) * 100.0
        }
    }
}

#[component]
pub fn App() -> Element {
    let mut data = use_signal(|| DashboardData {
        cpu: 0.0,
        memory_used: 0,
        memory_total: 0,
        disk_used: 0,
        disk_total: 0,
        uptime: 0,
    });
use_effect(move || {
    let mut telemetry = Telemetry::new();

    spawn(async move {
        loop {
            let snapshot = telemetry.snapshot();

            data.set(DashboardData {
                cpu: snapshot.cpu_usage,
                memory_used: snapshot.memory_used,
                memory_total: snapshot.memory_total,
                disk_used: snapshot.disk_used,
                disk_total: snapshot.disk_total,
                uptime: snapshot.uptime,
            });

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
});
     
    let current = data();

    rsx! {
        style {
            r#"
            * {{
                box-sizing: border-box;
            }}

            body {{
                margin: 0;
                background: #080b10;
                color: #e8edf5;
                font-family: "Segoe UI", Arial, sans-serif;
            }}

            .app {{
                min-height: 100vh;
                padding: 28px;
                background: #080b10;
            }}

            .header {{
                display: flex;
                justify-content: space-between;
                align-items: center;
                margin-bottom: 28px;
            }}

            .brand {{
                display: flex;
                align-items: center;
                gap: 14px;
            }}

            .logo {{
                width: 46px;
                height: 46px;
                border-radius: 12px;
                display: flex;
                align-items: center;
                justify-content: center;
                background: #151c28;
                border: 1px solid #2b3545;
                font-size: 22px;
            }}

            .title {{
                font-size: 25px;
                font-weight: 700;
                letter-spacing: 1px;
            }}

            .subtitle {{
                color: #7f8da3;
                font-size: 13px;
                margin-top: 3px;
            }}

            .online {{
                display: flex;
                align-items: center;
                gap: 8px;
                padding: 9px 14px;
                border-radius: 20px;
                background: #101820;
                border: 1px solid #263440;
                color: #9fe0b0;
                font-size: 13px;
                font-weight: 600;
            }}

            .dot {{
                width: 9px;
                height: 9px;
                border-radius: 50%;
                background: #57c982;
                box-shadow: 0 0 10px #57c982;
            }}

            .grid {{
                display: grid;
                grid-template-columns: repeat(3, 1fr);
                gap: 16px;
                margin-bottom: 18px;
            }}

            .card {{
                background: #0e131b;
                border: 1px solid #202938;
                border-radius: 14px;
                padding: 20px;
            }}

            .label {{
                color: #7f8da3;
                font-size: 12px;
                text-transform: uppercase;
                letter-spacing: 1px;
                margin-bottom: 10px;
            }}

            .value {{
                font-size: 30px;
                font-weight: 700;
            }}

            .secondary {{
                color: #7f8da3;
                font-size: 12px;
                margin-top: 6px;
            }}

            .section {{
                display: grid;
                grid-template-columns: 1fr 1fr;
                gap: 18px;
            }}

            .section-title {{
                font-size: 15px;
                font-weight: 700;
                margin-bottom: 16px;
            }}

            .service {{
                display: flex;
                justify-content: space-between;
                align-items: center;
                padding: 13px 0;
                border-bottom: 1px solid #1b2330;
            }}

            .service:last-child {{
                border-bottom: 0;
            }}

            .service-name {{
                font-size: 14px;
            }}

            .running {{
                color: #8fd5a3;
                font-size: 12px;
                font-weight: 600;
            }}

            .event {{
                padding: 11px 0;
                border-bottom: 1px solid #1b2330;
                font-size: 13px;
            }}

            .event:last-child {{
                border-bottom: 0;
            }}

            .time {{
                color: #64748b;
                margin-right: 10px;
                font-family: Consolas, monospace;
            }}

            .footer {{
                margin-top: 18px;
                display: flex;
                justify-content: space-between;
                color: #566276;
                font-size: 12px;
            }}

            @media (max-width: 850px) {{
                .grid, .section {{
                    grid-template-columns: 1fr;
                }}
            }}
            "#
        }

        div { class: "app",

            div { class: "header",
                div { class: "brand",
                    div { class: "logo", "◈" }
                    div {
                        div { class: "title", "DAEMON OS" }
                        div { class: "subtitle", "Security & Intelligence Platform" }
                    }
                }

                div { class: "online",
                    span { class: "dot" }
                    "SYSTEM ONLINE"
                }
            }

            div { class: "grid",

                div { class: "card",
                    div { class: "label", "CPU" }
                    div { class: "value", "{current.cpu:.1}%" }
                    div { class: "secondary", "Live processor utilization" }
                }

                div { class: "card",
                    div { class: "label", "Memory" }
                    div { class: "value", "{current.memory_percent():.1}%" }
                    div {
                        class: "secondary",
                        "{current.memory_used / 1024 / 1024 / 1024:.1} GB / {current.memory_total / 1024 / 1024 / 1024:.1} GB"
                    }
                }

                div { class: "card",
                    div { class: "label", "Disk" }
                    div { class: "value", "{current.disk_percent():.1}%" }
                    div {
                        class: "secondary",
                        "{current.disk_used / 1024 / 1024 / 1024:.1} GB / {current.disk_total / 1024 / 1024 / 1024:.1} GB"
                    }
                }
            }

            div { class: "section",

                div { class: "card",
                    div { class: "section-title", "DAEMON SERVICES" }

                    div { class: "service",
                        span { class: "service-name", "Daemon Core" }
                        span { class: "running", "● RUNNING" }
                    }

                    div { class: "service",
                        span { class: "service-name", "Telemetry Engine" }
                        span { class: "running", "● RUNNING" }
                    }

                    div { class: "service",
                        span { class: "service-name", "Intelligence Engine" }
                        span { class: "running", "● READY" }
                    }

                    div { class: "service",
                        span { class: "service-name", "Security Monitor" }
                        span { class: "running", "● READY" }
                    }
                }

                div { class: "card",
                    div { class: "section-title", "LIVE EVENTS" }

                    div { class: "event",
                        span { class: "time", "LIVE" }
                        "Telemetry collection active"
                    }

                    div { class: "event",
                        span { class: "time", "CORE" }
                        "Daemon Core connected"
                    }

                    div { class: "event",
                        span { class: "time", "INTEL" }
                        "Intelligence engine ready"
                    }

                    div { class: "event",
                        span { class: "time", "UP" }
                        "System uptime: {current.uptime}s"
                    }
                }
            }

            div { class: "footer",
                span { "Daemon OS • Local Console" }
                span { "Live Telemetry" }
            }
        }
    }
}
//! Render the dashboard with a realistic snapshot to stdout for visual review.
use std::time::{Duration, SystemTime};

use mtop::model::*;
use mtop::theme::Theme;
use mtop::ui::{AppView, DashboardPage, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

fn main() {
    let w: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(150);
    let h: u16 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(45);

    let cores = 8;
    let snapshot = SystemSnapshot {
        captured_at: SystemTime::now(),
        uptime: Duration::from_secs(3 * 3600 + 27 * 60 + 12),
        load_average: [1.85, 1.42, 1.10],
        load_average_available: true,
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 37.5,
            per_core_percent: vec![62.0, 12.0, 88.0, 5.0, 44.0, 22.0, 71.0, 9.0],
            frequency_mhz: Some(3600),
            cpu_name: Some("Intel Core i7-5775C".into()),
            temperature_celsius: Some(58.0),
            per_core_temperature: (0..cores).map(|i| 50.0 + i as f32 * 2.0).collect(),
        },
        memory: MemorySnapshot {
            total_bytes: 16 * 1024 * 1024 * 1024,
            used_bytes: 9 * 1024 * 1024 * 1024 + 512 * 1024 * 1024,
            cached_bytes: Some(3 * 1024 * 1024 * 1024),
            swap_total_bytes: 4 * 1024 * 1024 * 1024,
            swap_used_bytes: 512 * 1024 * 1024,
        },
        disks: vec![
            DiskSnapshot {
                name: "/dev/disk0".into(),
                mount_point: "/".into(),
                total_bytes: 500 * 1024 * 1024 * 1024,
                available_bytes: 220 * 1024 * 1024 * 1024,
                read_bytes_per_second: Some(12 * 1024 * 1024),
                write_bytes_per_second: Some(3 * 1024 * 1024),
            },
            DiskSnapshot {
                name: "/dev/disk1".into(),
                mount_point: "/data".into(),
                total_bytes: 2 * 1024 * 1024 * 1024 * 1024,
                available_bytes: 900 * 1024 * 1024 * 1024,
                read_bytes_per_second: Some(0),
                write_bytes_per_second: Some(1024 * 512),
            },
        ],
        networks: vec![NetworkSnapshot {
            interface: "en0".into(),
            received_bytes_per_second: 2 * 1024 * 1024,
            transmitted_bytes_per_second: 512 * 1024,
            received_bytes_total: Some(9 * 1024 * 1024 * 1024),
            transmitted_bytes_total: Some(2 * 1024 * 1024 * 1024),
        }],
        processes: (0..40)
            .map(|i| ProcessSnapshot {
                pid: 100 + i,
                name: format!("process_{i}"),
                cpu_percent: (97.0 - i as f32 * 2.3).max(0.0),
                memory_bytes: (400 - i as u64 * 7) * 1024 * 1024,
                status: if i % 3 == 0 { "R" } else { "S" }.into(),
                user: Some(if i % 2 == 0 { "root" } else { "ronnakon" }.into()),
                elapsed_secs: Some(60 * (i as u64 + 1)),
                threads: Some(1 + i % 8),
                parent_pid: if i == 0 { None } else { Some(100) },
                command: String::new(),
            })
            .collect(),
        gpus: vec![mtop::model::GpuSnapshot {
            name: "Apple M5 (10-core)".into(),
            utilization_percent: Some(40.0),
            memory_used_bytes: Some(1_161_412_608),
            memory_total_bytes: Some(17_179_869_184),
            temperature_celsius: None,
            renderer_percent: Some(38.0),
            tiler_percent: Some(4.0),
        }],
        npus: vec![mtop::model::NpuSnapshot {
            name: "Apple Neural Engine".into(),
            utilization_percent: None,
            power_watts: Some(0.82),
            frequency_mhz: Some(900),
        }],
        power: mtop::model::PowerSnapshot {
            cpu_watts: Some(4.1),
            gpu_watts: Some(1.3),
            npu_watts: Some(0.82),
            dram_watts: Some(1.1),
            package_watts: Some(7.6),
            note: Some("Estimated on supported hardware".into()),
        },
        battery: None,
        warnings: vec![],
    };

    let theme = std::env::args().nth(3).unwrap_or_else(|| "neon".into());
    let page = match std::env::args().nth(4).as_deref() {
        Some("cpu") => DashboardPage::Cpu,
        Some("memory" | "mem") => DashboardPage::Memory,
        Some("gpu") => DashboardPage::Gpu,
        Some("npu") => DashboardPage::Npu,
        Some("network" | "net") => DashboardPage::Network,
        Some("processes" | "proc") => DashboardPage::Processes,
        Some("storage" | "disk") => DashboardPage::Storage,
        Some("power") => DashboardPage::Power,
        _ => DashboardPage::Overview,
    };
    mtop::ui::set_palette(mtop::ui::Palette::from_name(&theme));
    let mut view = AppView::new(snapshot.clone(), Theme::from_name(&theme));
    view.page = page;
    view.refresh_interval = Duration::from_secs(1);
    view.hostname = "workstation".into();
    // Populate history so graphs have data (simulate ~120 ticks).
    for t in 0..260u32 {
        let mut s = snapshot.clone();
        let phase = (t as f32) * 0.2;
        s.cpu.overall_percent = 40.0 + 35.0 * phase.sin();
        for (i, c) in s.cpu.per_core_percent.iter_mut().enumerate() {
            *c = 50.0 + 40.0 * (phase + i as f32).sin();
        }
        if let Some(g) = s.gpus.first_mut() {
            g.utilization_percent = Some(35.0 + 30.0 * ((t as f32) / 9.0).sin().abs());
        }
        s.captured_at = SystemTime::now() + Duration::from_millis(t as u64 + 1);
        view.accept_snapshot(s);
    }

    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw_dashboard(f, &view)).unwrap();
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..h {
        for x in 0..w {
            out.push_str(buf.cell((x, y)).unwrap().symbol());
        }
        out.push('\n');
    }
    print!("{out}");
}

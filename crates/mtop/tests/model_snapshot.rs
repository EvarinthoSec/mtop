use std::time::{Duration, SystemTime};

use mtop::model::{
    CpuSnapshot, DiskSnapshot, GpuSnapshot, MemorySnapshot, NetworkSnapshot, ProcessSnapshot,
    SystemSnapshot,
};

#[test]
fn system_snapshot_default_is_empty_safe() {
    let snapshot = SystemSnapshot::default();

    assert_eq!(snapshot.captured_at, SystemTime::UNIX_EPOCH);
    assert_eq!(snapshot.uptime, Duration::default());
    assert_eq!(snapshot.load_average, [0.0; 3]);
    assert!(!snapshot.load_average_available);
    assert_eq!(snapshot.cpu, CpuSnapshot::default());
    assert_eq!(snapshot.memory, MemorySnapshot::default());
    assert!(snapshot.disks.is_empty());
    assert!(snapshot.networks.is_empty());
    assert!(snapshot.processes.is_empty());
    assert!(snapshot.gpus.is_empty());
    assert!(snapshot.warnings.is_empty());
}

#[test]
fn snapshot_represents_cross_platform_metrics_and_optional_fields() {
    let snapshot = SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(42),
        uptime: Duration::from_secs(86_400),
        load_average: [0.5, 1.0, 1.5],
        load_average_available: true,
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 37.5,
            per_core_percent: vec![25.0, 50.0],
            frequency_mhz: Some(3_200),
            cpu_name: Some("Intel Core i7-5775C".into()),
            temperature_celsius: None,
            per_core_temperature: Vec::new(),
        },
        memory: MemorySnapshot {
            total_bytes: 16_000,
            used_bytes: 8_000,
            cached_bytes: None,
            swap_total_bytes: 4_000,
            swap_used_bytes: 500,
        },
        disks: vec![DiskSnapshot {
            name: "disk0".into(),
            mount_point: "/".into(),
            total_bytes: 1_000_000,
            available_bytes: 600_000,
            read_bytes_per_second: Some(12_000),
            write_bytes_per_second: None,
        }],
        networks: vec![NetworkSnapshot {
            interface: "en0".into(),
            received_bytes_per_second: 2_000,
            transmitted_bytes_per_second: 1_000,
            received_bytes_total: None,
            transmitted_bytes_total: None,
        }],
        processes: vec![ProcessSnapshot {
            user: None,
            elapsed_secs: None,
            pid: 42,
            name: "monitor".into(),
            cpu_percent: 2.5,
            memory_bytes: 4_096,
            status: "running".into(),
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        gpus: vec![GpuSnapshot {
            name: "Integrated GPU".into(),
            utilization_percent: Some(64.0),
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: Some(55),
            ..GpuSnapshot::default()
        }],
        npus: Vec::new(),
        power: Default::default(),
        battery: None,
        warnings: vec!["GPU memory unavailable".into()],
    };

    assert_eq!(snapshot.cpu.frequency_mhz, Some(3_200));
    assert_eq!(snapshot.disks[0].write_bytes_per_second, None);
    assert_eq!(snapshot.gpus[0].memory_total_bytes, None);
    assert_eq!(snapshot.processes[0].status, "running");
}

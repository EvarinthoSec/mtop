//! Shared application data models.

use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub struct SystemSnapshot {
    pub captured_at: SystemTime,
    pub uptime: Duration,
    pub load_average: [f32; 3],
    pub load_average_available: bool,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub disks: Vec<DiskSnapshot>,
    pub networks: Vec<NetworkSnapshot>,
    pub processes: Vec<ProcessSnapshot>,
    pub gpus: Vec<GpuSnapshot>,
    pub npus: Vec<NpuSnapshot>,
    pub power: PowerSnapshot,
    /// Primary battery, if the machine has one.
    pub battery: Option<BatterySnapshot>,
    pub warnings: Vec<String>,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            captured_at: SystemTime::UNIX_EPOCH,
            uptime: Duration::default(),
            load_average: [0.0; 3],
            load_average_available: false,
            cpu: CpuSnapshot::default(),
            memory: MemorySnapshot::default(),
            disks: Vec::new(),
            networks: Vec::new(),
            processes: Vec::new(),
            gpus: Vec::new(),
            npus: Vec::new(),
            power: PowerSnapshot::default(),
            battery: None,
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CpuSnapshot {
    pub available: bool,
    pub overall_percent: f32,
    pub per_core_percent: Vec<f32>,
    pub frequency_mhz: Option<u64>,
    /// CPU brand name e.g. "Intel Core i7-5775C"
    pub cpu_name: Option<String>,
    /// Overall CPU temperature in Celsius (if available)
    pub temperature_celsius: Option<f32>,
    /// Per-core temperatures (may be empty even when overall is available)
    pub per_core_temperature: Vec<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    /// Page-cache bytes (Linux /proc/meminfo Cached; None on macOS/BSD)
    pub cached_bytes: Option<u64>,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub read_bytes_per_second: Option<u64>,
    pub write_bytes_per_second: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetworkSnapshot {
    pub interface: String,
    pub received_bytes_per_second: u64,
    pub transmitted_bytes_per_second: u64,
    /// Cumulative bytes received since start (optional)
    pub received_bytes_total: Option<u64>,
    /// Cumulative bytes transmitted since start (optional)
    pub transmitted_bytes_total: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub status: String,
    pub user: Option<String>,
    pub elapsed_secs: Option<u64>,
    /// Thread / task count
    pub threads: Option<u32>,
    /// Parent PID
    pub parent_pid: Option<u32>,
    /// Full command line (btop "Command:" column); empty when unavailable.
    pub command: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuSnapshot {
    pub name: String,
    pub utilization_percent: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub temperature_celsius: Option<u32>,
    /// Apple "Renderer Utilization %" (fragment/compute engine).
    pub renderer_percent: Option<f32>,
    /// Apple "Tiler Utilization %" (geometry engine).
    pub tiler_percent: Option<f32>,
}

/// Accelerator telemetry for a neural-processing unit (NPU/ANE).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NpuSnapshot {
    pub name: String,
    /// `None` when the driver exposes power/frequency but not busy time.
    pub utilization_percent: Option<f32>,
    pub power_watts: Option<f32>,
    pub frequency_mhz: Option<u64>,
}

/// Best-effort power telemetry; values are only populated when the OS/driver
/// provides them. `package_watts` is a hardware package estimate, not total
/// wall-plug system power.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PowerSnapshot {
    pub cpu_watts: Option<f32>,
    pub gpu_watts: Option<f32>,
    pub npu_watts: Option<f32>,
    pub dram_watts: Option<f32>,
    pub package_watts: Option<f32>,
    /// Explains permission or platform limits when no power source is active.
    pub note: Option<String>,
}

/// Battery charge direction as btop shows it (▲ charging, ▼ discharging).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatterySnapshot {
    pub percent: f32,
    pub state: BatteryState,
    /// Estimated time until empty/full, when the OS reports one.
    pub seconds_left: Option<u64>,
}

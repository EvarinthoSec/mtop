//! Portable system integration backed by long-lived `sysinfo` state.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use crate::config::{Config, ProcessSort};
use crate::fan::{FanProvider, fan_provider_for};
use crate::model::{
    BatterySnapshot, BatteryState, CpuSnapshot, DiskSnapshot, GpuSnapshot, MemorySnapshot,
    NetworkSnapshot, ProcessSnapshot, SystemSnapshot,
};
use crate::npu::{NpuProvider, npu_provider_for};
use crate::windows_telemetry::PlatformSensors;
#[cfg(any(target_os = "macos", all(feature = "gpu-nvidia", target_os = "linux")))]
use anyhow::anyhow;
use mtop_core::SnapshotProvider;
use sysinfo::{
    Components, CpuRefreshKind, DiskRefreshKind, Disks, Networks, ProcessRefreshKind,
    ProcessesToUpdate, RefreshKind, System, UpdateKind, Users,
};

/// Capabilities exposed by the current target platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsCapabilities {
    pub os_name: &'static str,
    pub process_details: bool,
    pub disk_io_counters: bool,
    pub network_counters: bool,
    pub load_average: bool,
    pub gpu_platform_support: bool,
    pub gpu_backend_compiled: bool,
}

/// Return the compile-time capability contract for the current target.
pub fn current_os_capabilities() -> OsCapabilities {
    capabilities_for_target()
}

fn power_telemetry_note() -> String {
    if cfg!(target_os = "macos") {
        "Run mtop with sudo to read macOS component power; ANE utilization is not exposed by powermetrics."
    } else if cfg!(target_os = "windows") {
        "No Windows Energy Meter (RAPL) counters found; unavailable readings remain N/A."
    } else {
        "No supported component-power source is active; unavailable readings remain N/A."
    }
    .to_owned()
}

#[cfg(target_os = "macos")]
fn mac_core_count(name: &'static std::ffi::CStr) -> Option<usize> {
    let mut count = 0u32;
    let mut size = std::mem::size_of::<u32>();
    let result = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut count as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (result == 0 && count > 0).then_some(count as usize)
}

#[cfg(target_os = "macos")]
fn mac_core_counts() -> (Option<usize>, Option<usize>) {
    (
        mac_core_count(c"hw.perflevel0.logicalcpu"),
        mac_core_count(c"hw.perflevel1.logicalcpu"),
    )
}

#[cfg(target_os = "linux")]
fn capabilities_for_target() -> OsCapabilities {
    OsCapabilities {
        os_name: "linux",
        process_details: true,
        disk_io_counters: true,
        network_counters: true,
        load_average: true,
        gpu_platform_support: true,
        gpu_backend_compiled: cfg!(feature = "gpu-nvidia"),
    }
}

#[cfg(target_os = "macos")]
fn capabilities_for_target() -> OsCapabilities {
    OsCapabilities {
        os_name: "macos",
        process_details: true,
        disk_io_counters: true,
        network_counters: true,
        load_average: true,
        gpu_platform_support: false,
        gpu_backend_compiled: false,
    }
}

#[cfg(target_os = "windows")]
fn capabilities_for_target() -> OsCapabilities {
    OsCapabilities {
        os_name: "windows",
        process_details: true,
        disk_io_counters: true,
        network_counters: true,
        load_average: false,
        gpu_platform_support: true,
        gpu_backend_compiled: true,
    }
}

#[cfg(target_os = "freebsd")]
fn capabilities_for_target() -> OsCapabilities {
    bsd_capabilities("freebsd")
}

#[cfg(target_os = "openbsd")]
fn capabilities_for_target() -> OsCapabilities {
    bsd_capabilities("openbsd")
}

#[cfg(target_os = "netbsd")]
fn capabilities_for_target() -> OsCapabilities {
    bsd_capabilities("netbsd")
}

#[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
fn bsd_capabilities(os_name: &'static str) -> OsCapabilities {
    OsCapabilities {
        os_name,
        process_details: true,
        disk_io_counters: false,
        network_counters: true,
        load_average: true,
        gpu_platform_support: false,
        gpu_backend_compiled: false,
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
)))]
fn capabilities_for_target() -> OsCapabilities {
    OsCapabilities {
        os_name: "unknown",
        process_details: false,
        disk_io_counters: false,
        network_counters: false,
        load_average: false,
        gpu_platform_support: false,
        gpu_backend_compiled: false,
    }
}

/// An optional source of GPU snapshots.
pub trait GpuProvider: Send {
    fn collect(&mut self) -> anyhow::Result<Vec<GpuSnapshot>>;

    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// GPU provider used when GPU metrics are disabled or unavailable at compile time.
pub struct NoopGpuProvider;

impl GpuProvider for NoopGpuProvider {
    fn collect(&mut self) -> anyhow::Result<Vec<GpuSnapshot>> {
        Ok(Vec::new())
    }
}

/// Pick (package, per-core) CPU temperatures from raw sensor readings.
///
/// Handles Linux coretemp ("Package id 0", "Core N"), AMD k10temp ("Tctl"/
/// "Tdie") and Apple Silicon ("PMU tdieN" = CPU die sensors; "PMU2" is the
/// second power manager, "tdev" are board sensors). Readings outside
/// 0..=150 °C are dropped: some Apple sensors report garbage like -9202.
pub fn select_cpu_temps(sensors: &[(String, Option<f32>)]) -> (Option<f32>, Vec<f32>) {
    let plausible = |t: &Option<f32>| t.filter(|t| (0.0..=150.0).contains(t));
    let trailing_num = |s: &str| -> u32 {
        let digits: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
        digits
            .chars()
            .rev()
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    };
    let mut cores: Vec<(u32, f32)> = Vec::new();
    let mut package: Option<f32> = None;
    for (label, temp) in sensors {
        let Some(t) = plausible(temp) else { continue };
        let lbl = label.to_lowercase();
        let is_apple_die = lbl.starts_with("pmu tdie");
        let is_core = lbl.contains("core ") || is_apple_die;
        let is_package = lbl.contains("package")
            || lbl.contains("tctl")
            || lbl.contains("tdie")
            || lbl.contains("cpu");
        if is_core {
            cores.push((trailing_num(&lbl), t));
        }
        if is_core || is_package {
            package = Some(package.map_or(t, |p: f32| p.max(t)));
        }
    }
    cores.sort_by_key(|(n, _)| *n);
    (package, cores.into_iter().map(|(_, t)| t).collect())
}

/// Parse `vm_stat`: "File-backed pages" × page size = cached file bytes.
pub fn parse_vm_stat_cached(text: &str) -> Option<u64> {
    let page: u64 = text
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages: u64 = text
        .lines()
        .find(|l| l.starts_with("File-backed pages:"))?
        .split(':')
        .nth(1)?
        .trim()
        .trim_end_matches('.')
        .parse()
        .ok()?;
    Some(pages * page)
}

#[cfg(target_os = "macos")]
fn read_cached_memory() -> Option<u64> {
    let out = std::process::Command::new("/usr/bin/vm_stat")
        .output()
        .ok()?;
    parse_vm_stat_cached(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
fn read_cached_memory() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = text
        .lines()
        .find(|l| l.starts_with("Cached:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb * 1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_cached_memory() -> Option<u64> {
    None
}

/// Parse `pmset -g batt`. `None` when the machine reports no battery.
pub fn parse_pmset_batt(text: &str) -> Option<BatterySnapshot> {
    let line = text.lines().find(|l| l.contains("InternalBattery"))?;
    let fields = line.split('\t').nth(1)?;
    let mut parts = fields.split(';').map(str::trim);
    let percent: f32 = parts.next()?.trim_end_matches('%').parse().ok()?;
    let state_s = parts.next().unwrap_or("");
    let state = match state_s {
        "charging" | "finishing charge" => BatteryState::Charging,
        "discharging" => BatteryState::Discharging,
        "charged" | "AC attached" => BatteryState::Full,
        _ => BatteryState::Unknown,
    };
    let seconds_left = parts.next().and_then(|rest| {
        let hm = rest.split_whitespace().next()?;
        let (h, m) = hm.split_once(':')?;
        Some(h.parse::<u64>().ok()? * 3600 + m.parse::<u64>().ok()? * 60)
    });
    Some(BatterySnapshot {
        percent,
        state,
        seconds_left,
    })
}

/// Parse Linux `/sys/class/power_supply/BAT*/{capacity,status}` contents.
pub fn parse_sysfs_battery(capacity: &str, status: &str) -> Option<BatterySnapshot> {
    let percent: f32 = capacity.trim().parse().ok()?;
    let state = match status.trim() {
        "Charging" => BatteryState::Charging,
        "Discharging" => BatteryState::Discharging,
        "Full" | "Not charging" => BatteryState::Full,
        _ => BatteryState::Unknown,
    };
    Some(BatterySnapshot {
        percent,
        state,
        seconds_left: None,
    })
}

/// Convert fields from Win32 `SYSTEM_POWER_STATUS` into a battery snapshot.
/// The Win32 sentinel for unknown remaining time is `u32::MAX`.
const WIN32_BATTERY_FLAG_CHARGING: u8 = 0x08;
const WIN32_BATTERY_FLAG_NO_BATTERY: u8 = 0x80;
const WIN32_BATTERY_FLAG_UNKNOWN: u8 = 0xFF;

pub fn parse_windows_power_status(
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    battery_life_time: u32,
) -> Option<BatterySnapshot> {
    if battery_flag == WIN32_BATTERY_FLAG_NO_BATTERY || battery_life_percent > 100 {
        return None;
    }

    let state = if battery_flag == WIN32_BATTERY_FLAG_UNKNOWN {
        BatteryState::Unknown
    } else if battery_flag & WIN32_BATTERY_FLAG_CHARGING != 0 {
        BatteryState::Charging
    } else if battery_life_percent == 100 {
        BatteryState::Full
    } else if ac_line_status == 0 {
        BatteryState::Discharging
    } else {
        BatteryState::Unknown
    };
    let seconds_left = (state == BatteryState::Discharging && battery_life_time != u32::MAX)
        .then_some(battery_life_time as u64);

    Some(BatterySnapshot {
        percent: battery_life_percent as f32,
        state,
        seconds_left,
    })
}

#[cfg(target_os = "macos")]
fn read_battery() -> Option<BatterySnapshot> {
    let out = std::process::Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    parse_pmset_batt(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(target_os = "linux")]
fn read_battery() -> Option<BatterySnapshot> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    dir.flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("BAT"))
        .find_map(|e| {
            let p = e.path();
            let cap = std::fs::read_to_string(p.join("capacity")).ok()?;
            let st = std::fs::read_to_string(p.join("status")).unwrap_or_default();
            parse_sysfs_battery(&cap, &st)
        })
}

#[cfg(target_os = "windows")]
fn read_battery() -> Option<BatterySnapshot> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    // SAFETY: SYSTEM_POWER_STATUS is a C struct of integers; the API fills it
    // before any field is read.
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: status points to valid writable storage for SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }
    parse_windows_power_status(
        status.ACLineStatus,
        status.BatteryFlag,
        status.BatteryLifePercent,
        status.BatteryLifeTime,
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn read_battery() -> Option<BatterySnapshot> {
    None
}

/// Parse `ioreg -r -d 1 -c IOAccelerator` text into GPU snapshots. One
/// snapshot per `+-o` accelerator node. Pure so it is testable offline; Apple
/// Silicon exposes utilization and the GPU's unified-memory working set here,
/// but no temperature (that needs SMC access).
pub fn parse_ioreg_gpus(text: &str) -> Vec<GpuSnapshot> {
    fn quoted_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        let rest = line.trim().strip_prefix(&format!("\"{key}\" = "))?;
        Some(rest.trim().trim_matches('"'))
    }
    fn stat(stats: &str, key: &str) -> Option<u64> {
        // Match the exact `"key"=` token so "In use system memory" does not
        // pick up "In use system memory (driver)".
        let needle = format!("\"{key}\"=");
        let start = stats.find(&needle)? + needle.len();
        let digits: String = stats[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    }

    let mut gpus = Vec::new();
    let mut current: Option<(String, Option<u32>, String)> = None; // (model, cores, stats)
    let flush = |cur: Option<(String, Option<u32>, String)>, out: &mut Vec<GpuSnapshot>| {
        if let Some((model, cores, stats)) = cur {
            let name = match (model.is_empty(), cores) {
                (true, _) => "GPU".to_owned(),
                (false, Some(n)) => format!("{model} ({n}-core)"),
                (false, None) => model,
            };
            out.push(GpuSnapshot {
                name,
                utilization_percent: stat(&stats, "Device Utilization %")
                    .map(|v| v.min(100) as f32),
                memory_used_bytes: stat(&stats, "In use system memory"),
                memory_total_bytes: None,
                temperature_celsius: None,
                renderer_percent: stat(&stats, "Renderer Utilization %").map(|v| v.min(100) as f32),
                tiler_percent: stat(&stats, "Tiler Utilization %").map(|v| v.min(100) as f32),
            });
        }
    };
    for line in text.lines() {
        if line.trim_start().starts_with("+-o") {
            flush(current.take(), &mut gpus);
            current = Some((String::new(), None, String::new()));
            continue;
        }
        let Some(cur) = current.as_mut() else {
            continue;
        };
        if let Some(v) = quoted_value(line, "model") {
            cur.0 = v.to_owned();
        } else if let Some(v) = quoted_value(line, "gpu-core-count") {
            cur.1 = v.parse().ok();
        } else if line.contains("\"PerformanceStatistics\"") {
            cur.2 = line.to_owned();
        }
    }
    flush(current.take(), &mut gpus);
    gpus
}

/// Apple Silicon / macOS GPU provider backed by `ioreg` (no extra deps, no
/// root). One `ioreg` call is ~15ms and runs on the collector thread.
#[cfg(target_os = "macos")]
pub struct AppleGpuProvider {
    total_memory_bytes: u64,
}

#[cfg(target_os = "macos")]
impl AppleGpuProvider {
    pub fn new(total_memory_bytes: u64) -> Self {
        Self { total_memory_bytes }
    }
}

#[cfg(target_os = "macos")]
impl GpuProvider for AppleGpuProvider {
    fn collect(&mut self) -> anyhow::Result<Vec<GpuSnapshot>> {
        let output = std::process::Command::new("/usr/sbin/ioreg")
            .args(["-r", "-d", "1", "-c", "IOAccelerator"])
            .output()?;
        if !output.status.success() {
            return Err(anyhow!("ioreg exited with {}", output.status));
        }
        let mut gpus = parse_ioreg_gpus(&String::from_utf8_lossy(&output.stdout));
        // Unified memory: the GPU can address system RAM, so that is its ceiling.
        for gpu in &mut gpus {
            if self.total_memory_bytes > 0 {
                gpu.memory_total_bytes = Some(self.total_memory_bytes);
            }
        }
        Ok(gpus)
    }
}

#[cfg(all(feature = "gpu-nvidia", target_os = "linux"))]
struct NvidiaGpuProvider {
    nvml: Option<nvml_wrapper::Nvml>,
    initialization_error: Option<String>,
    warnings: Vec<String>,
}

#[cfg(all(feature = "gpu-nvidia", target_os = "linux"))]
impl NvidiaGpuProvider {
    fn new() -> Self {
        match nvml_wrapper::Nvml::init() {
            Ok(nvml) => Self {
                nvml: Some(nvml),
                initialization_error: None,
                warnings: Vec::new(),
            },
            Err(error) => Self {
                nvml: None,
                initialization_error: Some(error.to_string()),
                warnings: Vec::new(),
            },
        }
    }
}

#[cfg(all(feature = "gpu-nvidia", target_os = "linux"))]
impl GpuProvider for NvidiaGpuProvider {
    fn collect(&mut self) -> anyhow::Result<Vec<GpuSnapshot>> {
        self.warnings.clear();
        if let Some(error) = &self.initialization_error {
            return Err(anyhow!(error.clone()));
        }
        let nvml = self
            .nvml
            .as_ref()
            .ok_or_else(|| anyhow!("NVML is not initialized"))?;
        let count = nvml
            .device_count()
            .map_err(|error| anyhow!("NVML device count failed: {error}"))?;
        if count == 0 {
            return Err(anyhow!("no NVIDIA GPUs detected"));
        }

        let mut snapshots = Vec::with_capacity(count as usize);
        for index in 0..count {
            let device = match nvml.device_by_index(index) {
                Ok(device) => device,
                Err(error) => {
                    self.warnings
                        .push(format!("GPU device {index} unavailable: {error}"));
                    continue;
                }
            };
            let name = match device.name() {
                Ok(name) => name,
                Err(error) => {
                    self.warnings
                        .push(format!("GPU device {index} name unavailable: {error}"));
                    continue;
                }
            };
            let utilization_percent = match device.utilization_rates() {
                Ok(utilization) => Some(utilization.gpu as f32),
                Err(error) => {
                    self.warnings.push(format!(
                        "GPU device {index} utilization unavailable: {error}"
                    ));
                    None
                }
            };
            let (memory_used_bytes, memory_total_bytes) = match device.memory_info() {
                Ok(memory) => (Some(memory.used), Some(memory.total)),
                Err(error) => {
                    self.warnings
                        .push(format!("GPU device {index} memory unavailable: {error}"));
                    (None, None)
                }
            };
            let temperature_celsius = match device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            {
                Ok(temperature) => Some(temperature),
                Err(error) => {
                    self.warnings.push(format!(
                        "GPU device {index} temperature unavailable: {error}"
                    ));
                    None
                }
            };
            snapshots.push(GpuSnapshot {
                name,
                utilization_percent,
                memory_used_bytes,
                memory_total_bytes,
                temperature_celsius,
                ..GpuSnapshot::default()
            });
        }
        Ok(snapshots)
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Construct the configured best-effort GPU provider without failing startup.
pub fn gpu_provider_for(config: &Config) -> Box<dyn GpuProvider> {
    if !config.show_gpu {
        return Box::new(NoopGpuProvider);
    }

    #[cfg(all(feature = "gpu-nvidia", target_os = "linux"))]
    {
        return Box::new(NvidiaGpuProvider::new());
    }

    #[cfg(target_os = "macos")]
    {
        let total = System::new_with_specifics(
            RefreshKind::nothing().with_memory(sysinfo::MemoryRefreshKind::nothing().with_ram()),
        )
        .total_memory();
        return Box::new(AppleGpuProvider::new(total));
    }

    #[cfg(windows)]
    {
        return Box::new(crate::WindowsGpuProvider::new());
    }

    #[allow(unreachable_code)]
    Box::new(NoopGpuProvider)
}

/// Collects portable CPU, memory, storage, network, and process metrics.
pub struct SysinfoCollector {
    system: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    battery: Option<BatterySnapshot>,
    /// Cached-memory bytes (macOS vm_stat), re-read at most every 5s.
    cached_mem: Option<u64>,
    cached_mem_at: Option<Instant>,
    battery_read_at: Option<Instant>,
    process_sort: ProcessSort,
    previous_disks: HashMap<String, (u64, u64)>,
    previous_networks: HashMap<String, (u64, u64)>,
    previous_sample: Instant,
    cpu_sample_ready: bool,
    gpu_provider: Box<dyn GpuProvider>,
    npu_provider: Box<dyn NpuProvider>,
    /// Extra power/thermal sources `sysinfo` lacks (Windows RAPL, ACPI zones).
    platform_sensors: PlatformSensors,
    fan_provider: Box<dyn FanProvider>,
    users: Users,
    samples_since_user_refresh: u32,
}

impl SysinfoCollector {
    pub fn new(config: &Config) -> Self {
        Self::new_with_gpu_provider(config, gpu_provider_for(config))
    }

    pub fn new_with_gpu_provider(config: &Config, gpu_provider: Box<dyn GpuProvider>) -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
        );
        // Seed CPU counters and populate the per-core list before targeted refreshes.
        system.refresh_cpu_specifics(CpuRefreshKind::everything());

        Self {
            system,
            disks: Disks::new_with_refreshed_list_specifics(DiskRefreshKind::everything()),
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            battery: None,
            cached_mem: None,
            cached_mem_at: None,
            battery_read_at: None,
            process_sort: config.process_sort.clone(),
            previous_disks: HashMap::new(),
            previous_networks: HashMap::new(),
            previous_sample: Instant::now(),
            cpu_sample_ready: false,
            gpu_provider,
            npu_provider: npu_provider_for(),
            platform_sensors: PlatformSensors::new(),
            fan_provider: fan_provider_for(),
            users: Users::new_with_refreshed_list(),
            samples_since_user_refresh: 0,
        }
    }
}

impl SnapshotProvider for SysinfoCollector {
    fn collect(&mut self) -> SystemSnapshot {
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::everything());
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet),
        );
        // Removing missing disks prevents mounts removed since the last sample from lingering.
        self.disks
            .refresh_specifics(true, DiskRefreshKind::everything());
        self.networks.refresh(true);
        self.components.refresh(true);
        // Accounts change rarely; re-read the user list about once a minute.
        self.samples_since_user_refresh += 1;
        if self.samples_since_user_refresh >= 60 {
            self.users.refresh();
            self.samples_since_user_refresh = 0;
        }

        let observed_at = Instant::now();
        let elapsed = observed_at.duration_since(self.previous_sample);
        self.previous_sample = observed_at;

        let cpu_sample_ready = self.cpu_sample_ready;
        self.cpu_sample_ready = true;
        let mut warnings = Vec::new();
        if !cpu_sample_ready {
            warnings.push("CPU sample warming up".to_owned());
        }
        let disks = self.collect_disks(elapsed, &mut warnings);
        let networks = self.collect_networks(elapsed, &mut warnings);
        let processes = self.collect_processes();
        let cpus = self.system.cpus();
        if cpus.is_empty() {
            warnings.push("CPU data unavailable".to_owned());
        }
        let frequency_mhz = if cpus.is_empty() {
            None
        } else {
            Some(cpus.iter().map(|cpu| cpu.frequency()).sum::<u64>() / cpus.len() as u64)
        };
        #[cfg(target_os = "macos")]
        let (performance_core_count, efficiency_core_count) = mac_core_counts();
        #[cfg(not(target_os = "macos"))]
        let (performance_core_count, efficiency_core_count) = (None, None);
        let load = System::load_average();
        let gpus = match self.gpu_provider.collect() {
            Ok(gpus) => gpus,
            Err(error) => {
                warnings.push(format!("GPU unavailable: {error}"));
                Vec::new()
            }
        };
        warnings.extend(self.gpu_provider.take_warnings());
        let npus = self.npu_provider.collect();
        let fans = self.fan_provider.collect();
        let mut power = self.npu_provider.power_snapshot();

        let sensors: Vec<(String, Option<f32>)> = self
            .components
            .iter()
            .map(|c| (c.label().to_owned(), c.temperature()))
            .collect();
        let platform_readings = self.platform_sensors.sample();
        // sysinfo has no CPU sensors on Windows; fall back to PawnIO /
        // HWiNFO, then live ACPI thermal zones.
        let cpu_temps = match select_cpu_temps(&sensors) {
            (None, cores) if cores.is_empty() => platform_readings.cpu_temperatures(),
            temps => temps,
        };
        if platform_readings.has_power() {
            power = platform_readings.power.clone();
        } else if power.note.is_none() && !cfg!(target_os = "macos") {
            power.note = Some(power_telemetry_note());
        }

        SystemSnapshot {
            captured_at: SystemTime::now(),
            uptime: Duration::from_secs(System::uptime()),
            load_average: [load.one as f32, load.five as f32, load.fifteen as f32],
            load_average_available: current_os_capabilities().load_average,
            cpu: CpuSnapshot {
                available: cpu_sample_ready && !cpus.is_empty(),
                overall_percent: if cpu_sample_ready {
                    self.system.global_cpu_usage()
                } else {
                    0.0
                },
                per_core_percent: if cpu_sample_ready {
                    cpus.iter().map(|cpu| cpu.cpu_usage()).collect()
                } else {
                    vec![0.0; cpus.len()]
                },
                performance_core_count,
                efficiency_core_count,
                frequency_mhz,
                cpu_name: cpus
                    .first()
                    .map(|c| c.brand().trim().to_owned())
                    .filter(|s| !s.is_empty()),
                temperature_celsius: cpu_temps.0,
                per_core_temperature: cpu_temps.1,
            },
            memory: MemorySnapshot {
                total_bytes: self.system.total_memory(),
                used_bytes: self.system.used_memory(),
                cached_bytes: self.collect_cached_memory(),
                swap_total_bytes: self.system.total_swap(),
                swap_used_bytes: self.system.used_swap(),
            },
            disks,
            networks,
            processes,
            gpus,
            npus,
            power,
            fans,
            battery: self.collect_battery(),
            warnings,
        }
    }
}

impl SysinfoCollector {
    /// Battery state, re-read at most every 10s (pmset spawns a process).
    fn collect_battery(&mut self) -> Option<BatterySnapshot> {
        let stale = self
            .battery_read_at
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(10));
        if stale {
            self.battery = read_battery();
            self.battery_read_at = Some(Instant::now());
        }
        self.battery.clone()
    }

    /// Page-cache bytes. sysinfo has none on macOS, so read `vm_stat`
    /// file-backed pages (what Activity Monitor calls "Cached Files").
    fn collect_cached_memory(&mut self) -> Option<u64> {
        let stale = self
            .cached_mem_at
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(5));
        if stale {
            self.cached_mem = read_cached_memory();
            self.cached_mem_at = Some(Instant::now());
        }
        self.cached_mem
    }

    fn collect_disks(
        &mut self,
        elapsed: Duration,
        warnings: &mut Vec<String>,
    ) -> Vec<DiskSnapshot> {
        let mut current = HashMap::new();
        let mut snapshots = Vec::with_capacity(self.disks.len());

        for disk in &self.disks {
            let key = format!(
                "{}\0{}",
                disk.name().to_string_lossy(),
                disk.mount_point().display()
            );
            let usage = disk.usage();
            let read = self
                .previous_disks
                .get(&key)
                .and_then(|(previous, _)| rate_per_second(*previous, usage.read_bytes, elapsed));
            let write = self
                .previous_disks
                .get(&key)
                .and_then(|(_, previous)| rate_per_second(*previous, usage.written_bytes, elapsed));
            if (read.is_none() || write.is_none()) && self.previous_disks.contains_key(&key) {
                warnings.push(format!(
                    "disk counter reset: {}",
                    disk.mount_point().display()
                ));
            }
            current.insert(key, (usage.read_bytes, usage.written_bytes));
            snapshots.push(DiskSnapshot {
                name: disk.name().to_string_lossy().into_owned(),
                mount_point: disk.mount_point().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                read_bytes_per_second: read,
                write_bytes_per_second: write,
            });
        }
        self.previous_disks = current;
        snapshots.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
        snapshots
    }

    fn collect_networks(
        &mut self,
        elapsed: Duration,
        warnings: &mut Vec<String>,
    ) -> Vec<NetworkSnapshot> {
        let mut current = HashMap::new();
        let mut snapshots = Vec::with_capacity(self.networks.len());

        for (interface, network) in &self.networks {
            let received_rate = self
                .previous_networks
                .get(interface)
                .and_then(|(previous, _)| {
                    rate_per_second(*previous, network.total_received(), elapsed)
                });
            let transmitted_rate =
                self.previous_networks
                    .get(interface)
                    .and_then(|(_, previous)| {
                        rate_per_second(*previous, network.total_transmitted(), elapsed)
                    });
            if self.previous_networks.contains_key(interface)
                && (received_rate.is_none() || transmitted_rate.is_none())
            {
                warnings.push(format!("network counter unavailable: {interface}"));
            }
            let received = received_rate.unwrap_or(0);
            let transmitted = transmitted_rate.unwrap_or(0);
            current.insert(
                interface.clone(),
                (network.total_received(), network.total_transmitted()),
            );
            snapshots.push(NetworkSnapshot {
                interface: interface.clone(),
                received_bytes_per_second: received,
                transmitted_bytes_per_second: transmitted,
                received_bytes_total: Some(network.total_received()),
                transmitted_bytes_total: Some(network.total_transmitted()),
            });
        }
        self.previous_networks = current;
        snapshots.sort_by(|left, right| left.interface.cmp(&right.interface));
        snapshots
    }

    fn collect_processes(&self) -> Vec<ProcessSnapshot> {
        const COLLECTOR_HARD_CAP: usize = 10_000;
        let processes = self
            .system
            .processes()
            .values()
            .map(|process| ProcessSnapshot {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                cpu_percent: process.cpu_usage(),
                memory_bytes: process.memory(),
                status: format!("{:?}", process.status()),
                user: process.user_id().map(|uid| {
                    self.users
                        .get_user_by_id(uid)
                        .map(|u| u.name().to_owned())
                        .unwrap_or_else(|| format!("{}", **uid))
                }),
                elapsed_secs: Some(process.run_time()),
                threads: Some(process.tasks().map(|t| t.len() as u32).unwrap_or(1)),
                parent_pid: process.parent().map(|p| p.as_u32()),
                command: process
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
            })
            .collect();
        // Keep a deterministic seed order; the UI re-sorts and applies the
        // user-visible limit. Hard cap only guards pathological process counts.
        sort_processes(processes, self.process_sort.clone(), COLLECTOR_HARD_CAP)
    }
}

/// Return a non-negative counter delta expressed as units per second.
///
/// A zero elapsed duration or counter reset returns `None`; callers use zero or
/// `None` in the model because an invented rate is less useful than no sample.
pub fn rate_per_second(previous: u64, current: u64, elapsed: Duration) -> Option<u64> {
    let delta = current.checked_sub(previous)?;
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }
    Some((delta as f64 / seconds).min(u64::MAX as f64) as u64)
}

/// Sort and truncate process snapshots with deterministic tie-breakers.
pub fn sort_processes(
    mut processes: Vec<ProcessSnapshot>,
    sort: ProcessSort,
    limit: usize,
) -> Vec<ProcessSnapshot> {
    processes.sort_by(|left, right| {
        let primary = match sort {
            ProcessSort::Cpu => right.cpu_percent.total_cmp(&left.cpu_percent),
            ProcessSort::Memory => right.memory_bytes.cmp(&left.memory_bytes),
            ProcessSort::Pid => left.pid.cmp(&right.pid),
            ProcessSort::Name => left.name.cmp(&right.name),
        };
        primary
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.pid.cmp(&right.pid))
    });
    processes.truncate(limit);
    processes
}

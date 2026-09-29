//! Windows GPU/NPU, temperature, and power telemetry.
//!
//! Utilization and memory come from the same PDH counters Task Manager uses
//! (`GPU Engine`, `GPU Adapter Memory`); adapter names, classification and
//! temperature come from `D3DKMTQueryAdapterInfo`. Power uses the RAPL rails
//! Windows publishes as `Energy Meter` counters, and temperatures fall back to
//! ACPI `Thermal Zone Information`. Everything is best-effort and user-mode:
//! missing counters simply leave readings unavailable.
//!
//! The parsers below are portable so they can be unit-tested on any host.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::model::PowerSnapshot;

/// Adapter LUID as printed in PDH instance names (`luid_0xHIGH_0xLOW`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AdapterLuid {
    pub high: u32,
    pub low: u32,
}

impl fmt::Display for AdapterLuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}_0x{:08X}", self.high, self.low)
    }
}

/// One `GPU Engine` counter instance, e.g.
/// `pid_1234_luid_0x00000000_0x0000D1B5_phys_0_eng_3_engtype_VideoDecode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuEngineInstance {
    pub pid: Option<u32>,
    pub luid: AdapterLuid,
    pub phys: u32,
    pub engine: u32,
    pub engine_type: String,
}

fn parse_hex(text: &str) -> Option<u32> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;
    u32::from_str_radix(digits, 16).ok()
}

/// Parse the `luid_0xHIGH_0xLOW_phys_N` part shared by all GPU counter sets.
/// Returns the LUID, physical adapter index, and the unparsed remainder.
fn parse_luid_phys(text: &str) -> Option<(AdapterLuid, u32, &str)> {
    let rest = text.strip_prefix("luid_")?;
    let (high, rest) = rest.split_once('_')?;
    let (low, rest) = rest.split_once('_')?;
    let rest = rest.strip_prefix("phys_")?;
    let (phys, rest) = rest.split_once('_').unwrap_or((rest, ""));
    Some((
        AdapterLuid {
            high: parse_hex(high)?,
            low: parse_hex(low)?,
        },
        phys.parse().ok()?,
        rest,
    ))
}

/// Parse a `GPU Engine` instance name (case-insensitive prefixes).
pub fn parse_gpu_engine_instance(name: &str) -> Option<GpuEngineInstance> {
    let lower = name.to_ascii_lowercase();
    let (pid, rest) = match lower.strip_prefix("pid_") {
        Some(rest) => {
            let (pid, rest) = rest.split_once('_')?;
            (Some(pid.parse().ok()?), rest)
        }
        None => (None, lower.as_str()),
    };
    let (luid, phys, rest) = parse_luid_phys(rest)?;
    let rest = rest.strip_prefix("eng_")?;
    let (engine, rest) = rest.split_once('_')?;
    let engine_type = rest.strip_prefix("engtype_").unwrap_or("");
    // Preserve the original spelling of the engine type ("VideoDecode").
    let engine_type = &name[name.len() - engine_type.len()..];
    Some(GpuEngineInstance {
        pid,
        luid,
        phys,
        engine: engine.parse().ok()?,
        engine_type: engine_type.to_owned(),
    })
}

/// Parse a `GPU Adapter Memory` / `GPU Local Adapter Memory` instance name
/// (`luid_0x00000000_0x0000D1B5_phys_0[_part_0]`).
pub fn parse_gpu_adapter_instance(name: &str) -> Option<(AdapterLuid, u32)> {
    let lower = name.to_ascii_lowercase();
    let (luid, phys, _) = parse_luid_phys(&lower)?;
    Some((luid, phys))
}

/// Busy percentage per adapter, computed the way Task Manager does: sum each
/// engine across processes, then report the busiest engine of the adapter.
pub fn gpu_utilization_by_adapter(samples: &[(String, f64)]) -> BTreeMap<AdapterLuid, f32> {
    let mut engines: HashMap<(AdapterLuid, u32, u32), f64> = HashMap::new();
    for (name, value) in samples {
        let Some(instance) = parse_gpu_engine_instance(name) else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        *engines
            .entry((instance.luid, instance.phys, instance.engine))
            .or_default() += value.max(0.0);
    }
    let mut adapters: BTreeMap<AdapterLuid, f32> = BTreeMap::new();
    for ((luid, _, _), busy) in engines {
        let busy = busy.clamp(0.0, 100.0) as f32;
        let entry = adapters.entry(luid).or_default();
        *entry = entry.max(busy);
    }
    adapters
}

/// Sum a per-adapter counter (memory usage) across physical adapters.
pub fn sum_by_adapter(samples: &[(String, f64)]) -> BTreeMap<AdapterLuid, u64> {
    let mut totals: BTreeMap<AdapterLuid, u64> = BTreeMap::new();
    for (name, value) in samples {
        if let Some((luid, _)) = parse_gpu_adapter_instance(name)
            && value.is_finite()
            && *value >= 0.0
        {
            *totals.entry(luid).or_default() += *value as u64;
        }
    }
    totals
}

/// How an adapter reported by D3DKMT should be presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsAdapterClass {
    Gpu,
    Npu,
    /// Software renderers and display-only (indirect display) adapters.
    Ignored,
}

/// `D3DKMT_ADAPTERTYPE` bitfield positions (d3dkmthk.h).
pub mod adapter_type_bits {
    pub const RENDER_SUPPORTED: u32 = 1 << 0;
    pub const DISPLAY_SUPPORTED: u32 = 1 << 1;
    pub const SOFTWARE_DEVICE: u32 = 1 << 2;
    pub const INDIRECT_DISPLAY_DEVICE: u32 = 1 << 6;
    pub const COMPUTE_ONLY: u32 = 1 << 11;
}

fn looks_like_npu_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["npu", "neural", "ai boost", "ryzen ai", "hexagon", "vpu"]
        .iter()
        .any(|needle| lower.contains(needle))
}

/// Classify an adapter from its `D3DKMT_ADAPTERTYPE` bits (when known), its
/// dedicated VRAM size and its name. Compute-only (MCDM) adapters are NPUs
/// unless they carry dedicated VRAM, which marks a datacenter GPU in MCDM mode.
pub fn classify_windows_adapter(
    type_bits: Option<u32>,
    dedicated_video_bytes: u64,
    name: &str,
) -> WindowsAdapterClass {
    use adapter_type_bits::*;
    let Some(bits) = type_bits else {
        return if looks_like_npu_name(name) {
            WindowsAdapterClass::Npu
        } else {
            WindowsAdapterClass::Gpu
        };
    };
    if bits & (SOFTWARE_DEVICE | INDIRECT_DISPLAY_DEVICE) != 0 {
        return WindowsAdapterClass::Ignored;
    }
    if bits & COMPUTE_ONLY != 0 {
        return if dedicated_video_bytes == 0 || looks_like_npu_name(name) {
            WindowsAdapterClass::Npu
        } else {
            WindowsAdapterClass::Gpu
        };
    }
    if bits & RENDER_SUPPORTED == 0 {
        return WindowsAdapterClass::Ignored;
    }
    WindowsAdapterClass::Gpu
}

/// Integrated GPUs carve out a token amount of "dedicated" memory and really
/// work from shared system memory; below this size report the shared pool.
const INTEGRATED_DEDICATED_LIMIT: u64 = 512 * 1024 * 1024;

/// Pick (used, total) GPU memory from dedicated and shared pools.
pub fn windows_gpu_memory(
    dedicated_used: Option<u64>,
    shared_used: Option<u64>,
    dedicated_size: u64,
    shared_size: u64,
) -> (Option<u64>, Option<u64>) {
    if dedicated_size >= INTEGRATED_DEDICATED_LIMIT {
        (dedicated_used, Some(dedicated_size))
    } else if shared_size > 0 {
        let used = match (dedicated_used, shared_used) {
            (None, None) => None,
            (dedicated, shared) => Some(dedicated.unwrap_or(0) + shared.unwrap_or(0)),
        };
        (used, Some(shared_size + dedicated_size))
    } else {
        (dedicated_used.or(shared_used), None)
    }
}

/// Convert `D3DKMT_ADAPTER_PERFDATA::Temperature` (deci-°C, 0 = unsupported).
pub fn deci_celsius_to_celsius(value: u32) -> Option<u32> {
    (value > 0 && value <= 1500).then(|| (value + 5) / 10)
}

/// Map `Energy Meter` RAPL rails (`RAPL_Package0_PKG`, `_PP0`, `_PP1`,
/// `_DRAM`) to watts. `power_mw` is the `Power` counter in milliwatts; rails
/// whose cumulative `Energy` is zero are treated as unsupported.
pub fn rapl_power_from_samples(
    power_mw: &[(String, f64)],
    energy: &[(String, f64)],
) -> PowerSnapshot {
    let supported: HashMap<String, bool> = energy
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), *value > 0.0))
        .collect();
    let mut power = PowerSnapshot::default();
    for (name, value) in power_mw {
        let lower = name.to_ascii_lowercase();
        if !lower.starts_with("rapl_") || !value.is_finite() || *value < 0.0 {
            continue;
        }
        if supported.get(&lower) == Some(&false) {
            continue;
        }
        let slot = if lower.ends_with("_pkg") {
            &mut power.package_watts
        } else if lower.ends_with("_pp0") {
            &mut power.cpu_watts
        } else if lower.ends_with("_pp1") {
            &mut power.gpu_watts
        } else if lower.ends_with("_dram") {
            &mut power.dram_watts
        } else {
            continue;
        };
        *slot = Some(slot.unwrap_or(0.0) + (*value / 1000.0) as f32);
    }
    power
}

/// Convert ACPI thermal-zone readings to °C. `tenths_kelvin` selects the
/// `High Precision Temperature` unit instead of whole kelvin. Implausible
/// values (outside 1..=150 °C) are dropped.
pub fn thermal_zones_celsius(samples: &[(String, f64)], tenths_kelvin: bool) -> Vec<(String, f32)> {
    samples
        .iter()
        .filter_map(|(name, value)| {
            let kelvin = if tenths_kelvin { value / 10.0 } else { *value };
            let celsius = (kelvin - 273.15) as f32;
            (celsius.is_finite() && (1.0..=150.0).contains(&celsius))
                .then(|| (name.clone(), celsius))
        })
        .collect()
}

/// Keeps only ACPI thermal zones whose reading has moved at least once.
///
/// Many desktop boards publish a firmware placeholder zone (`\_TZ.TZ00` stuck
/// at 301 K ≈ 27.85 °C) that never changes; showing it as a CPU temperature
/// is worse than showing nothing.
#[derive(Clone, Debug, Default)]
pub struct ThermalZoneTracker {
    first: HashMap<String, f32>,
    live: std::collections::HashSet<String>,
}

impl ThermalZoneTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, zones: Vec<(String, f32)>) -> Vec<(String, f32)> {
        zones
            .into_iter()
            .filter(|(name, celsius)| {
                if self.live.contains(name) {
                    return true;
                }
                let first = *self.first.entry(name.clone()).or_insert(*celsius);
                if (first - celsius).abs() > 0.05 {
                    self.live.insert(name.clone());
                    true
                } else {
                    false
                }
            })
            .collect()
    }
}

/// `SENSOR_READING_TYPE::SENSOR_TYPE_TEMP` in the HWiNFO shared-memory ABI.
pub const HWINFO_READING_TEMP: u32 = 1;
/// `"HWiS"` little-endian; `"DEAD"` means HWiNFO stopped sharing.
const HWINFO_SIGNATURE: u32 = 0x5369_5748;

/// One reading from the HWiNFO `Global\HWiNFO_SENS_SM2` shared memory.
#[derive(Clone, Debug, PartialEq)]
pub struct HwinfoReading {
    /// Original (untranslated, un-renamed) sensor name, e.g. `CPU [#0]: Intel Core i5-12400: DTS`.
    pub sensor: String,
    /// Original reading label, e.g. `Core #0` or `CPU Package`.
    pub label: String,
    pub unit: String,
    pub kind: u32,
    pub value: f64,
}

/// Parse the packed `HWiNFO_SENSORS_SHARED_MEM2` layout. Returns `None` when
/// the header is missing, inactive, or points outside `bytes`.
pub fn parse_hwinfo_shared_memory(bytes: &[u8]) -> Option<Vec<HwinfoReading>> {
    let u32_at = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let f64_at = |offset: usize| -> Option<f64> {
        Some(f64::from_le_bytes(
            bytes.get(offset..offset + 8)?.try_into().ok()?,
        ))
    };
    let str_at = |offset: usize, len: usize| -> Option<String> {
        let raw = bytes.get(offset..offset + len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        // Legacy fields are ANSI; map bytes 1:1 so `°` (0xB0) survives.
        Some(
            raw[..end]
                .iter()
                .map(|&b| b as char)
                .collect::<String>()
                .trim()
                .to_owned(),
        )
    };

    if u32_at(0)? != HWINFO_SIGNATURE {
        return None;
    }
    // Header: signature, version, revision, poll_time (i64), then the
    // sensor/reading section descriptors.
    let sensor_offset = u32_at(20)? as usize;
    let sensor_size = u32_at(24)? as usize;
    let sensor_count = u32_at(28)? as usize;
    let reading_offset = u32_at(32)? as usize;
    let reading_size = u32_at(36)? as usize;
    let reading_count = u32_at(40)? as usize;
    // SensorID, SensorInst, szSensorNameOrig[128], szSensorNameUser[128].
    const SENSOR_MIN: usize = 8 + 128 + 128;
    // tReading, SensorIndex, ReadingID, szLabelOrig[128], szLabelUser[128],
    // szUnit[16], Value, ValueMin, ValueMax, ValueAvg.
    const READING_MIN: usize = 12 + 128 + 128 + 16 + 32;
    if sensor_size < SENSOR_MIN || reading_size < READING_MIN {
        return None;
    }

    let sensors: Vec<String> = (0..sensor_count)
        .map(|i| str_at(sensor_offset + i * sensor_size + 8, 128))
        .collect::<Option<_>>()?;
    (0..reading_count)
        .map(|i| {
            let base = reading_offset + i * reading_size;
            let sensor_index = u32_at(base + 4)? as usize;
            Some(HwinfoReading {
                sensor: sensors.get(sensor_index).cloned().unwrap_or_default(),
                label: str_at(base + 12, 128)?,
                unit: str_at(base + 268, 16)?,
                kind: u32_at(base)?,
                value: f64_at(base + 284)?,
            })
        })
        .collect()
}

/// (label, °C) CPU temperature readings from HWiNFO's first CPU socket.
pub fn hwinfo_cpu_temperatures(readings: &[HwinfoReading]) -> Vec<(String, f32)> {
    readings
        .iter()
        .filter(|r| r.kind == HWINFO_READING_TEMP && r.sensor.starts_with("CPU [#0]"))
        .map(|r| {
            let celsius = if r.unit.ends_with('F') {
                (r.value - 32.0) * 5.0 / 9.0
            } else {
                r.value
            };
            (r.label.clone(), celsius as f32)
        })
        .collect()
}

/// Intel digital thermal sensor reading: `TjMax - readout`.
///
/// `temperature_target` is `MSR_TEMPERATURE_TARGET` (0x1A2, TjMax in bits
/// 23:16); `therm_status` is `IA32_THERM_STATUS` (0x19C) or
/// `IA32_PACKAGE_THERM_STATUS` (0x1B1), readout in bits 22:16. Only the
/// per-core register has a "reading valid" flag (bit 31).
pub fn intel_dts_celsius(
    temperature_target: u64,
    therm_status: u64,
    check_valid: bool,
) -> Option<f32> {
    if check_valid && therm_status & (1 << 31) == 0 {
        return None;
    }
    let tjmax = match (temperature_target >> 16) & 0xFF {
        0 => 100,
        tjmax => tjmax,
    };
    let readout = (therm_status >> 16) & 0x7F;
    let celsius = tjmax.checked_sub(readout)? as f32;
    (1.0..=150.0).contains(&celsius).then_some(celsius)
}

/// CPU package and per-core temperatures from a sensor tool.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CpuTemperatures {
    pub package: Option<f32>,
    /// One entry per physical core, in core order.
    pub cores: Vec<f32>,
}

/// Core index from a per-core label such as `Core #0`, `CPU Core #3`,
/// `P-core 2`, `E-Core #1` or `Core0 (CCD1)`. Returns (is E-core, index);
/// labels like `Core Max` or `Core #0 Distance to TjMAX` are rejected.
fn core_label_index(label: &str) -> Option<(bool, u32)> {
    let mut rest = label.trim().to_ascii_lowercase();
    if let Some(open) = rest.find(" (") {
        rest.truncate(open);
    }
    let rest = rest.strip_prefix("cpu ").unwrap_or(&rest);
    let rest = rest.strip_suffix(" temperature").unwrap_or(rest);
    let (efficiency, rest) = if let Some(r) = rest.strip_prefix("e-") {
        (true, r)
    } else {
        (false, rest.strip_prefix("p-").unwrap_or(rest))
    };
    let digits = rest
        .strip_prefix("core")?
        .trim_start()
        .trim_start_matches('#')
        .trim();
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok().map(|n| (efficiency, n)))
        .flatten()
}

/// Pick package and per-core CPU temperatures from HWiNFO (or
/// LibreHardwareMonitor-style) labels. Returns `None` if nothing CPU-like is found.
pub fn select_sensor_cpu_temps(readings: &[(String, f32)]) -> Option<CpuTemperatures> {
    const PACKAGE_LABELS: [&str; 7] = [
        "cpu package",
        "package",
        "cpu (tctl/tdie)",
        "core (tctl/tdie)",
        "cpu (tctl)",
        "cpu die (average)",
        "core max",
    ];
    let plausible = |t: f32| t.is_finite() && (1.0..=150.0).contains(&t);
    let mut cores: Vec<((bool, u32), f32)> = readings
        .iter()
        .filter(|(_, t)| plausible(*t))
        .filter_map(|(label, t)| Some((core_label_index(label)?, *t)))
        .collect();
    cores.sort_by_key(|(key, _)| *key);
    cores.dedup_by_key(|(key, _)| *key);
    let cores: Vec<f32> = cores.into_iter().map(|(_, t)| t).collect();

    let package = PACKAGE_LABELS
        .iter()
        .find_map(|want| {
            readings
                .iter()
                .find(|(label, t)| plausible(*t) && label.trim().eq_ignore_ascii_case(want))
                .map(|(_, t)| *t)
        })
        .or_else(|| cores.iter().copied().reduce(f32::max));
    (package.is_some() || !cores.is_empty()).then_some(CpuTemperatures { package, cores })
}

/// Spread physical-core temperatures over logical CPUs. `logical_to_core[i]`
/// is the physical core of logical CPU `i` (from the OS topology). When the
/// sensor core count doesn't match the topology, temperatures are returned
/// unchanged.
pub fn core_temps_per_logical_cpu(core_temps: &[f32], logical_to_core: &[usize]) -> Vec<f32> {
    let core_count = logical_to_core.iter().max().map_or(0, |max| max + 1);
    if core_count != core_temps.len() {
        return core_temps.to_vec();
    }
    logical_to_core
        .iter()
        .map(|&core| core_temps[core])
        .collect()
}

/// Power and temperature readings that don't come from `sysinfo`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlatformSensorReadings {
    pub power: PowerSnapshot,
    /// (zone name, °C) from ACPI thermal zones whose reading is live.
    pub thermal_zones: Vec<(String, f32)>,
    /// From PawnIO or HWiNFO, with `cores` already spread over
    /// logical CPUs.
    pub cpu: Option<CpuTemperatures>,
}

impl PlatformSensorReadings {
    pub fn has_power(&self) -> bool {
        let p = &self.power;
        [
            p.cpu_watts,
            p.gpu_watts,
            p.npu_watts,
            p.dram_watts,
            p.package_watts,
        ]
        .iter()
        .any(Option::is_some)
    }

    /// Hottest thermal zone, used when no CPU-specific sensor is readable.
    pub fn hottest_zone(&self) -> Option<f32> {
        self.thermal_zones
            .iter()
            .map(|(_, celsius)| *celsius)
            .reduce(f32::max)
    }

    /// (package, per-logical-CPU) temperatures: sensor-tool readings first,
    /// then the hottest live ACPI zone as a package-only fallback.
    pub fn cpu_temperatures(&self) -> (Option<f32>, Vec<f32>) {
        match &self.cpu {
            Some(cpu) => (cpu.package, cpu.cores.clone()),
            None => (self.hottest_zone(), Vec::new()),
        }
    }
}

#[cfg(windows)]
pub use imp::{PlatformSensors, WindowsGpuProvider, WindowsNpuProvider};

#[cfg(not(windows))]
/// No extra sensor sources outside Windows; `sysinfo` covers the rest.
pub struct PlatformSensors;

#[cfg(not(windows))]
impl PlatformSensors {
    pub fn new() -> Self {
        Self
    }

    pub fn sample(&mut self) -> PlatformSensorReadings {
        PlatformSensorReadings::default()
    }
}

#[cfg(not(windows))]
impl Default for PlatformSensors {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
mod imp {
    use std::collections::{BTreeMap, HashSet};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::{null, null_mut};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use anyhow::anyhow;
    use windows_sys::Wdk::Graphics::Direct3D::{
        D3DKMT_ADAPTER_PERFDATA, D3DKMT_ADAPTERREGISTRYINFO, D3DKMT_ADAPTERTYPE,
        D3DKMT_CLOSEADAPTER, D3DKMT_DRIVER_DESCRIPTION, D3DKMT_NODE_PERFDATA,
        D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO, D3DKMT_SEGMENTSIZEINFO,
        D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo,
        KMTQAITYPE_ADAPTERPERFDATA, KMTQAITYPE_ADAPTERREGISTRYINFO, KMTQAITYPE_ADAPTERTYPE,
        KMTQAITYPE_DRIVER_DESCRIPTION, KMTQAITYPE_GETSEGMENTSIZE, KMTQAITYPE_NODEPERFDATA,
        KMTQUERYADAPTERINFOTYPE,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, LUID};
    use windows_sys::Win32::System::Memory::{
        FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
        VirtualQuery,
    };
    use windows_sys::Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
        PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
        PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows_sys::Win32::System::SystemInformation::{
        GROUP_AFFINITY, GetLogicalProcessorInformationEx, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    use super::{
        AdapterLuid, CpuTemperatures, HwinfoReading, PlatformSensorReadings, ThermalZoneTracker,
        WindowsAdapterClass, classify_windows_adapter, core_temps_per_logical_cpu,
        deci_celsius_to_celsius, gpu_utilization_by_adapter, hwinfo_cpu_temperatures,
        parse_hwinfo_shared_memory, rapl_power_from_samples, select_sensor_cpu_temps,
        sum_by_adapter, thermal_zones_celsius, windows_gpu_memory,
    };
    use crate::model::{GpuSnapshot, NpuSnapshot};
    use crate::npu::NpuProvider;
    use crate::platform::GpuProvider;
    use crate::windows_pawnio::IntelDts;

    /// Don't cap rate counters at 100 (multi-engine sums are clamped later).
    const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
    /// GPU and NPU providers share one sample per collector tick.
    const ADAPTER_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

    struct PdhQuery(PDH_HQUERY);

    // SAFETY: PDH query handles are not tied to the creating thread; access is
    // serialized through `&mut self` / a mutex.
    unsafe impl Send for PdhQuery {}

    struct PdhCounter(PDH_HCOUNTER);

    // SAFETY: counter handles belong to their query; see `PdhQuery`.
    unsafe impl Send for PdhCounter {}

    impl PdhQuery {
        fn open() -> Option<Self> {
            let mut handle = null_mut();
            // SAFETY: valid out-pointer; a null data source means live data.
            let status = unsafe { PdhOpenQueryW(null(), 0, &mut handle) };
            (status == 0).then_some(Self(handle))
        }

        fn add(&self, path: &str) -> Option<PdhCounter> {
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut counter = null_mut();
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            let status = unsafe { PdhAddEnglishCounterW(self.0, wide.as_ptr(), 0, &mut counter) };
            (status == 0).then_some(PdhCounter(counter))
        }

        fn collect(&self) -> bool {
            // SAFETY: the query handle is open until drop.
            unsafe { PdhCollectQueryData(self.0) == 0 }
        }
    }

    impl Drop for PdhQuery {
        fn drop(&mut self) {
            // SAFETY: closes the query and every counter added to it.
            unsafe { PdhCloseQuery(self.0) };
        }
    }

    impl PdhCounter {
        /// Valid (instance, value) pairs from the last collection. Instances
        /// without a rate baseline yet (new processes) are skipped.
        fn values(&self) -> Vec<(String, f64)> {
            let format = PDH_FMT_DOUBLE | PDH_FMT_NOCAP100;
            let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let mut buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::new();
            for _ in 0..3 {
                let mut bytes = (buffer.capacity() * item_size) as u32;
                let mut count = 0u32;
                let items = if buffer.capacity() == 0 {
                    null_mut()
                } else {
                    buffer.as_mut_ptr()
                };
                // SAFETY: `items` is null or points to `bytes` writable bytes.
                let status = unsafe {
                    PdhGetFormattedCounterArrayW(self.0, format, &mut bytes, &mut count, items)
                };
                if status == PDH_MORE_DATA {
                    buffer = Vec::with_capacity((bytes as usize).div_ceil(item_size) + 1);
                    continue;
                }
                if status != 0 || items.is_null() {
                    return Vec::new();
                }
                // SAFETY: PDH wrote `count` items (and their name strings) into
                // `buffer`, which stays alive while we read them.
                let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
                return items
                    .iter()
                    .filter(|item| {
                        matches!(
                            item.FmtValue.CStatus,
                            PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
                        )
                    })
                    .map(|item| {
                        // SAFETY: PDH_FMT_DOUBLE selects the double member; the
                        // name is a NUL-terminated string inside `buffer`.
                        let value = unsafe { item.FmtValue.Anonymous.doubleValue };
                        (unsafe { wide_ptr_to_string(item.szName) }, value)
                    })
                    .collect();
            }
            Vec::new()
        }
    }

    /// # Safety
    /// `ptr` must be null or point to a NUL-terminated UTF-16 string.
    unsafe fn wide_ptr_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0;
        // SAFETY: guaranteed NUL-terminated by the caller.
        while unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` elements were just read.
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
    }

    fn wide_array_to_string(wide: &[u16]) -> String {
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..end]).trim().to_owned()
    }

    /// An open kernel-mode thunk adapter handle.
    struct KmtAdapter(u32);

    impl KmtAdapter {
        fn open(luid: AdapterLuid) -> Option<Self> {
            let mut open = D3DKMT_OPENADAPTERFROMLUID {
                AdapterLuid: LUID {
                    LowPart: luid.low,
                    HighPart: luid.high as i32,
                },
                hAdapter: 0,
            };
            // SAFETY: valid in/out struct.
            let status = unsafe { D3DKMTOpenAdapterFromLuid(&mut open) };
            (status >= 0).then_some(Self(open.hAdapter))
        }

        fn query<T>(&self, kind: KMTQUERYADAPTERINFOTYPE, data: &mut T) -> bool {
            let mut query = D3DKMT_QUERYADAPTERINFO {
                hAdapter: self.0,
                Type: kind,
                pPrivateDriverData: data as *mut T as *mut c_void,
                PrivateDriverDataSize: size_of::<T>() as u32,
            };
            // SAFETY: `data` is a writable buffer of the size the query expects.
            unsafe { D3DKMTQueryAdapterInfo(&mut query) >= 0 }
        }

        fn type_bits(&self) -> Option<u32> {
            let mut value = D3DKMT_ADAPTERTYPE::default();
            let ok = self.query(KMTQAITYPE_ADAPTERTYPE, &mut value);
            // SAFETY: every bit pattern is a valid `u32` view of the bitfield.
            ok.then_some(unsafe { value.Anonymous.Value })
        }

        fn name(&self) -> Option<String> {
            // SAFETY: plain-old-data structs; all-zero is a valid value.
            let mut registry: Box<D3DKMT_ADAPTERREGISTRYINFO> =
                Box::new(unsafe { std::mem::zeroed() });
            if self.query(KMTQAITYPE_ADAPTERREGISTRYINFO, registry.as_mut()) {
                let name = wide_array_to_string(&registry.AdapterString);
                if !name.is_empty() {
                    return Some(name);
                }
            }
            let mut description: Box<D3DKMT_DRIVER_DESCRIPTION> =
                Box::new(unsafe { std::mem::zeroed() });
            if self.query(KMTQAITYPE_DRIVER_DESCRIPTION, description.as_mut()) {
                let name = wide_array_to_string(&description.DriverDescription);
                if !name.is_empty() {
                    return Some(name);
                }
            }
            None
        }

        fn segment_sizes(&self) -> Option<D3DKMT_SEGMENTSIZEINFO> {
            let mut sizes = D3DKMT_SEGMENTSIZEINFO::default();
            self.query(KMTQAITYPE_GETSEGMENTSIZE, &mut sizes)
                .then_some(sizes)
        }

        fn temperature_celsius(&self) -> Option<u32> {
            let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
            if !self.query(KMTQAITYPE_ADAPTERPERFDATA, &mut perf) {
                return None;
            }
            deci_celsius_to_celsius(perf.Temperature)
        }

        fn node_frequency_mhz(&self, node: u32) -> Option<u64> {
            let mut perf = D3DKMT_NODE_PERFDATA {
                NodeOrdinal: node,
                ..Default::default()
            };
            if !self.query(KMTQAITYPE_NODEPERFDATA, &mut perf) {
                return None;
            }
            let mhz = perf.Frequency / 1_000_000;
            (mhz > 0).then_some(mhz)
        }
    }

    impl Drop for KmtAdapter {
        fn drop(&mut self) {
            let close = D3DKMT_CLOSEADAPTER { hAdapter: self.0 };
            // SAFETY: closes the handle opened in `open`.
            unsafe { D3DKMTCloseAdapter(&close) };
        }
    }

    // SAFETY: a D3DKMT adapter handle is a process-wide kernel handle.
    unsafe impl Send for KmtAdapter {}

    struct AdapterInfo {
        kmt: Option<KmtAdapter>,
        name: String,
        class: WindowsAdapterClass,
        dedicated_size: u64,
        shared_size: u64,
    }

    impl AdapterInfo {
        fn open(luid: AdapterLuid) -> Self {
            let kmt = KmtAdapter::open(luid);
            let name = kmt
                .as_ref()
                .and_then(KmtAdapter::name)
                .unwrap_or_else(|| format!("Adapter {luid}"));
            let sizes = kmt.as_ref().and_then(KmtAdapter::segment_sizes);
            let dedicated_size = sizes.map_or(0, |s| s.DedicatedVideoMemorySize);
            let shared_size = sizes.map_or(0, |s| s.SharedSystemMemorySize);
            let class = classify_windows_adapter(
                kmt.as_ref().and_then(KmtAdapter::type_bits),
                dedicated_size,
                &name,
            );
            Self {
                kmt,
                name,
                class,
                dedicated_size,
                shared_size,
            }
        }
    }

    struct AdapterCounters {
        query: PdhQuery,
        engine: Option<PdhCounter>,
        dedicated: Option<PdhCounter>,
        shared: Option<PdhCounter>,
    }

    /// Samples every WDDM adapter once and splits the result into GPUs and NPUs.
    struct AdapterSampler {
        counters: Option<AdapterCounters>,
        adapters: BTreeMap<AdapterLuid, AdapterInfo>,
        last_sample_at: Option<Instant>,
        gpus: Vec<GpuSnapshot>,
        npus: Vec<NpuSnapshot>,
    }

    impl AdapterSampler {
        fn new() -> Self {
            let counters = PdhQuery::open().map(|query| {
                let engine = query.add(r"\GPU Engine(*)\Utilization Percentage");
                let dedicated = query.add(r"\GPU Adapter Memory(*)\Dedicated Usage");
                let shared = query.add(r"\GPU Adapter Memory(*)\Shared Usage");
                // Prime rate counters so the next collection has a baseline.
                query.collect();
                AdapterCounters {
                    query,
                    engine,
                    dedicated,
                    shared,
                }
            });
            Self {
                counters,
                adapters: BTreeMap::new(),
                last_sample_at: None,
                gpus: Vec::new(),
                npus: Vec::new(),
            }
        }

        fn available(&self) -> bool {
            self.counters
                .as_ref()
                .is_some_and(|c| c.engine.is_some() || c.dedicated.is_some())
        }

        fn sample(&mut self) {
            let now = Instant::now();
            if self
                .last_sample_at
                .is_some_and(|last| now.duration_since(last) < ADAPTER_SAMPLE_INTERVAL)
            {
                return;
            }
            self.last_sample_at = Some(now);
            let Some(counters) = &self.counters else {
                return;
            };
            if !counters.query.collect() {
                return;
            }
            let values = |counter: &Option<PdhCounter>| {
                counter.as_ref().map(PdhCounter::values).unwrap_or_default()
            };
            let engine_samples = values(&counters.engine);
            let utilization = gpu_utilization_by_adapter(&engine_samples);
            let dedicated = sum_by_adapter(&values(&counters.dedicated));
            let shared = sum_by_adapter(&values(&counters.shared));

            let mut present: HashSet<AdapterLuid> = HashSet::new();
            present.extend(dedicated.keys().copied());
            present.extend(shared.keys().copied());
            present.extend(utilization.keys().copied());
            present.extend(
                engine_samples
                    .iter()
                    .filter_map(|(name, _)| super::parse_gpu_engine_instance(name))
                    .map(|instance| instance.luid),
            );
            // Forget removed adapters (driver restart, eGPU unplug).
            self.adapters.retain(|luid, _| present.contains(luid));

            let mut gpus = Vec::new();
            let mut npus = Vec::new();
            for luid in present {
                let info = self
                    .adapters
                    .entry(luid)
                    .or_insert_with(|| AdapterInfo::open(luid));
                match info.class {
                    WindowsAdapterClass::Ignored => {}
                    WindowsAdapterClass::Gpu => {
                        let (memory_used_bytes, memory_total_bytes) = windows_gpu_memory(
                            dedicated.get(&luid).copied(),
                            shared.get(&luid).copied(),
                            info.dedicated_size,
                            info.shared_size,
                        );
                        gpus.push((
                            info.dedicated_size,
                            luid,
                            GpuSnapshot {
                                name: info.name.clone(),
                                utilization_percent: utilization.get(&luid).copied(),
                                memory_used_bytes,
                                memory_total_bytes,
                                temperature_celsius: info
                                    .kmt
                                    .as_ref()
                                    .and_then(KmtAdapter::temperature_celsius),
                                ..GpuSnapshot::default()
                            },
                        ));
                    }
                    WindowsAdapterClass::Npu => npus.push((
                        luid,
                        NpuSnapshot {
                            name: info.name.clone(),
                            utilization_percent: utilization.get(&luid).copied(),
                            power_watts: None,
                            frequency_mhz: info
                                .kmt
                                .as_ref()
                                .and_then(|kmt| kmt.node_frequency_mhz(0)),
                        },
                    )),
                }
            }
            // Discrete (largest VRAM) first; the overview shows the first GPU.
            gpus.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            npus.sort_by_key(|(luid, _)| *luid);
            self.gpus = gpus.into_iter().map(|(_, _, gpu)| gpu).collect();
            self.npus = npus.into_iter().map(|(_, npu)| npu).collect();
        }
    }

    fn shared_sampler() -> Arc<Mutex<AdapterSampler>> {
        static SAMPLER: OnceLock<Arc<Mutex<AdapterSampler>>> = OnceLock::new();
        SAMPLER
            .get_or_init(|| Arc::new(Mutex::new(AdapterSampler::new())))
            .clone()
    }

    /// GPUs from WDDM performance counters and D3DKMT adapter queries.
    pub struct WindowsGpuProvider {
        sampler: Arc<Mutex<AdapterSampler>>,
    }

    impl WindowsGpuProvider {
        pub fn new() -> Self {
            Self {
                sampler: shared_sampler(),
            }
        }
    }

    impl Default for WindowsGpuProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl GpuProvider for WindowsGpuProvider {
        fn collect(&mut self) -> anyhow::Result<Vec<GpuSnapshot>> {
            let mut sampler = self
                .sampler
                .lock()
                .map_err(|_| anyhow!("GPU sampler poisoned"))?;
            if !sampler.available() {
                return Err(anyhow!("GPU performance counters unavailable"));
            }
            sampler.sample();
            Ok(sampler.gpus.clone())
        }
    }

    /// Compute-only (MCDM) adapters such as Intel AI Boost and AMD/Qualcomm NPUs.
    pub struct WindowsNpuProvider {
        sampler: Arc<Mutex<AdapterSampler>>,
    }

    impl WindowsNpuProvider {
        pub fn new() -> Self {
            Self {
                sampler: shared_sampler(),
            }
        }
    }

    impl Default for WindowsNpuProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl NpuProvider for WindowsNpuProvider {
        fn collect(&mut self) -> Vec<NpuSnapshot> {
            let Ok(mut sampler) = self.sampler.lock() else {
                return Vec::new();
            };
            sampler.sample();
            sampler.npus.clone()
        }
    }

    /// How often to look for HWiNFO while absent.
    const SENSOR_TOOL_PROBE_INTERVAL: Duration = Duration::from_secs(10);

    /// RAPL power rails, ACPI thermal zones, and CPU temperatures (Windows
    /// has no user-mode API for per-core temperatures; reading the DTS MSRs
    /// needs a kernel driver, so they come from PawnIO or HWiNFO).
    pub struct PlatformSensors {
        query: Option<PdhQuery>,
        power: Option<PdhCounter>,
        energy: Option<PdhCounter>,
        thermal: Option<(PdhCounter, bool)>,
        zones: ThermalZoneTracker,
        cpu: CpuSensorTools,
    }

    impl PlatformSensors {
        pub fn new() -> Self {
            let cpu = CpuSensorTools::new();
            let zones = ThermalZoneTracker::new();
            let Some(query) = PdhQuery::open() else {
                return Self {
                    query: None,
                    power: None,
                    energy: None,
                    thermal: None,
                    zones,
                    cpu,
                };
            };
            let power = query.add(r"\Energy Meter(*)\Power");
            let energy = query.add(r"\Energy Meter(*)\Energy");
            let thermal = query
                .add(r"\Thermal Zone Information(*)\High Precision Temperature")
                .map(|counter| (counter, true))
                .or_else(|| {
                    query
                        .add(r"\Thermal Zone Information(*)\Temperature")
                        .map(|counter| (counter, false))
                });
            query.collect();
            Self {
                query: Some(query),
                power,
                energy,
                thermal,
                zones,
                cpu,
            }
        }

        pub fn sample(&mut self) -> PlatformSensorReadings {
            let cpu = self.cpu.sample();
            if !self.query.as_ref().is_some_and(PdhQuery::collect) {
                return PlatformSensorReadings {
                    cpu,
                    ..Default::default()
                };
            }
            let power = match &self.power {
                Some(power) => rapl_power_from_samples(
                    &power.values(),
                    &self
                        .energy
                        .as_ref()
                        .map(PdhCounter::values)
                        .unwrap_or_default(),
                ),
                None => Default::default(),
            };
            let thermal_zones = self
                .thermal
                .as_ref()
                .map(|(counter, tenths)| thermal_zones_celsius(&counter.values(), *tenths))
                .unwrap_or_default();
            PlatformSensorReadings {
                power,
                thermal_zones: self.zones.observe(thermal_zones),
                cpu,
            }
        }
    }

    /// CPU temperatures: PawnIO (built in, needs Administrator), then HWiNFO.
    struct CpuSensorTools {
        /// Physical core index of each logical CPU.
        logical_to_core: Vec<usize>,
        intel_dts: Option<IntelDts>,
        next_hwinfo_probe: Instant,
    }

    impl CpuSensorTools {
        fn new() -> Self {
            let cores = processor_cores();
            Self {
                logical_to_core: logical_cpu_cores(&cores),
                intel_dts: IntelDts::open(&cores),
                next_hwinfo_probe: Instant::now(),
            }
        }

        fn sample(&mut self) -> Option<CpuTemperatures> {
            let mut temps = self
                .intel_dts
                .as_ref()
                .and_then(IntelDts::sample)
                .or_else(|| self.hwinfo())?;
            temps.cores = core_temps_per_logical_cpu(&temps.cores, &self.logical_to_core);
            Some(temps)
        }

        fn hwinfo(&mut self) -> Option<CpuTemperatures> {
            if Instant::now() < self.next_hwinfo_probe {
                return None;
            }
            let temps = read_hwinfo_shared_memory()
                .and_then(|readings| select_sensor_cpu_temps(&hwinfo_cpu_temperatures(&readings)));
            if temps.is_none() {
                self.next_hwinfo_probe = Instant::now() + SENSOR_TOOL_PROBE_INTERVAL;
            }
            temps
        }
    }

    /// Affinity of each physical core, in OS core order.
    fn processor_cores() -> Vec<GROUP_AFFINITY> {
        let mut len = 0u32;
        // SAFETY: a null buffer queries the required length.
        unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, null_mut(), &mut len) };
        if len == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u8; len as usize];
        // SAFETY: `buffer` holds `len` writable bytes.
        let ok = unsafe {
            GetLogicalProcessorInformationEx(
                RelationProcessorCore,
                buffer
                    .as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
                &mut len,
            )
        };
        if ok == 0 {
            return Vec::new();
        }
        let mut cores = Vec::new();
        let mut offset = 0usize;
        while offset + size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() <= len as usize {
            // SAFETY: the record lies within the `len` bytes Windows filled;
            // records are variable-sized, so read unaligned.
            let info = unsafe {
                buffer
                    .as_ptr()
                    .add(offset)
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
                    .read_unaligned()
            };
            if info.Size == 0 {
                break;
            }
            // SAFETY: RelationProcessorCore records carry `Processor`, whose
            // GroupCount is always 1 for a core.
            cores.push(unsafe { info.Anonymous.Processor.GroupMask[0] });
            offset += info.Size as usize;
        }
        cores
    }

    /// Physical core index for each logical CPU, in logical-CPU order.
    fn logical_cpu_cores(cores: &[GROUP_AFFINITY]) -> Vec<usize> {
        let mut logical: Vec<(usize, usize)> = Vec::new();
        for (core, affinity) in cores.iter().enumerate() {
            for bit in 0..usize::BITS as usize {
                if affinity.Mask & (1usize << bit) != 0 {
                    logical.push((affinity.Group as usize * 64 + bit, core));
                }
            }
        }
        logical.sort_unstable();
        logical.into_iter().map(|(_, core)| core).collect()
    }

    /// Copy HWiNFO's sensor shared memory (needs "Shared Memory Support"
    /// enabled in HWiNFO's settings) and parse it.
    fn read_hwinfo_shared_memory() -> Option<Vec<HwinfoReading>> {
        let name: Vec<u16> = r"Global\HWiNFO_SENS_SM2"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: `name` is NUL-terminated.
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
        if mapping.is_null() {
            return None;
        }
        // SAFETY: `mapping` is a valid section handle; map it all read-only.
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0) };
        let bytes = if view.Value.is_null() {
            None
        } else {
            // SAFETY: all-zero is a valid MEMORY_BASIC_INFORMATION.
            let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: `view` is a live mapping; `info` is a valid out-buffer.
            let queried = unsafe {
                VirtualQuery(view.Value, &mut info, size_of::<MEMORY_BASIC_INFORMATION>())
            };
            // SAFETY: the region VirtualQuery reported is mapped readable.
            let bytes = (queried != 0).then(|| unsafe {
                std::slice::from_raw_parts(view.Value.cast::<u8>(), info.RegionSize).to_vec()
            });
            // SAFETY: unmaps the view created above.
            unsafe { UnmapViewOfFile(view) };
            bytes
        };
        // SAFETY: closes the handle opened above.
        unsafe { CloseHandle(mapping) };
        parse_hwinfo_shared_memory(&bytes?)
    }

    impl Default for PlatformSensors {
        fn default() -> Self {
            Self::new()
        }
    }
}

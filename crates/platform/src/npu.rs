use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use mtop_core::model::NpuSnapshot;

const IVPU_SYSFS_ROOT: &str = "/sys/bus/pci/drivers/intel_vpu";
const IVPU_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// Derive busy percentage from the IVPU driver's cumulative busy-time counter.
/// The kernel ABI doesn't specify wrap/reset semantics, so a counter decrease is
/// treated as a discontinuity rather than wrapping subtraction.
fn derive_npu_utilization(
    previous_busy_us: Option<u64>,
    current_busy_us: u64,
    elapsed: Duration,
) -> Option<f32> {
    let previous_busy_us = previous_busy_us?;
    let elapsed_us = elapsed.as_micros();
    if elapsed_us == 0 || current_busy_us < previous_busy_us {
        return None;
    }

    let busy_delta = current_busy_us - previous_busy_us;
    let percent = (busy_delta as f64 / elapsed_us as f64 * 100.0).clamp(0.0, 100.0);
    percent.is_finite().then_some(percent as f32)
}

fn parse_sysfs_counter(contents: &str) -> Option<u64> {
    contents.trim().parse().ok()
}

/// Source of platform NPU snapshots. Unsupported platforms return no devices.
pub trait NpuProvider: Send {
    fn collect(&mut self) -> Vec<NpuSnapshot>;
}

#[derive(Default)]
pub struct NoopNpuProvider;

impl NpuProvider for NoopNpuProvider {
    fn collect(&mut self) -> Vec<NpuSnapshot> {
        Vec::new()
    }
}

/// Intel IVPU telemetry from the documented Linux sysfs ABI.
/// Utilization is sampled no faster than once per second; power is deliberately
/// left unavailable because the standard IVPU ABI doesn't expose a power rail.
pub struct LinuxIvpuProvider {
    sysfs_root: PathBuf,
    previous: HashMap<PathBuf, (u64, Instant)>,
    last_sample_at: Option<Instant>,
    last_snapshots: Vec<NpuSnapshot>,
}

impl Default for LinuxIvpuProvider {
    fn default() -> Self {
        Self::with_sysfs_root(IVPU_SYSFS_ROOT)
    }
}

impl LinuxIvpuProvider {
    fn with_sysfs_root(root: impl Into<PathBuf>) -> Self {
        Self {
            sysfs_root: root.into(),
            previous: HashMap::new(),
            last_sample_at: None,
            last_snapshots: Vec::new(),
        }
    }

    fn collect_at(&mut self, now: Instant) -> Vec<NpuSnapshot> {
        if self
            .last_sample_at
            .is_some_and(|last| now.saturating_duration_since(last) < IVPU_SAMPLE_INTERVAL)
        {
            return self.last_snapshots.clone();
        }

        let mut seen = HashSet::new();
        let mut snapshots = Vec::new();
        if let Ok(devices) = fs::read_dir(&self.sysfs_root) {
            for entry in devices.flatten() {
                let device_path = entry.path();
                let counter_path = device_path.join("npu_busy_time_us");
                let Ok(contents) = fs::read_to_string(&counter_path) else {
                    self.previous.remove(&device_path);
                    continue;
                };
                let Some(busy_us) = parse_sysfs_counter(&contents) else {
                    self.previous.remove(&device_path);
                    continue;
                };
                seen.insert(device_path.clone());

                let utilization_percent =
                    self.previous
                        .get(&device_path)
                        .and_then(|(previous_busy_us, previous_at)| {
                            derive_npu_utilization(
                                Some(*previous_busy_us),
                                busy_us,
                                now.saturating_duration_since(*previous_at),
                            )
                        });
                self.previous.insert(device_path.clone(), (busy_us, now));

                let frequency_mhz = fs::read_to_string(device_path.join("freq/current_freq"))
                    .ok()
                    .and_then(|value| parse_sysfs_counter(&value))
                    .filter(|frequency| *frequency > 0);
                let device_name = device_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown".to_owned());
                snapshots.push(NpuSnapshot {
                    name: format!("Intel VPU {device_name}"),
                    utilization_percent,
                    power_watts: None,
                    frequency_mhz,
                });
            }
        }
        self.previous
            .retain(|device_path, _| seen.contains(device_path));
        self.last_sample_at = Some(now);
        self.last_snapshots.clone_from(&snapshots);
        snapshots
    }
}

impl NpuProvider for LinuxIvpuProvider {
    fn collect(&mut self) -> Vec<NpuSnapshot> {
        self.collect_at(Instant::now())
    }
}

/// Create the best-effort NPU provider for this target.
pub fn npu_provider_for() -> Box<dyn NpuProvider> {
    #[cfg(target_os = "linux")]
    {
        return Box::new(LinuxIvpuProvider::default());
    }

    #[cfg(windows)]
    {
        return Box::new(crate::WindowsNpuProvider::new());
    }

    #[allow(unreachable_code)]
    Box::new(NoopNpuProvider)
}

#[cfg(test)]
mod tests {
    use super::{derive_npu_utilization, parse_sysfs_counter};
    use std::time::Duration;

    #[test]
    fn first_sample_has_no_utilization_baseline() {
        assert_eq!(
            derive_npu_utilization(None, 500_000, Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn derives_busy_ratio_from_monotonic_counter_delta() {
        assert_eq!(
            derive_npu_utilization(Some(10_000), 510_000, Duration::from_secs(1)),
            Some(50.0)
        );
    }

    #[test]
    fn reports_idle_as_zero_and_caps_impossible_busy_time() {
        assert_eq!(
            derive_npu_utilization(Some(10), 10, Duration::from_secs(1)),
            Some(0.0)
        );
        assert_eq!(
            derive_npu_utilization(Some(0), 2_000_000, Duration::from_secs(1)),
            Some(100.0)
        );
    }

    #[test]
    fn counter_reset_and_zero_elapsed_are_unavailable() {
        assert_eq!(
            derive_npu_utilization(Some(100), 99, Duration::from_secs(1)),
            None
        );
        assert_eq!(derive_npu_utilization(Some(1), 2, Duration::ZERO), None);
    }

    #[test]
    fn malformed_sysfs_counter_is_unavailable() {
        assert_eq!(parse_sysfs_counter("not-a-number\n"), None);
        assert_eq!(parse_sysfs_counter(" 12345\n"), Some(12_345));
    }

    #[test]
    fn linux_provider_reads_ivpu_busy_time_frequency_and_throttles() {
        use super::LinuxIvpuProvider;
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicU64, Ordering},
            time::Instant,
        };

        struct TempRoot(PathBuf);

        impl TempRoot {
            fn new() -> Self {
                static NEXT_ID: AtomicU64 = AtomicU64::new(0);
                let path = std::env::temp_dir().join(format!(
                    "mtop-ivpu-test-{}-{}",
                    std::process::id(),
                    NEXT_ID.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&path).unwrap();
                Self(path)
            }
        }

        impl Drop for TempRoot {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let root = TempRoot::new();
        let device = root.0.join("0000:00:0a.0");
        fs::create_dir_all(device.join("freq")).unwrap();
        fs::write(device.join("npu_busy_time_us"), "10000\n").unwrap();
        fs::write(device.join("freq/current_freq"), "1500\n").unwrap();

        let mut provider = LinuxIvpuProvider::with_sysfs_root(&root.0);
        let start = Instant::now();
        let first = provider.collect_at(start);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].utilization_percent, None);
        assert_eq!(first[0].power_watts, None);
        assert_eq!(first[0].frequency_mhz, Some(1500));

        fs::write(device.join("npu_busy_time_us"), "260000\n").unwrap();
        let throttled = provider.collect_at(start + Duration::from_millis(500));
        assert_eq!(throttled[0].utilization_percent, None);

        fs::write(device.join("npu_busy_time_us"), "510000\n").unwrap();
        let second = provider.collect_at(start + Duration::from_secs(1));
        assert_eq!(second[0].utilization_percent, Some(50.0));
        assert_eq!(second[0].power_watts, None);
    }
}

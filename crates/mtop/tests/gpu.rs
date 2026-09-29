use anyhow::anyhow;

use mtop::config::Config;
use mtop::platform::{
    GpuProvider, NoopGpuProvider, SnapshotProvider, SysinfoCollector, gpu_provider_for,
};

#[test]
fn noop_gpu_provider_returns_no_snapshots() {
    let mut provider = NoopGpuProvider;

    assert!(provider.collect().unwrap().is_empty());
}

struct FailingGpuProvider;

impl GpuProvider for FailingGpuProvider {
    fn collect(&mut self) -> anyhow::Result<Vec<mtop::model::GpuSnapshot>> {
        Err(anyhow!("test GPU failure"))
    }
}

struct WarningGpuProvider;

impl GpuProvider for WarningGpuProvider {
    fn collect(&mut self) -> anyhow::Result<Vec<mtop::model::GpuSnapshot>> {
        Ok(vec![mtop::model::GpuSnapshot {
            name: "Test GPU".to_owned(),
            utilization_percent: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: None,
            ..mtop::model::GpuSnapshot::default()
        }])
    }

    fn take_warnings(&mut self) -> Vec<String> {
        vec!["GPU device 1 unavailable: test warning".to_owned()]
    }
}

#[test]
fn provider_error_becomes_snapshot_warning_and_empty_gpu_list() {
    let mut collector =
        SysinfoCollector::new_with_gpu_provider(&Config::default(), Box::new(FailingGpuProvider));

    let snapshot = collector.collect();

    assert!(snapshot.gpus.is_empty());
    assert!(
        snapshot
            .warnings
            .iter()
            .any(|warning| warning == "GPU unavailable: test GPU failure")
    );
}

#[test]
fn provider_warnings_are_added_to_snapshot_without_discarding_gpus() {
    let mut collector =
        SysinfoCollector::new_with_gpu_provider(&Config::default(), Box::new(WarningGpuProvider));

    let snapshot = collector.collect();

    assert_eq!(snapshot.gpus.len(), 1);
    assert_eq!(snapshot.gpus[0].name, "Test GPU");
    assert!(
        snapshot
            .warnings
            .iter()
            .any(|warning| warning == "GPU device 1 unavailable: test warning")
    );
}

#[test]
fn factory_respects_disabled_gpu_configuration() {
    let config = Config {
        show_gpu: false,
        ..Config::default()
    };
    let mut provider = gpu_provider_for(&config);

    assert!(provider.collect().unwrap().is_empty());
}

#[cfg(feature = "gpu-nvidia")]
#[test]
fn nvidia_provider_is_hardware_independent_at_construction_and_collection() {
    let mut provider = gpu_provider_for(&Config::default());

    match provider.collect() {
        Ok(gpus) => {
            for gpu in gpus {
                assert!(!gpu.name.is_empty());
                if let Some(utilization) = gpu.utilization_percent {
                    assert!(utilization.is_finite());
                    assert!((0.0..=100.0).contains(&utilization));
                }
                if let (Some(used), Some(total)) = (gpu.memory_used_bytes, gpu.memory_total_bytes) {
                    assert!(used <= total);
                }
                if let Some(temperature) = gpu.temperature_celsius {
                    assert!(temperature <= 200);
                }
            }
        }
        Err(error) => assert!(!error.to_string().is_empty()),
    }
}

const IOREG_SAMPLE: &str = r#"+-o AGXAcceleratorG17G  <class AGXAcceleratorG17G, id 0x1000005f1, registered>
    {
      "PerformanceStatistics" = {"In use system memory (driver)"=0,"Alloc system memory"=7340146688,"Tiler Utilization %"=3,"Renderer Utilization %"=60,"Device Utilization %"=46,"In use system memory"=843038720}
      "model" = "Apple M5"
      "gpu-core-count" = 10
    }
"#;

#[test]
fn ioreg_accelerator_output_parses_into_gpu_snapshot() {
    use mtop::platform::parse_ioreg_gpus;
    let gpus = parse_ioreg_gpus(IOREG_SAMPLE);
    assert_eq!(gpus.len(), 1);
    let g = &gpus[0];
    assert_eq!(g.name, "Apple M5 (10-core)");
    assert_eq!(g.utilization_percent, Some(46.0));
    // "In use system memory" is the GPU's live working set on unified memory;
    // the driver-only counter must not win just because it appears first.
    assert_eq!(g.memory_used_bytes, Some(843_038_720));
    assert_eq!(g.temperature_celsius, None);
}

#[test]
fn ioreg_parser_tolerates_missing_or_garbage_fields() {
    use mtop::platform::parse_ioreg_gpus;
    assert!(parse_ioreg_gpus("").is_empty());
    let gpus = parse_ioreg_gpus("+-o Weird  <class X>\n  \"model\" = \"Mystery GPU\"\n");
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].name, "Mystery GPU");
    assert_eq!(gpus[0].utilization_percent, None);
}

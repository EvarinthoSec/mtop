//! Windows GPU/NPU, power and thermal counter parsing (portable helpers).
use mtop::platform::{
    AdapterLuid, CpuTemperatures, HWINFO_READING_TEMP, PlatformSensorReadings, ThermalZoneTracker,
    WindowsAdapterClass, adapter_type_bits::*, classify_windows_adapter,
    core_temps_per_logical_cpu, deci_celsius_to_celsius, gpu_utilization_by_adapter,
    hwinfo_cpu_temperatures, intel_dts_celsius, parse_gpu_adapter_instance,
    parse_gpu_engine_instance, parse_hwinfo_shared_memory, rapl_power_from_samples,
    select_sensor_cpu_temps, sum_by_adapter, thermal_zones_celsius, windows_gpu_memory,
};

const NVIDIA: AdapterLuid = AdapterLuid {
    high: 0,
    low: 0x0001_0960,
};
const INTEL: AdapterLuid = AdapterLuid {
    high: 0,
    low: 0x0001_41EB,
};

fn samples(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
    pairs.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect()
}

#[test]
fn parses_gpu_engine_instance_names() {
    let engine = parse_gpu_engine_instance(
        "pid_9484_luid_0x00000000_0x00010960_phys_0_eng_2_engtype_VideoDecode",
    )
    .unwrap();
    assert_eq!(engine.pid, Some(9484));
    assert_eq!(engine.luid, NVIDIA);
    assert_eq!(engine.phys, 0);
    assert_eq!(engine.engine, 2);
    assert_eq!(engine.engine_type, "VideoDecode");

    let unnamed =
        parse_gpu_engine_instance("pid_4_luid_0x00000000_0x000141eb_phys_0_eng_11_engtype_")
            .unwrap();
    assert_eq!(unnamed.luid, INTEL);
    assert_eq!(unnamed.engine_type, "");

    assert_eq!(parse_gpu_engine_instance("_Total"), None);
    assert_eq!(
        parse_gpu_engine_instance("pid_x_luid_0x0_0x1_phys_0_eng_0_engtype_3D"),
        None
    );
}

#[test]
fn parses_adapter_memory_instance_names() {
    assert_eq!(
        parse_gpu_adapter_instance("luid_0x00000000_0x00010960_phys_0"),
        Some((NVIDIA, 0))
    );
    assert_eq!(
        parse_gpu_adapter_instance("luid_0x00000000_0x000141EB_phys_1_part_0"),
        Some((INTEL, 1))
    );
    assert_eq!(parse_gpu_adapter_instance("luid_bogus"), None);
}

#[test]
fn utilization_sums_processes_per_engine_and_reports_busiest_engine() {
    let util = gpu_utilization_by_adapter(&samples(&[
        (
            "pid_1_luid_0x00000000_0x00010960_phys_0_eng_0_engtype_3D",
            30.0,
        ),
        (
            "pid_2_luid_0x00000000_0x00010960_phys_0_eng_0_engtype_3D",
            25.0,
        ),
        (
            "pid_2_luid_0x00000000_0x00010960_phys_0_eng_6_engtype_VideoEncode",
            40.0,
        ),
        (
            "pid_3_luid_0x00000000_0x000141eb_phys_0_eng_0_engtype_3D",
            80.0,
        ),
        (
            "pid_4_luid_0x00000000_0x000141eb_phys_0_eng_0_engtype_3D",
            70.0,
        ),
        ("garbage", 99.0),
    ]));
    assert_eq!(util.get(&NVIDIA), Some(&55.0), "3D engine: 30 + 25");
    assert_eq!(util.get(&INTEL), Some(&100.0), "engine sum is clamped");
    assert_eq!(util.len(), 2);
}

#[test]
fn memory_counters_sum_across_physical_adapters() {
    let totals = sum_by_adapter(&samples(&[
        ("luid_0x00000000_0x00010960_phys_0", 1024.0),
        ("luid_0x00000000_0x00010960_phys_1", 512.0),
        ("luid_0x00000000_0x000141eb_phys_0", 0.0),
    ]));
    assert_eq!(totals.get(&NVIDIA), Some(&1536));
    assert_eq!(totals.get(&INTEL), Some(&0));
}

#[test]
fn classifies_gpus_npus_and_virtual_adapters() {
    let gib = 1 << 30;
    assert_eq!(
        classify_windows_adapter(
            Some(RENDER_SUPPORTED | DISPLAY_SUPPORTED),
            3 * gib,
            "GeForce"
        ),
        WindowsAdapterClass::Gpu
    );
    assert_eq!(
        classify_windows_adapter(Some(COMPUTE_ONLY), 0, "Intel(R) AI Boost"),
        WindowsAdapterClass::Npu
    );
    assert_eq!(
        classify_windows_adapter(
            Some(COMPUTE_ONLY | RENDER_SUPPORTED),
            0,
            "Compute Accelerator"
        ),
        WindowsAdapterClass::Npu,
        "compute-only without VRAM"
    );
    assert_eq!(
        classify_windows_adapter(Some(COMPUTE_ONLY), 80 * gib, "NVIDIA A100 (MCDM)"),
        WindowsAdapterClass::Gpu,
        "datacenter GPU in MCDM mode keeps its VRAM"
    );
    assert_eq!(
        classify_windows_adapter(Some(RENDER_SUPPORTED | SOFTWARE_DEVICE), 0, "Basic Render"),
        WindowsAdapterClass::Ignored
    );
    assert_eq!(
        classify_windows_adapter(
            Some(DISPLAY_SUPPORTED | INDIRECT_DISPLAY_DEVICE),
            0,
            "Parsec"
        ),
        WindowsAdapterClass::Ignored
    );
    assert_eq!(
        classify_windows_adapter(None, 0, "AMD Ryzen AI NPU"),
        WindowsAdapterClass::Npu
    );
    assert_eq!(
        classify_windows_adapter(None, 0, "Unknown Adapter"),
        WindowsAdapterClass::Gpu
    );
}

#[test]
fn gpu_memory_uses_shared_pool_for_integrated_graphics() {
    let mib = 1u64 << 20;
    // Discrete: dedicated VRAM.
    assert_eq!(
        windows_gpu_memory(Some(2000 * mib), Some(50 * mib), 3072 * mib, 8192 * mib),
        (Some(2000 * mib), Some(3072 * mib))
    );
    // Integrated: tiny carve-out plus shared system memory.
    assert_eq!(
        windows_gpu_memory(Some(0), Some(84 * mib), 128 * mib, 8192 * mib),
        (Some(84 * mib), Some(8320 * mib))
    );
    assert_eq!(
        windows_gpu_memory(None, None, 0, 8192 * mib),
        (None, Some(8192 * mib))
    );
    assert_eq!(windows_gpu_memory(Some(5), None, 0, 0), (Some(5), None));
}

#[test]
fn adapter_temperature_is_deci_celsius_with_zero_unsupported() {
    assert_eq!(deci_celsius_to_celsius(0), None);
    assert_eq!(deci_celsius_to_celsius(384), Some(38));
    assert_eq!(deci_celsius_to_celsius(385), Some(39));
    assert_eq!(deci_celsius_to_celsius(40_000), None);
}

#[test]
fn rapl_rails_map_to_power_fields_in_watts() {
    let power = rapl_power_from_samples(
        &samples(&[
            ("RAPL_Package0_PKG", 19_835.8),
            ("RAPL_Package0_PP0", 18_293.2),
            ("RAPL_Package0_PP1", 0.0),
            ("RAPL_Package0_DRAM", 0.0),
            ("_Total", 0.0),
        ]),
        &samples(&[
            ("RAPL_Package0_PKG", 2.2e15),
            ("RAPL_Package0_PP0", 2.0e15),
            ("RAPL_Package0_PP1", 1.7e10),
            ("RAPL_Package0_DRAM", 0.0),
        ]),
    );
    assert!((power.package_watts.unwrap() - 19.8358).abs() < 1e-3);
    assert!((power.cpu_watts.unwrap() - 18.2932).abs() < 1e-3);
    assert_eq!(
        power.gpu_watts,
        Some(0.0),
        "idle iGPU rail is still a reading"
    );
    assert_eq!(
        power.dram_watts, None,
        "rail without accumulated energy is unsupported"
    );
    assert_eq!(power.npu_watts, None);
    assert_eq!(power.note, None);
}

#[test]
fn rapl_sums_multiple_packages() {
    let power = rapl_power_from_samples(
        &samples(&[
            ("rapl_package0_pkg", 50_000.0),
            ("rapl_package1_pkg", 45_000.0),
        ]),
        &[],
    );
    assert_eq!(power.package_watts, Some(95.0));
}

#[test]
fn thermal_zones_convert_kelvin_and_drop_implausible_values() {
    let zones = thermal_zones_celsius(
        &samples(&[(r"\_TZ.TZ00", 3010.0), (r"\_TZ.TZ01", 0.0)]),
        true,
    );
    assert_eq!(zones.len(), 1);
    assert!((zones[0].1 - 27.85).abs() < 1e-3);

    let whole = thermal_zones_celsius(&samples(&[(r"\_TZ.CPUZ", 333.0)]), false);
    assert!((whole[0].1 - 59.85).abs() < 1e-3);

    let readings = PlatformSensorReadings {
        thermal_zones: vec![("a".into(), 40.0), ("b".into(), 61.5)],
        ..Default::default()
    };
    assert_eq!(readings.hottest_zone(), Some(61.5));
    assert!(!readings.has_power());
}

#[test]
fn thermal_zone_tracker_drops_static_placeholder_zones() {
    let mut tracker = ThermalZoneTracker::new();
    let zones = |tz00: f32, cpuz: f32| {
        vec![
            (r"\_TZ.TZ00".to_owned(), tz00),
            (r"\_TZ.CPUZ".to_owned(), cpuz),
        ]
    };
    // Nothing is trusted until it has moved.
    assert!(tracker.observe(zones(27.85, 45.0)).is_empty());
    assert_eq!(
        tracker.observe(zones(27.85, 47.0)),
        vec![(r"\_TZ.CPUZ".to_owned(), 47.0)]
    );
    // A zone stays live once it has changed, even if it then holds steady.
    assert_eq!(tracker.observe(zones(27.85, 47.0)).len(), 1);
}

#[test]
fn selects_hwinfo_intel_core_and_package_temps() {
    let readings = [
        ("Core #0", 64.0),
        ("Core #1", 64.0),
        ("Core #2", 63.0),
        ("Core #0 Distance to TjMAX", 36.0),
        ("Core Max", 69.0),
        ("CPU Package", 66.0),
        ("Core #10", 59.0),
    ];
    let readings: Vec<(String, f32)> = readings.iter().map(|(l, t)| ((*l).into(), *t)).collect();
    let temps = select_sensor_cpu_temps(&readings).unwrap();
    assert_eq!(temps.package, Some(66.0));
    // Numeric (not lexical) order, TjMAX distance and "Core Max" excluded.
    assert_eq!(temps.cores, vec![64.0, 64.0, 63.0, 59.0]);
}

#[test]
fn selects_lhm_hybrid_and_amd_labels() {
    let hybrid: Vec<(String, f32)> = [
        ("E-Core #1", 50.0),
        ("P-Core #2", 61.0),
        ("P-Core #1", 60.0),
        ("CPU Core #1 Distance to TjMax", 40.0),
        ("Core Average", 55.0),
    ]
    .iter()
    .map(|(l, t)| ((*l).into(), *t))
    .collect();
    let temps = select_sensor_cpu_temps(&hybrid).unwrap();
    // P-cores first, then E-cores; package falls back to the hottest core.
    assert_eq!(temps.cores, vec![60.0, 61.0, 50.0]);
    assert_eq!(temps.package, Some(61.0));

    let amd: Vec<(String, f32)> = [("Core (Tctl/Tdie)", 71.5), ("CCD1 (Tdie)", 65.0)]
        .iter()
        .map(|(l, t)| ((*l).into(), *t))
        .collect();
    let temps = select_sensor_cpu_temps(&amd).unwrap();
    assert_eq!(temps.package, Some(71.5));
    assert!(temps.cores.is_empty());

    assert_eq!(
        select_sensor_cpu_temps(&[("GPU Hot Spot".into(), 70.0)]),
        None
    );
}

#[test]
fn spreads_core_temps_over_hyperthreads() {
    // 3 cores with 2 threads each, then a single-threaded E-core.
    let logical_to_core = [0, 0, 1, 1, 2, 2, 3];
    assert_eq!(
        core_temps_per_logical_cpu(&[60.0, 61.0, 62.0, 50.0], &logical_to_core),
        vec![60.0, 60.0, 61.0, 61.0, 62.0, 62.0, 50.0]
    );
    // Sensor count doesn't match topology: leave as-is.
    assert_eq!(
        core_temps_per_logical_cpu(&[60.0, 61.0], &logical_to_core),
        vec![60.0, 61.0]
    );
}

fn hwinfo_buffer(sensors: &[&str], readings: &[(u32, u32, &str, &str, f64)]) -> Vec<u8> {
    const HEADER: usize = 48;
    const SENSOR: usize = 8 + 128 + 128;
    const READING: usize = 12 + 128 + 128 + 16 + 32;
    let fixed = |text: &[u8], len: usize| {
        let mut field = text.to_vec();
        field.resize(len, 0);
        field
    };
    let mut buf = Vec::new();
    buf.extend(0x5369_5748u32.to_le_bytes());
    buf.extend(2u32.to_le_bytes());
    buf.extend(0u32.to_le_bytes());
    buf.extend(0i64.to_le_bytes());
    for value in [
        HEADER,
        SENSOR,
        sensors.len(),
        HEADER + SENSOR * sensors.len(),
        READING,
        readings.len(),
    ] {
        buf.extend((value as u32).to_le_bytes());
    }
    buf.resize(HEADER, 0);
    for (i, name) in sensors.iter().enumerate() {
        buf.extend((i as u32).to_le_bytes());
        buf.extend(0u32.to_le_bytes());
        buf.extend(fixed(name.as_bytes(), 128));
        buf.extend(fixed(b"", 128));
    }
    for (i, (kind, sensor, label, unit, value)) in readings.iter().enumerate() {
        buf.extend(kind.to_le_bytes());
        buf.extend(sensor.to_le_bytes());
        buf.extend((i as u32).to_le_bytes());
        buf.extend(fixed(label.as_bytes(), 128));
        buf.extend(fixed(b"", 128));
        buf.extend(fixed(unit.as_bytes(), 16));
        for _ in 0..4 {
            buf.extend(value.to_le_bytes());
        }
    }
    buf
}

#[test]
fn parses_hwinfo_shared_memory_cpu_temps() {
    let buf = hwinfo_buffer(
        &[
            "CPU [#0]: Intel Core i5-12400: DTS",
            "GPU [#0]: NVIDIA GeForce RTX 3060",
        ],
        &[
            (HWINFO_READING_TEMP, 0, "Core #0", "\u{b0}C", 64.0),
            (HWINFO_READING_TEMP, 0, "CPU Package", "\u{b0}F", 150.8),
            (7, 0, "Core #0 T0 Usage", "%", 12.0),
            (HWINFO_READING_TEMP, 1, "GPU Temperature", "\u{b0}C", 55.0),
        ],
    );
    let readings = parse_hwinfo_shared_memory(&buf).unwrap();
    assert_eq!(readings.len(), 4);
    assert_eq!(readings[3].sensor, "GPU [#0]: NVIDIA GeForce RTX 3060");

    let cpu = hwinfo_cpu_temperatures(&readings);
    assert_eq!(cpu.len(), 2);
    assert_eq!(cpu[0], ("Core #0".to_owned(), 64.0));
    assert!((cpu[1].1 - 66.0).abs() < 1e-3, "°F converted: {}", cpu[1].1);

    // Inactive ("DEAD") or truncated memory is rejected.
    let mut dead = buf.clone();
    dead[..4].copy_from_slice(b"DEAD");
    assert_eq!(parse_hwinfo_shared_memory(&dead), None);
    assert_eq!(parse_hwinfo_shared_memory(&buf[..buf.len() - 40]), None);
}

#[test]
fn sensor_tool_temps_take_precedence_over_acpi_zones() {
    let readings = PlatformSensorReadings {
        thermal_zones: vec![("zone".into(), 40.0)],
        ..Default::default()
    };
    assert_eq!(readings.cpu_temperatures(), (Some(40.0), Vec::new()));

    let readings = PlatformSensorReadings {
        cpu: Some(CpuTemperatures {
            package: Some(66.0),
            cores: vec![64.0, 64.0],
        }),
        ..readings
    };
    assert_eq!(readings.cpu_temperatures(), (Some(66.0), vec![64.0, 64.0]));
}

#[test]
fn intel_dts_is_tjmax_minus_readout() {
    // TjMax 100 °C (bits 23:16 of MSR 0x1A2), readout 36 (bits 22:16).
    let target = 100u64 << 16;
    let valid = 1u64 << 31;
    assert_eq!(
        intel_dts_celsius(target, valid | (36 << 16), true),
        Some(64.0)
    );
    // Per-core readings without the valid bit are ignored...
    assert_eq!(intel_dts_celsius(target, 36 << 16, true), None);
    // ...but the package register has no such bit.
    assert_eq!(intel_dts_celsius(target, 34 << 16, false), Some(66.0));
    // Missing TjMax falls back to 100 °C; readout at TjMax is implausible.
    assert_eq!(intel_dts_celsius(0, valid | (5 << 16), true), Some(95.0));
    assert_eq!(intel_dts_celsius(target, valid | (100 << 16), true), None);
}

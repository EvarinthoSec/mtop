//! CPU temperature selection from raw sensor (label, °C) pairs.
use mtop::platform::select_cpu_temps;

#[test]
fn apple_silicon_pmu_tdie_sensors_feed_package_and_cores() {
    let s = vec![
        ("NAND CH0 temp".to_owned(), Some(53.0)),
        ("gas gauge battery".to_owned(), Some(38.0)),
        ("PMU tdie1".to_owned(), Some(71.8)),
        ("PMU tdie2".to_owned(), Some(64.8)),
        ("PMU tdie10".to_owned(), Some(63.7)),
        ("PMU tdev1".to_owned(), Some(-9202.8)),
        ("PMU2 tdie1".to_owned(), Some(53.0)),
        ("als-temp".to_owned(), None),
    ];
    let (pkg, cores) = select_cpu_temps(&s);
    assert_eq!(pkg, Some(71.8), "package = hottest die sensor");
    // PMU (not PMU2) dies, in numeric order: tdie1, tdie2, tdie10.
    assert_eq!(cores, vec![71.8, 64.8, 63.7]);
}

#[test]
fn implausible_readings_are_ignored() {
    let s = vec![
        ("PMU tdie1".to_owned(), Some(-9202.8)),
        ("PMU tdie2".to_owned(), Some(250.0)),
        ("PMU tdie3".to_owned(), Some(55.0)),
    ];
    let (pkg, cores) = select_cpu_temps(&s);
    assert_eq!(pkg, Some(55.0));
    assert_eq!(cores, vec![55.0]);
}

#[test]
fn linux_coretemp_labels_still_work() {
    let s = vec![
        ("coretemp Package id 0".to_owned(), Some(60.0)),
        ("coretemp Core 0".to_owned(), Some(55.0)),
        ("coretemp Core 1".to_owned(), Some(58.0)),
        ("acpitz temp1".to_owned(), Some(40.0)),
    ];
    let (pkg, cores) = select_cpu_temps(&s);
    assert_eq!(pkg, Some(60.0));
    assert_eq!(cores, vec![55.0, 58.0]);
}

#[test]
fn amd_tctl_is_package() {
    let s = vec![("k10temp Tctl".to_owned(), Some(66.0))];
    assert_eq!(select_cpu_temps(&s), (Some(66.0), vec![]));
}

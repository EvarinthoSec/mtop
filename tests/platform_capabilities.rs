use mtop::config::Config;
use mtop::platform::{SysinfoCollector, current_os_capabilities};

#[test]
fn current_os_capabilities_is_deterministic() {
    let first = current_os_capabilities();
    let second = current_os_capabilities();

    assert_eq!(first, second);
    assert!(!first.os_name.is_empty());
}

#[cfg(all(target_os = "linux", not(feature = "gpu-nvidia")))]
#[test]
fn linux_capabilities_match_contract() {
    let capabilities = current_os_capabilities();

    assert_eq!(capabilities.os_name, "linux");
    assert!(capabilities.process_details);
    assert!(capabilities.disk_io_counters);
    assert!(capabilities.network_counters);
    assert!(capabilities.load_average);
    assert!(capabilities.gpu_platform_support);
    assert!(!capabilities.gpu_backend_compiled);
}

#[cfg(all(target_os = "linux", feature = "gpu-nvidia"))]
#[test]
fn linux_gpu_backend_capability_matches_compiled_feature() {
    let capabilities = current_os_capabilities();

    assert!(capabilities.gpu_platform_support);
    assert!(capabilities.gpu_backend_compiled);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_capabilities_match_contract() {
    let capabilities = current_os_capabilities();

    assert_eq!(capabilities.os_name, "macos");
    assert!(capabilities.process_details);
    assert!(capabilities.disk_io_counters);
    assert!(capabilities.network_counters);
    assert!(capabilities.load_average);
    assert!(!capabilities.gpu_platform_support);
    assert!(!capabilities.gpu_backend_compiled);
}

#[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
#[test]
fn bsd_capabilities_match_contract() {
    let capabilities = current_os_capabilities();

    assert!(matches!(
        capabilities.os_name,
        "freebsd" | "openbsd" | "netbsd"
    ));
    assert!(capabilities.process_details);
    assert!(!capabilities.disk_io_counters);
    assert!(capabilities.network_counters);
    assert!(capabilities.load_average);
    assert!(!capabilities.gpu_platform_support);
    assert!(!capabilities.gpu_backend_compiled);
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
)))]
#[test]
fn unknown_target_capabilities_match_contract() {
    let capabilities = current_os_capabilities();

    assert_eq!(capabilities.os_name, "unknown");
    assert!(!capabilities.process_details);
    assert!(!capabilities.disk_io_counters);
    assert!(!capabilities.network_counters);
    assert!(!capabilities.load_average);
    assert!(!capabilities.gpu_platform_support);
    assert!(!capabilities.gpu_backend_compiled);
}

#[test]
fn common_collector_constructs_with_default_config() {
    let _collector = SysinfoCollector::new(&Config::default());
}

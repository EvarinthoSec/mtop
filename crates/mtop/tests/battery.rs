//! btop battery meter: parsed from native macOS, Linux, and Windows sources,
//! shown in the cpu box title with charge state glyph.
use mtop::model::{BatterySnapshot, BatteryState, CpuSnapshot, SystemSnapshot};
use mtop::platform::{parse_pmset_batt, parse_sysfs_battery, parse_windows_power_status};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn pmset_output_parses_percent_state_and_time() {
    let out = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=24510563)\t64%; discharging; 3:12 remaining present: true\n";
    let b = parse_pmset_batt(out).unwrap();
    assert_eq!(b.percent, 64.0);
    assert_eq!(b.state, BatteryState::Discharging);
    assert_eq!(b.seconds_left, Some(3 * 3600 + 12 * 60));
}

#[test]
fn pmset_ac_states_and_no_estimate() {
    let full = parse_pmset_batt(
        " -InternalBattery-0 (id=1)\t80%; AC attached; not charging present: true",
    )
    .unwrap();
    assert_eq!(
        (full.percent, full.state, full.seconds_left),
        (80.0, BatteryState::Full, None)
    );
    let charging =
        parse_pmset_batt(" -InternalBattery-0 (id=1)\t41%; charging; (no estimate) present: true")
            .unwrap();
    assert_eq!(
        (charging.state, charging.seconds_left),
        (BatteryState::Charging, None)
    );
}

#[test]
fn desktop_mac_without_battery_is_none() {
    assert!(parse_pmset_batt("Now drawing from 'AC Power'\n").is_none());
    assert!(parse_pmset_batt("").is_none());
}

#[test]
fn linux_sysfs_battery_parses() {
    let b = parse_sysfs_battery("87\n", "Charging\n").unwrap();
    assert_eq!((b.percent, b.state), (87.0, BatteryState::Charging));
    let b = parse_sysfs_battery("100", "Full").unwrap();
    assert_eq!(b.state, BatteryState::Full);
    assert!(parse_sysfs_battery("garbage", "Charging").is_none());
}

#[test]
fn windows_power_status_parses_charge_and_remaining_time() {
    let discharging = parse_windows_power_status(0, 0, 64, 11_520).unwrap();
    assert_eq!(discharging.percent, 64.0);
    assert_eq!(discharging.state, BatteryState::Discharging);
    assert_eq!(discharging.seconds_left, Some(11_520));

    let charging = parse_windows_power_status(1, 8, 41, u32::MAX).unwrap();
    assert_eq!(charging.state, BatteryState::Charging);
    assert_eq!(charging.seconds_left, None);
}

#[test]
fn windows_power_status_handles_no_battery_and_unknown_percent() {
    assert!(parse_windows_power_status(1, 128, 255, u32::MAX).is_none());
    assert!(parse_windows_power_status(1, 0, 255, u32::MAX).is_none());
}

#[test]
fn cpu_box_title_shows_battery_meter() {
    let snap = SystemSnapshot {
        cpu: CpuSnapshot {
            available: true,
            ..CpuSnapshot::default()
        },
        battery: Some(BatterySnapshot {
            percent: 64.0,
            state: BatteryState::Discharging,
            seconds_left: Some(3 * 3600 + 12 * 60),
        }),
        ..SystemSnapshot::default()
    };
    let v = AppView::new(snap, Theme::from_name("neon"));
    let mut t = Terminal::new(TestBackend::new(150, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, &v)).unwrap();
    let top: String = (0..150)
        .map(|x| t.backend().buffer()[(x, 1)].symbol().to_string())
        .collect();
    assert!(top.contains("BAT▼"), "discharging arrow in title: {top}");
    assert!(top.contains("64%"), "percent in title: {top}");
    assert!(top.contains("3:12"), "time left in title: {top}");
    assert!(top.contains('■'), "meter in title: {top}");
}

#[test]
fn cpu_title_clock_is_wall_time_not_uptime() {
    use std::time::{Duration, SystemTime};
    // Uptime 5h00m must NOT be shown as "05:00": the clock is local time of
    // the snapshot, HH:MM:SS like btop.
    let snap = SystemSnapshot {
        uptime: Duration::from_secs(5 * 3600),
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(12 * 3600 + 34 * 60 + 56),
        ..SystemSnapshot::default()
    };
    let v = AppView::new(snap, Theme::from_name("neon"));
    let mut t = Terminal::new(TestBackend::new(150, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, &v)).unwrap();
    let top: String = (0..150)
        .map(|x| t.backend().buffer()[(x, 1)].symbol().to_string())
        .collect();
    assert!(!top.contains("05:00"), "uptime leaked into clock: {top}");
    let clock = mtop::ui::clock_string(v.snapshot.captured_at);
    assert_eq!(clock.len(), 8, "HH:MM:SS");
    assert!(top.contains(&clock), "title shows {clock}: {top}");
}

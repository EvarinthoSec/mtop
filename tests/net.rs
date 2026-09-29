//! btop net box: b/n cycle interface, z resets totals, a auto-scale, y sync.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{NetworkSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, SystemTime};

fn press(v: &mut AppView, c: char) {
    v.feed_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn nic(name: &str, rx: u64, tx: u64, total_rx: u64) -> NetworkSnapshot {
    NetworkSnapshot {
        interface: name.into(),
        received_bytes_per_second: rx,
        transmitted_bytes_per_second: tx,
        received_bytes_total: Some(total_rx),
        transmitted_bytes_total: Some(total_rx / 2),
    }
}

fn snap(t: u64, nets: Vec<NetworkSnapshot>) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        networks: nets,
        ..SystemSnapshot::default()
    }
}

fn view_with_two_nics() -> AppView {
    let mut v = AppView::new(snap(0, vec![]), Theme::from_name("neon"));
    for t in 1..=3 {
        v.accept_snapshot(snap(
            t,
            vec![
                nic("en0", 1000 * t, 100, 5 << 30),
                nic("utun3", 7, 9, 1 << 20),
            ],
        ));
    }
    v
}

fn render(v: &AppView) -> String {
    let mut t = Terminal::new(TestBackend::new(140, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn history_is_kept_per_interface() {
    let v = view_with_two_nics();
    assert_eq!(v.history.net_rx("en0"), vec![1000, 2000, 3000]);
    assert_eq!(v.history.net_rx("utun3"), vec![7, 7, 7]);
}

#[test]
fn n_and_b_cycle_the_shown_interface() {
    let mut v = view_with_two_nics();
    assert_eq!(v.net_iface(), Some("en0"));
    assert!(render(&v).contains("³net en0"));
    press(&mut v, 'n');
    assert_eq!(v.net_iface(), Some("utun3"));
    assert!(render(&v).contains("³net utun3"));
    press(&mut v, 'n');
    assert_eq!(v.net_iface(), Some("en0"), "n wraps");
    press(&mut v, 'b');
    assert_eq!(v.net_iface(), Some("utun3"), "b goes back and wraps");
}

#[test]
fn selected_interface_survives_reordering_between_snapshots() {
    let mut v = view_with_two_nics();
    press(&mut v, 'n');
    v.accept_snapshot(snap(9, vec![nic("utun3", 1, 1, 1), nic("en0", 1, 1, 1)]));
    assert_eq!(v.net_iface(), Some("utun3"));
}

#[test]
fn z_zeroes_totals_for_the_current_interface() {
    let mut v = view_with_two_nics();
    assert!(render(&v).contains("5.0 GiB"));
    press(&mut v, 'z');
    let t = render(&v);
    assert!(!t.contains("5.0 GiB"), "total resets to 0 after z");
    assert!(t.contains("zero■"), "title shows zero flag on");
    v.accept_snapshot(snap(
        10,
        vec![
            nic("en0", 1, 1, (5 << 30) + 2048),
            nic("utun3", 7, 9, 1 << 20),
        ],
    ));
    assert!(
        render(&v).contains("2.0 KiB"),
        "counts only traffic since reset"
    );
    press(&mut v, 'z');
    assert!(
        render(&v).contains("5.0 GiB"),
        "z again restores absolute totals"
    );
}

#[test]
fn a_and_y_toggle_scaling_flags_shown_in_title() {
    let mut v = view_with_two_nics();
    let t = render(&v);
    assert!(
        t.contains("auto■") && t.contains("sync□"),
        "defaults: auto on, sync off"
    );
    press(&mut v, 'a');
    press(&mut v, 'y');
    let t = render(&v);
    assert!(t.contains("auto□") && t.contains("sync■"));
}

#[test]
fn sync_scaling_uses_one_max_for_both_graphs() {
    let mut v = view_with_two_nics();
    assert_ne!(
        v.net_scale(),
        (v.net_scale().0, v.net_scale().0),
        "independent by default"
    );
    press(&mut v, 'y');
    let (rx, tx) = v.net_scale();
    assert_eq!(rx, tx, "sync: shared ceiling");
    assert_eq!(rx, 3000);
}

#[test]
fn manual_scale_uses_fixed_ceiling_when_auto_off() {
    let mut v = view_with_two_nics();
    press(&mut v, 'a');
    // btop_draw.cpp: (net_download << 20) / 8 — default 100 Mebibit.
    assert_eq!(v.net_scale(), (13_107_200, 13_107_200));
}

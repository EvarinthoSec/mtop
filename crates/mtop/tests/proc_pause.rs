//! btop `u`: pause the process list only; cpu/mem/net keep updating.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{CpuSnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, SystemTime};

fn snap(t: u64, cpu: f32, procs: &[(u32, &str)]) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        cpu: CpuSnapshot {
            available: true,
            overall_percent: cpu,
            ..CpuSnapshot::default()
        },
        processes: procs
            .iter()
            .map(|(pid, name)| ProcessSnapshot {
                pid: *pid,
                name: (*name).into(),
                ..ProcessSnapshot::default()
            })
            .collect(),
        ..SystemSnapshot::default()
    }
}

fn press(v: &mut AppView, c: char) {
    v.feed_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

#[test]
fn u_freezes_process_list_but_not_system_metrics() {
    let mut v = AppView::new(snap(1, 10.0, &[(1, "alpha")]), Theme::from_name("neon"));
    press(&mut v, 'u');
    assert!(v.proc_paused);
    assert!(!v.paused, "u must not pause the whole dashboard");
    v.accept_snapshot(snap(2, 90.0, &[(2, "beta")]));
    assert_eq!(v.snapshot.cpu.overall_percent, 90.0, "cpu keeps updating");
    let names: Vec<_> = v
        .snapshot
        .processes
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["alpha"], "process list is frozen");

    let mut t = Terminal::new(TestBackend::new(140, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, &v)).unwrap();
    let text: String = t
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("paused■"), "proc title flags the pause");

    press(&mut v, 'u');
    v.accept_snapshot(snap(3, 50.0, &[(3, "gamma")]));
    assert_eq!(
        v.snapshot.processes[0].name, "gamma",
        "unpaused list refreshes"
    );
}

#[test]
fn space_still_pauses_everything() {
    let mut v = AppView::new(snap(1, 10.0, &[]), Theme::from_name("neon"));
    press(&mut v, ' ');
    assert!(v.paused);
    assert!(!v.proc_paused);
}

//! macOS cached memory (vm_stat file-backed pages) and btop `d` / `C` keys.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{DiskSnapshot, MemorySnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::platform::parse_vm_stat_cached;
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

const VM_STAT: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                    12648.
Pages active:                                 205438.
Pages inactive:                               203616.
File-backed pages:                            143271.
Anonymous pages:                              266463.
";

#[test]
fn vm_stat_file_backed_pages_become_cached_bytes() {
    assert_eq!(parse_vm_stat_cached(VM_STAT), Some(143_271 * 16_384));
}

#[test]
fn vm_stat_without_page_size_or_field_is_none() {
    assert_eq!(parse_vm_stat_cached("File-backed pages: 10.\n"), None);
    assert_eq!(
        parse_vm_stat_cached("Mach Virtual Memory Statistics: (page size of 4096 bytes)\n"),
        None
    );
}

fn key(v: &mut AppView, c: char) {
    v.feed_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
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
fn d_toggles_disks_view_in_mem_box() {
    let snap = SystemSnapshot {
        memory: MemorySnapshot {
            total_bytes: 16 << 30,
            used_bytes: 8 << 30,
            ..MemorySnapshot::default()
        },
        disks: vec![DiskSnapshot {
            name: "disk0".into(),
            mount_point: "/".into(),
            total_bytes: 100 << 30,
            available_bytes: 40 << 30,
            read_bytes_per_second: None,
            write_bytes_per_second: None,
        }],
        ..SystemSnapshot::default()
    };
    let mut v = AppView::new(snap, Theme::from_name("neon"));
    assert!(render(&v).contains("disks"));
    key(&mut v, 'd');
    assert!(!v.show_disks);
    let t = render(&v);
    assert!(!t.contains("disks  io"), "disks box hidden");
    assert!(t.contains("²mem"), "mem box stays");
    key(&mut v, 'd');
    assert!(render(&v).contains("disks"));
}

fn tree_view() -> AppView {
    let p = |pid, ppid: Option<u32>, name: &str| ProcessSnapshot {
        pid,
        parent_pid: ppid,
        name: name.into(),
        ..ProcessSnapshot::default()
    };
    let snap = SystemSnapshot {
        processes: vec![
            p(1, None, "launchd"),
            p(10, Some(1), "sshd"),
            p(11, Some(10), "zsh"),
            p(20, Some(1), "cron"),
        ],
        ..SystemSnapshot::default()
    };
    let mut v = AppView::new(snap, Theme::from_name("neon"));
    key(&mut v, 'e');
    v
}

#[test]
fn shift_c_collapses_all_children_of_selected() {
    let mut v = tree_view();
    // Selected = launchd. C collapses each of its children (btop), so the
    // direct children stay visible but grandchildren hide.
    key(&mut v, 'C');
    let names: Vec<_> = v
        .display_indices()
        .into_iter()
        .map(|i| v.snapshot.processes[i].name.clone())
        .collect();
    assert_eq!(names, ["launchd", "sshd", "cron"]);
    key(&mut v, 'C');
    assert_eq!(v.display_indices().len(), 4, "C again expands them");
}

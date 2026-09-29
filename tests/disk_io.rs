//! btop `i`: disks box switches to big read/write IO graphs.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{DiskSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::time::{Duration, SystemTime};

fn snap(t: u64, r: u64, w: u64) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        disks: vec![DiskSnapshot {
            name: "disk0".into(),
            mount_point: "/".into(),
            total_bytes: 100 << 30,
            available_bytes: 40 << 30,
            read_bytes_per_second: Some(r),
            write_bytes_per_second: Some(w),
        }],
        ..SystemSnapshot::default()
    }
}

fn render(v: &AppView) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(140, 44)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend().buffer().clone()
}

fn braille_in_disks_box(b: &Buffer) -> usize {
    let text: Vec<String> = (0..b.area.height)
        .map(|y| {
            (0..b.area.width)
                .map(|x| b[(x, y)].symbol().to_string())
                .collect()
        })
        .collect();
    let top = text
        .iter()
        .position(|l| l.contains("disks"))
        .expect("disks box");
    let col = text[top].chars().position(|c| c == '╭').unwrap_or(0);
    text[top + 1..]
        .iter()
        .take_while(|l| !l.chars().nth(col).is_some_and(|c| c == '╰'))
        .flat_map(|l| l.chars().skip(col).take(60))
        .filter(|c| ('\u{2801}'..='\u{28FF}').contains(c))
        .count()
}

#[test]
fn i_toggles_disk_io_graph_mode() {
    let mut v = AppView::new(snap(0, 0, 0), Theme::from_name("neon"));
    for t in 1..40 {
        v.accept_snapshot(snap(t, t * 1_000_000, (40 - t) * 500_000));
    }
    assert!(!v.disk_io_mode);
    assert_eq!(
        braille_in_disks_box(&render(&v)),
        0,
        "usage view has no graph"
    );
    v.feed_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    assert!(v.disk_io_mode);
    let b = render(&v);
    assert!(
        braille_in_disks_box(&b) > 20,
        "io mode draws read/write graphs"
    );
    let all: String = b.content().iter().map(|c| c.symbol()).collect();
    assert!(all.contains("io■"), "title flags io mode");
    assert!(all.contains("read") && all.contains("write"));
}

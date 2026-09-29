//! btop detailed process view (Enter): per-pid cpu graph, mem %, cmdline,
//! nice, parent — kept across refreshes for the selected pid.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{MemorySnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::time::{Duration, SystemTime};

fn snap(t: u64, cpu: f32) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        memory: MemorySnapshot {
            total_bytes: 1000,
            ..MemorySnapshot::default()
        },
        processes: vec![ProcessSnapshot {
            pid: 777,
            parent_pid: Some(1),
            name: "render".into(),
            command: "/usr/bin/render --threads 8 --scene city.blend".into(),
            user: Some("ronnakon".into()),
            cpu_percent: cpu,
            memory_bytes: 250,
            threads: Some(8),
            status: "Run".into(),
            ..ProcessSnapshot::default()
        }],
        ..SystemSnapshot::default()
    }
}

fn render(v: &AppView) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(160, 44)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend().buffer().clone()
}

fn text(b: &Buffer) -> String {
    b.content().iter().map(|c| c.symbol()).collect()
}

fn detail_view() -> AppView {
    let mut v = AppView::new(snap(0, 10.0), Theme::from_name("neon"));
    for t in 1..30 {
        v.accept_snapshot(snap(t, (t * 3) as f32));
    }
    v.feed_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    v
}

#[test]
fn per_pid_cpu_history_is_recorded() {
    let v = detail_view();
    let h = v.history.proc_cpu(777);
    assert_eq!(h.len(), 29);
    assert_eq!(h.last().copied(), Some(87));
}

#[test]
fn detail_shows_cmdline_mem_percent_and_parent() {
    let t = text(&render(&detail_view()));
    assert!(t.contains("--scene city.blend"), "full command line");
    assert!(t.contains("25.0%"), "mem as % of RAM (250/1000)");
    assert!(
        t.contains("Parent:") && t.contains("Nice:"),
        "btop detail fields"
    );
}

#[test]
fn detail_draws_cpu_graph_for_the_process() {
    let b = render(&detail_view());
    let rows: Vec<String> = (0..b.area.height)
        .map(|y| {
            (0..b.area.width)
                .map(|x| b[(x, y)].symbol().to_string())
                .collect()
        })
        .collect();
    let start = rows
        .iter()
        .position(|r| r.contains("--scene"))
        .expect("detail row");
    let braille: usize = rows[start.saturating_sub(4)..start + 4]
        .iter()
        .flat_map(|r| r.chars())
        .filter(|c| ('\u{2801}'..='\u{28FF}').contains(c))
        .count();
    assert!(
        braille >= 10,
        "detail panel has a braille cpu graph, found {braille}"
    );
}

#[test]
fn history_for_exited_processes_is_dropped() {
    let mut v = detail_view();
    let mut s = snap(99, 1.0);
    s.processes.clear();
    v.accept_snapshot(s);
    assert!(v.history.proc_cpu(777).is_empty(), "no unbounded growth");
}

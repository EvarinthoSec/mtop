//! Swap history graph in the mem box, and GPU renderer/tiler stats.
use mtop::model::{GpuSnapshot, MemorySnapshot, SystemSnapshot};
use mtop::platform::parse_ioreg_gpus;
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, SystemTime};

fn snap(t: u64, swap_used: u64) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        memory: MemorySnapshot {
            total_bytes: 16 << 30,
            used_bytes: 8 << 30,
            swap_total_bytes: 16 << 30,
            swap_used_bytes: swap_used,
            ..MemorySnapshot::default()
        },
        gpus: vec![GpuSnapshot {
            name: "Apple M5".into(),
            utilization_percent: Some(49.0),
            renderer_percent: Some(49.0),
            tiler_percent: Some(3.0),
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: None,
        }],
        ..SystemSnapshot::default()
    }
}

#[test]
fn swap_percent_history_is_recorded() {
    let mut v = AppView::new(snap(0, 0), Theme::from_name("neon"));
    v.accept_snapshot(snap(1, 8 << 30));
    v.accept_snapshot(snap(2, 12 << 30));
    let tail: Vec<u64> = v.history.swap.iter().rev().take(2).copied().collect();
    assert_eq!(tail, vec![75, 50]);
}

#[test]
fn mem_box_labels_swap_graph() {
    let mut v = AppView::new(snap(0, 1 << 30), Theme::from_name("neon"));
    for t in 1..40 {
        v.accept_snapshot(snap(t, (t % 16) << 30));
    }
    let mut term = Terminal::new(TestBackend::new(150, 50)).unwrap();
    term.draw(|f| draw_dashboard(f, &v)).unwrap();
    let text: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("swap"), "swap graph label in mem box");
}

#[test]
fn ioreg_renderer_and_tiler_are_parsed() {
    let g = parse_ioreg_gpus(
        "+-o AGX  <class AGX>\n  \"PerformanceStatistics\" = {\"Tiler Utilization %\"=3,\"Renderer Utilization %\"=49,\"Device Utilization %\"=49}\n",
    );
    assert_eq!(g[0].renderer_percent, Some(49.0));
    assert_eq!(g[0].tiler_percent, Some(3.0));
}

#[test]
fn gpu_box_shows_renderer_and_tiler() {
    let v = AppView::new(snap(0, 0), Theme::from_name("neon"));
    let mut term = Terminal::new(TestBackend::new(150, 44)).unwrap();
    term.draw(|f| draw_dashboard(f, &v)).unwrap();
    let text: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        text.contains("Render 49%") && text.contains("Tiler 3%"),
        "gpu engine stats"
    );
}

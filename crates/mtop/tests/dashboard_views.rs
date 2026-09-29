use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mtop::model::{NpuSnapshot, PowerSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, DashboardPage, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

fn sample() -> SystemSnapshot {
    SystemSnapshot {
        npus: vec![NpuSnapshot {
            name: "Apple Neural Engine".into(),
            utilization_percent: None,
            power_watts: Some(1.25),
            frequency_mhz: Some(800),
        }],
        power: PowerSnapshot {
            cpu_watts: Some(5.0),
            gpu_watts: Some(2.0),
            npu_watts: Some(1.25),
            dram_watts: Some(0.5),
            package_watts: Some(8.75),
            note: None,
        },
        ..SystemSnapshot::default()
    }
}

fn render_at(view: &AppView, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, view)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn render(view: &AppView) -> String {
    render_at(view, 150, 44)
}

#[test]
fn dashboard_pages_cycle_in_tab_order() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    assert_eq!(view.page, DashboardPage::Overview);

    view.feed_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(view.page, DashboardPage::Cpu);
    view.feed_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(view.page, DashboardPage::Overview);
}

#[test]
fn npu_page_renders_current_power_and_navigation_tabs() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    view.page = DashboardPage::Npu;
    view.record_snapshot();

    let text = render(&view);
    assert!(text.contains("Overview"));
    assert!(text.contains("NPU"));
    assert!(text.contains("1.25 W"));
    assert!(text.contains("Power"));
    assert!(text.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)));
}

#[test]
fn clicking_a_tab_switches_dashboard_page() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    let _ = render(&view); // records tab hit regions
    view.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 11, // CPU tab after "Overview | "
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(view.page, DashboardPage::Cpu);
}

#[test]
fn unavailable_npu_does_not_render_stale_graph_history() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    view.page = DashboardPage::Npu;
    view.record_snapshot();
    view.snapshot = SystemSnapshot::default();

    let text = render(&view);
    assert!(text.contains("No current NPU graph data"));
    assert!(!text.contains("1.25 W"));
    assert!(!text.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)));
}

#[test]
fn narrow_terminal_keeps_selected_tab_visible() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    view.page = DashboardPage::Power;
    let text = render_at(&view, 40, 12);
    assert!(text.contains("Power"));
}

#[test]
fn ratty_3d_panel_frames_are_explicitly_opt_in() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    assert!(!render(&view).contains("\u{1b}_ratty;g;p;"));

    view.ratty_3d = true;
    let text = render(&view);
    assert!(text.contains("\u{1b}_ratty;g;p;"));
    assert!(text.matches("\u{1b}_ratty;g;p;").count() >= 4);
}

#[test]
fn power_page_shows_each_available_component_without_fabricating_missing_values() {
    let mut view = AppView::new(sample(), Theme::from_name("neon"));
    view.page = DashboardPage::Power;

    let text = render(&view);
    assert!(text.contains("CPU") && text.contains("5.00 W"));
    assert!(text.contains("GPU") && text.contains("2.00 W"));
    assert!(text.contains("NPU") && text.contains("1.25 W"));
    assert!(text.contains("Package") && text.contains("8.75 W"));
    assert!(text.contains("DRAM") && text.contains("0.50 W"));
}

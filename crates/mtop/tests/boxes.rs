//! btop box toggles: 1 cpu, 2 mem, 3 net, 4 proc, 5 gpu. Hidden boxes free
//! their space for the rest (btop recalculates the layout).
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{GpuSnapshot, NetworkSnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn press(v: &mut AppView, c: char) {
    v.feed_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn view() -> AppView {
    let snap = SystemSnapshot {
        networks: vec![NetworkSnapshot {
            interface: "en0".into(),
            received_bytes_per_second: 10,
            transmitted_bytes_per_second: 10,
            received_bytes_total: None,
            transmitted_bytes_total: None,
        }],
        processes: vec![ProcessSnapshot {
            pid: 1,
            name: "launchd".into(),
            ..ProcessSnapshot::default()
        }],
        gpus: vec![GpuSnapshot {
            name: "Apple M5".into(),
            utilization_percent: Some(10.0),
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: None,
            ..GpuSnapshot::default()
        }],
        ..SystemSnapshot::default()
    };
    AppView::new(snap, Theme::from_name("neon"))
}

fn render(v: &AppView) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(140, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend().buffer().clone()
}

fn text(b: &Buffer) -> String {
    b.content().iter().map(|c| c.symbol()).collect()
}

/// Rect-ish probe: (first row, first col) where a box title appears.
fn title_pos(b: &Buffer, needle: &str) -> Option<(u16, u16)> {
    for y in 0..b.area.height {
        let line: String = (0..b.area.width)
            .map(|x| b[(x, y)].symbol().to_string())
            .collect();
        if let Some(byte) = line.find(needle) {
            let col = line[..byte].chars().count() as u16;
            return Some((y, col));
        }
    }
    None
}

#[test]
fn boxes_are_numbered_like_btop() {
    let b = render(&view());
    for t in ["¹cpu", "²mem", "³net", "⁴proc", "⁵gpu"] {
        assert!(text(&b).contains(t), "missing box title {t}");
    }
}

#[test]
fn number_keys_toggle_each_box() {
    for (key, title) in [
        ('1', "¹cpu"),
        ('2', "²mem"),
        ('3', "³net"),
        ('4', "⁴proc"),
        ('5', "⁵gpu"),
    ] {
        let mut v = view();
        press(&mut v, key);
        let t = text(&render(&v));
        assert!(!t.contains(title), "key {key} must hide {title}");
        press(&mut v, key);
        assert!(
            text(&render(&v)).contains(title),
            "key {key} again must show {title}"
        );
    }
}

#[test]
fn hiding_top_row_gives_its_rows_to_the_lower_boxes() {
    let mut v = view();
    let before = title_pos(&render(&v), "⁴proc").unwrap();
    press(&mut v, '1');
    press(&mut v, '5'); // gpu shares the top row; hide it too
    let after = title_pos(&render(&v), "⁴proc").unwrap();
    assert_eq!(
        after.0, 1,
        "proc must move to the top row when cpu+gpu are hidden"
    );
    assert!(before.0 > 0);
}

#[test]
fn hiding_proc_lets_left_column_span_full_width() {
    let mut v = view();
    press(&mut v, '4');
    let b = render(&v);
    let (y, _) = title_pos(&b, "²mem").unwrap();
    let line: String = (0..b.area.width)
        .map(|x| b[(x, y)].symbol().to_string())
        .collect();
    assert!(
        line.trim_end().ends_with('╮'),
        "mem box should reach the right edge: {line}"
    );
    assert_eq!(
        line.chars().filter(|c| *c == '╮').count(),
        1,
        "single full-width box: {line}"
    );
}

#[test]
fn hiding_mem_and_net_gives_proc_full_width() {
    let mut v = view();
    press(&mut v, '2');
    press(&mut v, '3');
    let b = render(&v);
    let (y, x) = title_pos(&b, "⁴proc").unwrap();
    assert!(x <= 3, "proc starts at the left edge, got col {x}");
    assert!(y > 0);
}

#[test]
fn all_boxes_hidden_shows_hint_instead_of_blank_screen() {
    let mut v = view();
    for k in ['1', '2', '3', '4', '5'] {
        press(&mut v, k);
    }
    assert!(text(&render(&v)).contains("No boxes shown"));
}

#[test]
fn per_core_toggle_lives_in_options() {
    let mut v = view();
    assert!(v.show_cores);
    press(&mut v, 'o');
    for _ in 0..3 {
        v.feed_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    v.feed_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!v.show_cores);
}

#[test]
fn gpu_takes_the_whole_top_row_when_cpu_is_hidden() {
    let mut v = view();
    press(&mut v, '1');
    let b = render(&v);
    let (y, x) = title_pos(&b, "⁵gpu").unwrap();
    assert_eq!((y, x <= 3), (1, true), "gpu should move below navigation");
}

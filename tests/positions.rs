//! btop alternate box positions: cpu_bottom, proc_left, mem_below_net —
//! set via options or the preset P flag ("cpu:1:default").
use mtop::config::Config;
use mtop::model::{NetworkSnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard, parse_preset_positions};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn view() -> AppView {
    let snap = SystemSnapshot {
        networks: vec![NetworkSnapshot {
            interface: "en0".into(),
            received_bytes_per_second: 1,
            transmitted_bytes_per_second: 1,
            received_bytes_total: None,
            transmitted_bytes_total: None,
        }],
        processes: vec![ProcessSnapshot {
            pid: 1,
            name: "launchd".into(),
            ..ProcessSnapshot::default()
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

fn pos(b: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..b.area.height {
        let line: String = (0..b.area.width)
            .map(|x| b[(x, y)].symbol().to_string())
            .collect();
        if let Some(i) = line.find(needle) {
            return (line[..i].chars().count() as u16, y);
        }
    }
    panic!("{needle} missing");
}

#[test]
fn cpu_bottom_moves_cpu_box_below_the_rest() {
    let mut v = view();
    let (_, top_default) = pos(&render(&v), "¹cpu");
    v.cpu_bottom = true;
    let b = render(&v);
    let (_, cpu_y) = pos(&b, "¹cpu");
    let (_, proc_y) = pos(&b, "⁴proc");
    assert_eq!(top_default, 0);
    assert!(cpu_y > proc_y, "cpu {cpu_y} should be below proc {proc_y}");
}

#[test]
fn proc_left_swaps_proc_and_the_mem_net_column() {
    let mut v = view();
    let b = render(&v);
    assert!(pos(&b, "⁴proc").0 > pos(&b, "²mem").0);
    v.proc_left = true;
    let b = render(&v);
    assert!(pos(&b, "⁴proc").0 < pos(&b, "²mem").0, "proc on the left");
}

#[test]
fn mem_below_net_reorders_left_column() {
    let mut v = view();
    let b = render(&v);
    assert!(pos(&b, "²mem").1 < pos(&b, "³net").1);
    v.mem_below_net = true;
    let b = render(&v);
    assert!(pos(&b, "²mem").1 > pos(&b, "³net").1, "net above mem");
}

#[test]
fn preset_p_flag_sets_positions() {
    let p = parse_preset_positions("cpu:1:default,proc:1:default,mem:1:tty");
    assert_eq!(p, (true, true, true));
    assert_eq!(
        parse_preset_positions("cpu:0:default,proc:0:x"),
        (false, false, false)
    );
    let mut v = view();
    v.apply_config(&Config {
        presets: "cpu:1:default,proc:1:default".into(),
        ..Config::default()
    });
    v.feed_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('p'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(v.cpu_bottom && v.proc_left && !v.mem_below_net);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

#[test]
fn options_panel_toggles_positions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let k = |v: &mut AppView, c| {
        v.feed_key(KeyEvent::new(c, KeyModifiers::NONE));
    };
    let mut v = view();
    k(&mut v, KeyCode::Char('o'));
    let text: String = render(&v).content().iter().map(|c| c.symbol()).collect();
    for label in ["Cpu bottom", "Proc left", "Mem below net"] {
        assert!(text.contains(label), "options lists {label}");
    }
    for _ in 0..6 {
        k(&mut v, KeyCode::Down);
    }
    k(&mut v, KeyCode::Enter);
    k(&mut v, KeyCode::Down);
    k(&mut v, KeyCode::Enter);
    k(&mut v, KeyCode::Down);
    k(&mut v, KeyCode::Enter);
    assert!(v.cpu_bottom && v.proc_left && v.mem_below_net);
}

#[test]
fn positions_persist_in_config() {
    let mut v = view();
    v.cpu_bottom = true;
    v.proc_left = true;
    let cfg = v.to_config(&Config::default());
    assert!(cfg.cpu_bottom && cfg.proc_left && !cfg.mem_below_net);
    let mut w = view();
    w.apply_config(&cfg);
    assert!(w.cpu_bottom && w.proc_left);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

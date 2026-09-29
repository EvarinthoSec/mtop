//! btop draws the cpu info rail (name, meters, load avg) in its own box,
//! separated from the cpu graph by a border line.
use mtop::config::Config;
use mtop::model::{CpuSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

fn rows(cfg: Config, w: u16) -> Vec<String> {
    let snap = SystemSnapshot {
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 28.8,
            per_core_percent: vec![54.1, 48.3, 41.0],
            cpu_name: Some("Apple M5".into()),
            frequency_mhz: Some(4600),
            ..CpuSnapshot::default()
        },
        load_average: [3.33, 4.62, 12.92],
        ..SystemSnapshot::default()
    };
    let mut v = AppView::new(snap, Theme::from_name("neon"));
    v.apply_config(&cfg);
    let mut t = Terminal::new(TestBackend::new(w, 24)).unwrap();
    t.draw(|f| draw_dashboard(f, &v)).unwrap();
    let b = t.backend().buffer().clone();
    mtop::ui::set_palette(mtop::ui::Palette::btop());
    (0..b.area.height)
        .map(|y| (0..b.area.width).map(|x| b[(x, y)].symbol()).collect())
        .collect()
}

fn cpu_only() -> Config {
    Config {
        boxes: "cpu".into(),
        ..Config::default()
    }
}

#[test]
fn cpu_rail_is_boxed_off_from_the_graph() {
    let r = rows(cpu_only(), 120);
    let cpu_row = r
        .iter()
        .find(|l| l.contains("CPU "))
        .expect("CPU meter row");
    // outer left, rail left, rail right, outer right.
    assert_eq!(cpu_row.matches('│').count(), 4, "{cpu_row}");
    let load = r.iter().find(|l| l.contains("Load avg")).unwrap();
    assert_eq!(load.matches('│').count(), 4, "{load}");
    // Rail box has its own rounded top and bottom edges.
    assert!(
        r.iter()
            .skip(1)
            .any(|l| l.matches('╭').count() == 1 && l.contains('╮'))
    );
    assert!(r.iter().any(|l| l.contains("M5") && l.contains("GHz")));
}

#[test]
fn rail_box_follows_square_corners() {
    let r = rows(
        Config {
            rounded_corners: false,
            ..cpu_only()
        },
        120,
    );
    assert!(!r.concat().contains('╭'));
    assert!(r.iter().skip(1).any(|l| l.contains('┌')));
}

use mtop::model::{FanSnapshot, FanStatus, FanTelemetry, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, DashboardPage, FAN_ART, draw_dashboard, fan_art_rows, fan_grid};
use ratatui::{Terminal, backend::TestBackend};

fn fans_view(n: usize) -> AppView {
    let snapshot = SystemSnapshot {
        fans: FanTelemetry {
            status: FanStatus::Available,
            fans: (1..=n)
                .map(|i| FanSnapshot {
                    name: format!("Fan {i}"),
                    rpm: 2000.0,
                    max_rpm: Some(6000.0),
                })
                .collect(),
        },
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("neon"));
    view.page = DashboardPage::Fan;
    view
}

fn render_rows(view: &AppView, w: u16, h: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| draw_dashboard(f, view)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_owned()).collect())
        .collect()
}

fn braille_rows(rows: &[String]) -> usize {
    rows.iter()
        .filter(|r| r.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)))
        .count()
}

#[test]
fn fan_grid_picks_layout_that_maximizes_fan_size() {
    // Wide window: side by side.
    assert_eq!(fan_grid(2, 200, 50), (2, 1));
    // Tall window: stacked.
    assert_eq!(fan_grid(2, 60, 80), (1, 2));
    // Four fans on a roughly 2:1 (square-in-cells) window → 2×2.
    assert_eq!(fan_grid(4, 120, 60), (2, 2));
    assert_eq!(fan_grid(1, 10, 10), (1, 1));
}

#[test]
fn each_fan_gets_its_own_titled_box() {
    let rows = render_rows(&fans_view(3), 180, 50);
    let text = rows.concat();
    for i in 1..=3 {
        assert!(
            text.contains(&format!("┤Fan {i}├")),
            "missing box for Fan {i}"
        );
    }
}

#[test]
fn fan_art_grows_with_window() {
    let small = braille_rows(&render_rows(&fans_view(1), 80, 30));
    let large = braille_rows(&render_rows(&fans_view(1), 240, 80));
    assert!(
        large > small + 20,
        "art should scale: small={small} large={large}"
    );
    assert!(large > 15, "art must not be capped at source size: {large}");
}

#[test]
fn fan_art_at_native_size_and_zero_angle_matches_source() {
    let rows = fan_art_rows(30, 15, 0.0);
    let got: Vec<String> = rows.into_iter().map(|r| r.text).collect();
    let want: Vec<String> = FAN_ART.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(got, want);
}

#[test]
fn fan_rotor_rotates_but_frame_stays_fixed() {
    let still: Vec<String> = fan_art_rows(30, 15, 0.0)
        .into_iter()
        .map(|r| r.text)
        .collect();
    let turned: Vec<String> = fan_art_rows(30, 15, std::f64::consts::FRAC_PI_4)
        .into_iter()
        .map(|r| r.text)
        .collect();
    assert_ne!(still, turned, "rotor must change when angle changes");
    // Corner screw cells (outside the ring) never move.
    for (a, b) in still.iter().zip(turned.iter()) {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        assert_eq!(a[..3], b[..3]);
        assert_eq!(a[27..], b[27..]);
    }
}

#[test]
fn fan_art_scales_to_requested_size() {
    let rows = fan_art_rows(20, 10, 1.0);
    assert_eq!(rows.len(), 10);
    assert!(rows.iter().all(|r| r.text.chars().count() == 20));
    assert!(rows.iter().all(|r| r.blade.len() == 20));
}

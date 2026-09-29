//! btop graph_symbol: braille (default), block or tty glyphs, per box.
use mtop::config::Config;
use mtop::model::{CpuSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, GraphSymbol, draw_dashboard, graph_rows};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

fn ramp() -> VecDeque<u64> {
    (0..40).map(|i| i * 100 / 39).collect()
}

fn glyphs(rows: &[String]) -> String {
    rows.concat()
}

#[test]
fn each_symbol_set_uses_its_own_glyphs() {
    let b = glyphs(&graph_rows(
        &ramp(),
        100,
        20,
        4,
        false,
        GraphSymbol::Braille,
    ));
    let k = glyphs(&graph_rows(&ramp(), 100, 20, 4, false, GraphSymbol::Block));
    let t = glyphs(&graph_rows(&ramp(), 100, 20, 4, false, GraphSymbol::Tty));
    assert!(
        b.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)),
        "braille"
    );
    assert!(k.chars().any(|c| "▗▐▖▄▟▌▙█".contains(c)), "block: {k}");
    assert!(!k.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)));
    assert!(
        t.chars().all(|c| " ░▒█".contains(c)),
        "tty only uses shades: {t}"
    );
    assert!(t.contains('█'));
}

#[test]
fn symbol_sets_share_geometry() {
    // Same data → same set of non-blank cells, only the glyph differs.
    let shape = |s| {
        graph_rows(&ramp(), 100, 20, 4, false, s)
            .iter()
            .map(|r| r.chars().map(|c| c != ' ').collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(GraphSymbol::Braille), shape(GraphSymbol::Block));
}

#[test]
fn parse_names_like_btop() {
    assert_eq!(GraphSymbol::parse("block"), Some(GraphSymbol::Block));
    assert_eq!(GraphSymbol::parse("tty"), Some(GraphSymbol::Tty));
    assert_eq!(GraphSymbol::parse("braille"), Some(GraphSymbol::Braille));
    assert_eq!(GraphSymbol::parse("default"), None, "default = inherit");
    assert_eq!(GraphSymbol::parse("bogus"), None);
}

fn cpu_render(cfg: &Config) -> String {
    let snap = |t: u64, c: f32| SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        cpu: CpuSnapshot {
            available: true,
            overall_percent: c,
            ..CpuSnapshot::default()
        },
        ..SystemSnapshot::default()
    };
    let mut v = AppView::new(snap(0, 10.0), Theme::from_name("neon"));
    v.apply_config(cfg);
    for t in 1..80 {
        v.accept_snapshot(snap(t, (t % 10) as f32 * 10.0));
    }
    let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
    term.draw(|f| draw_dashboard(f, &v)).unwrap();
    let b = term.backend().buffer();
    // cpu box rows only (top ~12 rows, left graph area)
    (1..11)
        .map(|y| {
            (1..60)
                .map(|x| b[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn per_box_symbol_overrides_global_default() {
    let braille = cpu_render(&Config::default());
    assert!(
        braille
            .chars()
            .any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
    );
    let block = cpu_render(&Config {
        graph_symbol_cpu: "block".into(),
        ..Config::default()
    });
    assert!(
        block.chars().any(|c| "▄▟▙█".contains(c)),
        "cpu box uses block"
    );
    assert!(
        !block
            .chars()
            .any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
    );
    let tty = cpu_render(&Config {
        graph_symbol: "tty".into(),
        ..Config::default()
    });
    // Global tty applies to the cpu graph: only tty shades, never braille.
    assert!(tty.contains('█'), "global tty applies to cpu");
    assert!(!tty.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)));
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

#[test]
fn options_row_cycles_global_graph_symbol() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    let k = |v: &mut AppView, c| {
        v.feed_key(KeyEvent::new(c, KeyModifiers::NONE));
    };
    k(&mut v, KeyCode::Char('o'));
    k(&mut v, KeyCode::Up); // wraps to the last row: Graph symbol
    k(&mut v, KeyCode::Right);
    assert_eq!(v.graph_symbol, GraphSymbol::Block);
    k(&mut v, KeyCode::Right);
    assert_eq!(v.graph_symbol, GraphSymbol::Tty);
    k(&mut v, KeyCode::Left);
    assert_eq!(v.graph_symbol, GraphSymbol::Block);
    assert_eq!(v.to_config(&Config::default()).graph_symbol, "block");
}

#[test]
fn preset_graph_field_sets_box_symbol_via_p() {
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    v.apply_config(&Config {
        presets: "cpu:0:block,net:0:tty".into(),
        ..Config::default()
    });
    v.feed_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('p'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(v.box_symbol("cpu"), GraphSymbol::Block);
    assert_eq!(v.box_symbol("net"), GraphSymbol::Tty);
    assert_eq!(v.box_symbol("mem"), GraphSymbol::Braille);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

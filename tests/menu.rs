//! btop main menu (Esc / m) and options panel (o / F2).
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::SystemSnapshot;
use mtop::theme::Theme;
use mtop::ui::{AppView, KeyOutcome, Menu, Palette, active, draw_dashboard, set_palette};
use ratatui::{Terminal, backend::TestBackend};
use std::time::Duration;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn view() -> AppView {
    AppView::new(SystemSnapshot::default(), Theme::from_name("neon"))
}

fn render(view: &AppView) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, view)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn esc_opens_main_menu_instead_of_quitting() {
    let mut v = view();
    assert_eq!(v.feed_key(key(KeyCode::Esc)), KeyOutcome::Continue);
    assert_eq!(v.menu, Some(Menu::Main { selected: 0 }));
    let text = render(&v);
    for item in ["Options", "Help", "Quit"] {
        assert!(text.contains(item), "main menu must list {item}");
    }
    // Esc again closes it (btop toggles).
    v.feed_key(key(KeyCode::Esc));
    assert_eq!(v.menu, None);
}

#[test]
fn m_key_also_toggles_main_menu() {
    let mut v = view();
    v.feed_key(key(KeyCode::Char('m')));
    assert!(matches!(v.menu, Some(Menu::Main { .. })));
}

#[test]
fn main_menu_quit_entry_quits() {
    let mut v = view();
    v.feed_key(key(KeyCode::Esc));
    v.feed_key(key(KeyCode::Down));
    v.feed_key(key(KeyCode::Down));
    assert_eq!(v.menu, Some(Menu::Main { selected: 2 }));
    assert_eq!(v.feed_key(key(KeyCode::Enter)), KeyOutcome::Quit);
}

#[test]
fn main_menu_help_entry_opens_help() {
    let mut v = view();
    v.feed_key(key(KeyCode::Esc));
    v.feed_key(key(KeyCode::Down));
    v.feed_key(key(KeyCode::Enter));
    assert_eq!(v.menu, None);
    assert!(v.show_help);
    // Esc closes help rather than quitting or reopening the menu.
    assert_eq!(v.feed_key(key(KeyCode::Esc)), KeyOutcome::Continue);
    assert!(!v.show_help);
    assert_eq!(v.menu, None);
}

#[test]
fn menu_navigation_wraps_like_btop() {
    let mut v = view();
    v.feed_key(key(KeyCode::Esc));
    v.feed_key(key(KeyCode::Up));
    assert_eq!(v.menu, Some(Menu::Main { selected: 2 }));
    v.feed_key(key(KeyCode::Down));
    assert_eq!(v.menu, Some(Menu::Main { selected: 0 }));
}

#[test]
fn menu_swallows_dashboard_keys() {
    let mut v = view();
    v.feed_key(key(KeyCode::Esc));
    v.feed_key(key(KeyCode::Char('e')));
    v.feed_key(key(KeyCode::Char('5')));
    assert!(
        !v.tree,
        "e must not reach the dashboard while the menu is open"
    );
    assert!(v.show_gpu);
}

#[test]
fn o_opens_options_and_lists_settings() {
    let mut v = view();
    v.feed_key(key(KeyCode::Char('o')));
    assert_eq!(v.menu, Some(Menu::Options { selected: 0 }));
    let text = render(&v);
    for label in [
        "Color theme",
        "Update ms",
        "Vim keys",
        "Per-core",
        "Show GPU",
        "Tree view",
    ] {
        assert!(text.contains(label), "options must list {label}");
    }
}

#[test]
fn f2_and_main_menu_entry_open_options() {
    let mut v = view();
    v.feed_key(key(KeyCode::F(2)));
    assert!(matches!(v.menu, Some(Menu::Options { .. })));
    let mut v = view();
    v.feed_key(key(KeyCode::Esc));
    v.feed_key(key(KeyCode::Enter));
    assert!(matches!(v.menu, Some(Menu::Options { .. })));
}

#[test]
fn options_theme_cycles_live_palette() {
    let mut v = view();
    v.feed_key(key(KeyCode::Char('o')));
    // Row 0 = Color theme.
    v.feed_key(key(KeyCode::Right));
    assert_eq!(v.theme_name, "amber");
    assert_eq!(active(), Palette::amber());
    v.feed_key(key(KeyCode::Right));
    assert_eq!(v.theme_name, "mono");
    v.feed_key(key(KeyCode::Right));
    assert_eq!(v.theme_name, "neon");
    assert_eq!(active(), Palette::btop());
    v.feed_key(key(KeyCode::Left));
    assert_eq!(v.theme_name, "mono");
    set_palette(Palette::btop());
}

#[test]
fn options_update_ms_rewires_collector() {
    let mut v = view();
    v.feed_key(key(KeyCode::Char('o')));
    v.feed_key(key(KeyCode::Down)); // Update ms
    assert_eq!(v.feed_key(key(KeyCode::Right)), KeyOutcome::WireRefresh);
    assert_eq!(v.refresh_interval, Duration::from_millis(1100));
}

#[test]
fn options_toggles_flip_view_flags() {
    let mut v = view();
    v.feed_key(key(KeyCode::Char('o')));
    for _ in 0..2 {
        v.feed_key(key(KeyCode::Down));
    }
    v.feed_key(key(KeyCode::Enter)); // Vim keys (default On → Off)
    assert!(!v.vim_keys);
    v.feed_key(key(KeyCode::Down));
    v.feed_key(key(KeyCode::Right)); // Per-core
    assert!(!v.show_cores);
    v.feed_key(key(KeyCode::Down));
    v.feed_key(key(KeyCode::Char(' '))); // Show GPU
    assert!(!v.show_gpu);
    v.feed_key(key(KeyCode::Down));
    v.feed_key(key(KeyCode::Enter)); // Tree view
    assert!(v.tree);
    v.feed_key(key(KeyCode::Esc));
    assert_eq!(v.menu, None);
}

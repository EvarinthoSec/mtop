//! btop mouse: click a proc header to sort (again = reverse), click rows to
//! select, wheel scrolls, clicks in overlays are ignored.
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use mtop::config::ProcessSort;
use mtop::model::{ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, KeyOutcome, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn view() -> AppView {
    let snap = SystemSnapshot {
        processes: (1..=5)
            .map(|i| ProcessSnapshot {
                pid: i,
                name: format!("p{i}"),
                cpu_percent: i as f32,
                memory_bytes: (10 - i as u64) << 20,
                ..ProcessSnapshot::default()
            })
            .collect(),
        ..SystemSnapshot::default()
    };
    AppView::new(snap, Theme::from_name("neon"))
}

fn draw(v: &AppView) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(140, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend().buffer().clone()
}

fn find(b: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..b.area.height {
        let line: String = (0..b.area.width)
            .map(|x| b[(x, y)].symbol().to_string())
            .collect();
        if let Some(byte) = line.find(needle) {
            return (line[..byte].chars().count() as u16, y);
        }
    }
    panic!("{needle} not on screen");
}

fn click(v: &mut AppView, (x, y): (u16, u16)) {
    v.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn clicking_a_header_sorts_by_that_column_and_again_reverses() {
    let mut v = view();
    let b = draw(&v);
    click(&mut v, find(&b, "MemB"));
    assert_eq!(v.sort, ProcessSort::Memory);
    assert!(!v.sort_reverse);
    let b = draw(&v);
    click(&mut v, find(&b, "MemB"));
    assert!(v.sort_reverse, "second click on the active column reverses");
    let b = draw(&v);
    click(&mut v, find(&b, "Pid:"));
    assert_eq!(v.sort, ProcessSort::Pid);
    assert!(!v.sort_reverse, "new column resets direction");
    let b = draw(&v);
    click(&mut v, find(&b, "Program:"));
    assert_eq!(v.sort, ProcessSort::Name);
}

#[test]
fn clicking_a_row_still_selects_it() {
    let mut v = view();
    let b = draw(&v);
    let (x, y) = find(&b, "p3");
    click(&mut v, (x, y));
    assert_eq!(v.selected_pid(), Some(3));
}

#[test]
fn clicking_box_toggle_labels_flips_them() {
    let mut v = view();
    let b = draw(&v);
    click(&mut v, find(&b, "tree□"));
    assert!(v.tree, "click tree□ in proc title");
    let b = draw(&v);
    click(&mut v, find(&b, "reverse□"));
    assert!(v.sort_reverse);
}

#[test]
fn wheel_scrolls_and_clicks_are_ignored_under_overlays() {
    let mut v = view();
    v.feed_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(v.selected_process, 1);
    let b = draw(&v);
    let hdr = find(&b, "MemB");
    v.menu = Some(mtop::ui::Menu::Main { selected: 0 });
    click(&mut v, hdr);
    assert_eq!(v.sort, ProcessSort::Cpu, "menu open: header click ignored");
}

#[test]
fn main_menu_mouse_wheel_moves_selection_and_click_activates_row() {
    let mut v = view();
    v.feed_key(crossterm::event::KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    ));
    let b = draw(&v);
    let event = v
        .menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        )
        .expect("wheel should map to a menu navigation key");
    assert_eq!(event.code, KeyCode::Down);
    v.feed_key(event);
    assert_eq!(v.menu, Some(mtop::ui::Menu::Main { selected: 1 }));

    let b = draw(&v);
    let (x, y) = find(&b, "Help");
    let event = v
        .menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        )
        .expect("clicking a main-menu row should activate it");
    assert_eq!(event.code, KeyCode::Enter);
    assert_eq!(v.menu, Some(mtop::ui::Menu::Main { selected: 1 }));
    v.feed_key(event);
    assert_eq!(v.menu, None);
    assert!(v.show_help);
}

#[test]
fn options_mouse_selects_rows_adjusts_values_and_wheel_navigates() {
    let mut v = view();
    v.feed_key(crossterm::event::KeyEvent::new(
        KeyCode::Char('o'),
        KeyModifiers::NONE,
    ));
    let b = draw(&v);
    let (x, y) = find(&b, "Update ms");
    assert_eq!(
        v.menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x + 5,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        ),
        None,
        "clicking the row label selects without changing its value"
    );
    assert_eq!(v.menu, Some(mtop::ui::Menu::Options { selected: 1 }));
    assert_eq!(v.refresh_interval, std::time::Duration::from_millis(1000));

    let b = draw(&v);
    let (x, y) = find(&b, "Update ms");
    let event = v
        .menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x + 27,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        )
        .expect("clicking the right half of the value should adjust it");
    assert_eq!(event.code, KeyCode::Right);
    assert_eq!(v.feed_key(event), KeyOutcome::WireRefresh);
    assert_eq!(v.menu, Some(mtop::ui::Menu::Options { selected: 1 }));
    assert_eq!(v.refresh_interval, std::time::Duration::from_millis(1100));

    let b = draw(&v);
    let (x, y) = find(&b, "Update ms");
    let event = v
        .menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x + 22,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        )
        .expect("clicking the left half of the value should adjust it");
    assert_eq!(event.code, KeyCode::Left);
    assert_eq!(v.feed_key(event), KeyOutcome::WireRefresh);
    assert_eq!(v.refresh_interval, std::time::Duration::from_millis(1000));

    let b = draw(&v);
    let event = v
        .menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        )
        .expect("wheel should move the options selection");
    assert_eq!(event.code, KeyCode::Down);
    v.feed_key(event);
    assert_eq!(v.menu, Some(mtop::ui::Menu::Options { selected: 2 }));
}

#[test]
fn clicking_outside_a_menu_closes_it() {
    let mut v = view();
    v.feed_key(crossterm::event::KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    ));
    let b = draw(&v);
    let event = v
        .menu_mouse_key(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            b.area,
        )
        .expect("outside click should map to Escape");
    assert_eq!(event.code, KeyCode::Esc);
    v.feed_key(event);
    assert_eq!(v.menu, None);
}

//! btop `N`: select a new nice value for the selected process.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{ProcessSnapshot, SystemSnapshot};
use mtop::process_control::{ProcessController, RecordingController};
use mtop::theme::Theme;
use mtop::ui::{AppView, KeyOutcome, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

fn key(v: &mut AppView, code: KeyCode) -> KeyOutcome {
    v.feed_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn view() -> AppView {
    let snap = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 4242,
            name: "worker".into(),
            ..ProcessSnapshot::default()
        }],
        ..SystemSnapshot::default()
    };
    AppView::new(snap, Theme::from_name("neon"))
}

fn render(v: &AppView) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn shift_n_opens_nice_picker_and_enter_dispatches() {
    let mut v = view();
    key(&mut v, KeyCode::Char('N'));
    assert_eq!(v.nice_picker.as_deref(), Some(""));
    assert!(render(&v).contains("nice"), "picker overlay visible");
    for c in "10".chars() {
        key(&mut v, KeyCode::Char(c));
    }
    assert!(render(&v).contains("10"));
    assert_eq!(key(&mut v, KeyCode::Enter), KeyOutcome::DispatchRenice);
    let ctl = RecordingController::default();
    v.confirm_renice(&ctl);
    assert_eq!(ctl.reniced(), vec![(4242, 10)]);
    assert!(v.nice_picker.is_none());
}

#[test]
fn negative_values_and_range_are_validated() {
    let mut v = view();
    key(&mut v, KeyCode::Char('N'));
    for c in "-5".chars() {
        key(&mut v, KeyCode::Char(c));
    }
    assert_eq!(key(&mut v, KeyCode::Enter), KeyOutcome::DispatchRenice);
    assert_eq!(v.pending_nice, Some((4242, -5)));

    let mut v = view();
    key(&mut v, KeyCode::Char('N'));
    for c in "25".chars() {
        key(&mut v, KeyCode::Char(c));
    }
    assert_eq!(
        key(&mut v, KeyCode::Enter),
        KeyOutcome::Continue,
        "out of range cancels"
    );
    assert!(v.pending_nice.is_none());
}

#[test]
fn picker_ignores_letters_and_esc_cancels() {
    let mut v = view();
    key(&mut v, KeyCode::Char('N'));
    key(&mut v, KeyCode::Char('q'));
    key(&mut v, KeyCode::Char('e'));
    assert_eq!(v.nice_picker.as_deref(), Some(""), "letters are ignored");
    assert!(!v.tree, "e must not reach the dashboard");
    key(&mut v, KeyCode::Esc);
    assert!(v.nice_picker.is_none());
    assert!(v.menu.is_none(), "Esc closes picker, not opens menu");
}

struct Failing;
impl ProcessController for Failing {
    fn send(
        &self,
        _: u32,
        _: mtop::process_control::Signal,
    ) -> Result<(), mtop::process_control::SignalError> {
        Err(mtop::process_control::SignalError)
    }
    fn renice(&self, _: u32, _: i32) -> Result<(), mtop::process_control::SignalError> {
        Err(mtop::process_control::SignalError)
    }
}

#[test]
fn failed_renice_is_reported_in_status_line() {
    let mut v = view();
    key(&mut v, KeyCode::Char('N'));
    key(&mut v, KeyCode::Char('-'));
    key(&mut v, KeyCode::Char('9'));
    key(&mut v, KeyCode::Enter);
    v.confirm_renice(&Failing);
    assert!(render(&v).contains("renice 4242 failed"));
}

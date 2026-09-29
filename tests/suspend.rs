//! btop ctrl+z: sleep program and put in background.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::SystemSnapshot;
use mtop::theme::Theme;
use mtop::ui::{AppView, KeyOutcome};

#[test]
fn ctrl_z_requests_suspend_and_plain_z_does_not() {
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    let out = v.feed_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert_eq!(out, KeyOutcome::Suspend);
    let out = v.feed_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert_ne!(out, KeyOutcome::Suspend, "plain z is net totals reset");
}

#[test]
fn ctrl_z_works_even_inside_overlays() {
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    v.signal_picker = Some(String::new());
    let out = v.feed_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert_eq!(out, KeyOutcome::Suspend);
}

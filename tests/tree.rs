//! btop tree view: collapse/expand selected (space, +/-), all (E), glyphs.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};

fn key(v: &mut AppView, code: KeyCode) {
    v.feed_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn p(pid: u32, ppid: Option<u32>, name: &str) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        parent_pid: ppid,
        name: name.into(),
        ..ProcessSnapshot::default()
    }
}

/// launchd(1) ─ sshd(10) ─ zsh(11) ─ vim(12)
///            └ cron(20)
fn tree_view() -> AppView {
    let snap = SystemSnapshot {
        processes: vec![
            p(1, None, "launchd"),
            p(10, Some(1), "sshd"),
            p(11, Some(10), "zsh"),
            p(12, Some(11), "vim"),
            p(20, Some(1), "cron"),
        ],
        ..SystemSnapshot::default()
    };
    let mut v = AppView::new(snap, Theme::from_name("neon"));
    key(&mut v, KeyCode::Char('e'));
    v
}

fn names(v: &AppView) -> Vec<String> {
    v.display_indices()
        .into_iter()
        .map(|i| v.snapshot.processes[i].name.clone())
        .collect()
}

fn render(v: &AppView) -> String {
    let mut t = Terminal::new(TestBackend::new(150, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn space_collapses_and_expands_selected_subtree() {
    let mut v = tree_view();
    assert_eq!(names(&v), ["launchd", "sshd", "zsh", "vim", "cron"]);
    key(&mut v, KeyCode::Down); // sshd
    key(&mut v, KeyCode::Char(' '));
    assert_eq!(
        names(&v),
        ["launchd", "sshd", "cron"],
        "sshd children hidden"
    );
    assert!(render(&v).contains("[+]"), "collapsed node shows [+]");
    key(&mut v, KeyCode::Char(' '));
    assert_eq!(names(&v).len(), 5);
    assert!(render(&v).contains("[-]"), "expanded parent shows [-]");
}

#[test]
fn plus_expands_minus_collapses() {
    let mut v = tree_view();
    key(&mut v, KeyCode::Char('-')); // launchd
    assert_eq!(names(&v), ["launchd"]);
    key(&mut v, KeyCode::Char('-'));
    assert_eq!(names(&v), ["launchd"], "minus is idempotent");
    key(&mut v, KeyCode::Char('+'));
    assert_eq!(names(&v).len(), 5);
}

#[test]
fn leaf_cannot_be_collapsed() {
    let mut v = tree_view();
    for _ in 0..4 {
        key(&mut v, KeyCode::Down); // cron (leaf)
    }
    key(&mut v, KeyCode::Char(' '));
    assert!(v.collapsed.is_empty());
}

#[test]
fn shift_e_collapses_all_then_expands_all() {
    let mut v = tree_view();
    key(&mut v, KeyCode::Char('E'));
    assert_eq!(names(&v), ["launchd"], "only roots remain");
    key(&mut v, KeyCode::Char('E'));
    assert_eq!(names(&v).len(), 5);
}

#[test]
fn space_and_plus_minus_keep_normal_meaning_outside_tree() {
    let mut v = tree_view();
    key(&mut v, KeyCode::Char('e')); // tree off
    key(&mut v, KeyCode::Char(' '));
    assert!(v.paused, "space pauses in list view");
    let before = v.refresh_interval;
    key(&mut v, KeyCode::Char('+'));
    assert!(
        v.refresh_interval > before,
        "+ adjusts interval in list view"
    );
}

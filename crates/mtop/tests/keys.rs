//! Keymap parity with btop (src/btop_input.cpp + help_text in btop_menu.cpp).
//! btop has two layouts: default, and `vim_keys` (h/j/k/l/g/G navigate, so
//! help moves to H and kill to K). mtop defaults to vim_keys ON to keep its
//! established j/k navigation; the options menu switches to btop defaults.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use mtop::app::Action;
use mtop::ui::key_to_action_with;

fn k(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ch(c: char) -> KeyEvent {
    k(KeyCode::Char(c))
}
fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn shared_btop_bindings_in_both_modes() {
    for vim in [false, true] {
        let m = |e| key_to_action_with(e, vim);
        assert_eq!(m(ch('q')), Some(Action::Quit), "vim={vim}");
        assert_eq!(m(ctrl('c')), Some(Action::Quit), "ctrl+c quits (vim={vim})");
        assert_eq!(m(k(KeyCode::Esc)), Some(Action::OpenMenu));
        assert_eq!(m(ch('m')), Some(Action::OpenMenu));
        assert_eq!(m(ch('o')), Some(Action::OpenOptions));
        assert_eq!(m(k(KeyCode::F(2))), Some(Action::OpenOptions));
        assert_eq!(m(k(KeyCode::F(1))), Some(Action::ToggleHelp));
        assert_eq!(m(ch('?')), Some(Action::ToggleHelp));
        assert_eq!(m(ch('f')), Some(Action::ToggleFilter), "btop: f, / filter");
        assert_eq!(m(ch('/')), Some(Action::ToggleFilter));
        assert_eq!(m(k(KeyCode::Delete)), Some(Action::FilterClear));
        assert_eq!(
            m(ch('r')),
            Some(Action::ToggleReverse),
            "btop: r reverses sort"
        );
        assert_eq!(m(ch('e')), Some(Action::ToggleTree));
        assert_eq!(m(ch('t')), Some(Action::RequestTerminate));
        assert_eq!(m(ch('s')), Some(Action::SignalPicker));
        assert_eq!(m(ch('5')), Some(Action::ToggleBox(5)));
        assert_eq!(m(ch('+')), Some(Action::IncreaseInterval));
        assert_eq!(m(ch('-')), Some(Action::DecreaseInterval));
        assert_eq!(m(k(KeyCode::Up)), Some(Action::SelectPrevious));
        assert_eq!(m(k(KeyCode::Down)), Some(Action::SelectNext));
        assert_eq!(m(k(KeyCode::PageUp)), Some(Action::PageUp));
        assert_eq!(m(k(KeyCode::PageDown)), Some(Action::PageDown));
        assert_eq!(m(k(KeyCode::Home)), Some(Action::SelectFirst));
        assert_eq!(m(k(KeyCode::End)), Some(Action::SelectLast));
        assert_eq!(m(k(KeyCode::Left)), Some(Action::SortPrev));
        assert_eq!(m(k(KeyCode::Right)), Some(Action::SortNext));
        assert_eq!(m(k(KeyCode::Enter)), Some(Action::ToggleDetail));
        assert_eq!(m(k(KeyCode::F(5))), Some(Action::RefreshNow));
        assert_eq!(
            m(ctrl('r')),
            None,
            "ctrl+r is a reload outcome, not an action"
        );
        assert_eq!(
            m(ch('u')),
            Some(Action::ToggleProcPause),
            "btop: u pauses proc list"
        );
    }
}

#[test]
fn btop_default_layout_kill_on_k_help_on_h() {
    let m = |e| key_to_action_with(e, false);
    assert_eq!(m(ch('k')), Some(Action::RequestKill));
    assert_eq!(m(ch('h')), Some(Action::ToggleHelp));
    // No vim navigation in the default layout.
    assert_eq!(m(ch('j')), None);
    assert_eq!(m(ch('g')), None);
    assert_eq!(m(ch('G')), None);
}

#[test]
fn btop_vim_layout_moves_help_and_kill_to_shift() {
    let m = |e| key_to_action_with(e, true);
    assert_eq!(m(ch('j')), Some(Action::SelectNext));
    assert_eq!(m(ch('k')), Some(Action::SelectPrevious));
    assert_eq!(m(ch('g')), Some(Action::SelectFirst));
    assert_eq!(m(ch('G')), Some(Action::SelectLast));
    assert_eq!(m(ch('h')), Some(Action::SortPrev));
    assert_eq!(m(ch('l')), Some(Action::SortNext));
    assert_eq!(m(ch('K')), Some(Action::RequestKill));
    assert_eq!(m(ch('H')), Some(Action::ToggleHelp));
}

#[test]
fn unbound_letters_do_nothing() {
    // Unknown keys must be ignored, not turned into stray actions.
    for vim in [false, true] {
        assert_eq!(key_to_action_with(ch('x'), vim), None);
        assert_eq!(key_to_action_with(ch('w'), vim), None);
    }
}

#[test]
fn release_key_events_are_ignored() {
    use mtop::model::SystemSnapshot;
    use mtop::theme::Theme;
    use mtop::ui::{AppView, KeyOutcome};

    let mut view = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('5'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );

    assert_eq!(view.feed_key(release), KeyOutcome::Continue);
    assert!(view.show_gpu, "release must not dispatch the toggle");

    view.feed_key(ch('5'));
    assert!(!view.show_gpu, "the following press toggles exactly once");
}

#[test]
fn view_follows_its_vim_keys_setting() {
    use mtop::model::{ProcessSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::AppView;
    let p = |pid| ProcessSnapshot {
        pid,
        name: format!("p{pid}"),
        ..ProcessSnapshot::default()
    };
    let snap = SystemSnapshot {
        processes: vec![p(1), p(2), p(3)],
        ..SystemSnapshot::default()
    };
    let mut v = AppView::new(snap, Theme::from_name("neon"));
    assert!(v.vim_keys, "mtop keeps j/k navigation by default");
    v.feed_key(ch('j'));
    assert_eq!(v.selected_process, 1);
    v.vim_keys = false;
    v.feed_key(ch('k'));
    assert!(
        v.pending_signal.is_some(),
        "btop default: k stages SIGKILL confirm"
    );
}

fn help_text(vim: bool) -> String {
    use mtop::model::SystemSnapshot;
    use mtop::theme::Theme;
    use mtop::ui::{AppView, draw_dashboard};
    use ratatui::{Terminal, backend::TestBackend};
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    v.vim_keys = vim;
    v.show_help = true;
    let mut t = Terminal::new(TestBackend::new(120, 44)).unwrap();
    t.draw(|f| draw_dashboard(f, &v)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn help_overlay_lists_btop_keys_for_active_layout() {
    let vim = help_text(true);
    for needle in [
        "Esc, m",
        "F2, o",
        "Toggles main menu",
        "GPU box",
        "Kill selected",
        "f, /",
    ] {
        assert!(vim.contains(needle), "help (vim) missing {needle:?}");
    }
    assert!(
        vim.contains("K ") && vim.contains("H "),
        "vim layout shows K kill / H help"
    );
    let default = help_text(false);
    assert!(
        default.contains("k ") || default.contains("Selected k"),
        "default layout shows k kill"
    );
    assert!(
        !default.contains("j, k"),
        "default layout must not advertise vim nav"
    );
}

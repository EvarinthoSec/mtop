//! btop ctrl+r: reload the config file from disk.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::config::Config;
use mtop::model::SystemSnapshot;
use mtop::theme::Theme;
use mtop::ui::{AppView, KeyOutcome};

#[test]
fn ctrl_r_requests_config_reload() {
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    let out = v.feed_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(out, KeyOutcome::ReloadConfig);
}

#[test]
fn reload_applies_file_contents_and_reports_status() {
    let dir = std::env::temp_dir().join(format!("mtop-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    Config {
        tree: true,
        vim_keys: false,
        boxes: "cpu proc".into(),
        ..Config::default()
    }
    .save(&path)
    .unwrap();
    v.reload_config(&path);
    assert!(v.tree && !v.vim_keys);
    assert!(v.show_cpu && v.show_proc && !v.show_mem && !v.show_net);
    assert_eq!(v.status_msg.as_deref(), Some("config reloaded"));

    std::fs::write(&path, "interval_ms = \"oops\"").unwrap();
    v.reload_config(&path);
    assert!(v.tree, "bad file keeps current settings");
    assert!(v.status_msg.as_deref().unwrap().starts_with("config error"));
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

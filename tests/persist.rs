//! btop writes its config when options change. mtop persists the options
//! panel + view toggles to TOML and restores them on start.
use mtop::config::{Config, ThemeName};
use mtop::model::SystemSnapshot;
use mtop::theme::Theme;
use mtop::ui::AppView;
use std::time::Duration;

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mtop-persist-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("config.toml")
}

#[test]
fn config_round_trips_through_disk_including_new_view_fields() {
    let path = tmp("roundtrip");
    let cfg = Config {
        interval_ms: 1500,
        theme: ThemeName::Amber,
        show_gpu: false,
        vim_keys: false,
        show_cores: false,
        tree: true,
        proc_per_core: false,
        net_sync: true,
        net_auto: false,
        boxes: "cpu mem proc".into(),
        ..Config::default()
    };
    cfg.save(&path).unwrap();
    assert_eq!(Config::load(Some(&path)).unwrap(), cfg);
}

#[test]
fn old_config_files_still_load_with_btop_defaults() {
    let path = tmp("legacy");
    std::fs::write(&path, "interval_ms = 2000\ntheme = \"mono\"\n").unwrap();
    let cfg = Config::load(Some(&path)).unwrap();
    assert_eq!(cfg.interval_ms, 2000);
    assert!(cfg.vim_keys && cfg.show_cores && cfg.proc_per_core && cfg.net_auto);
    assert_eq!(cfg.boxes, "cpu mem net proc gpu");
}

#[test]
fn view_applies_and_exports_config() {
    let cfg = Config {
        interval_ms: 700,
        theme: ThemeName::Mono,
        vim_keys: false,
        tree: true,
        boxes: "mem proc".into(),
        ..Config::default()
    };
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    v.apply_config(&cfg);
    assert_eq!(v.refresh_interval, Duration::from_millis(700));
    assert_eq!(v.theme_name, "mono");
    assert!(!v.vim_keys && v.tree);
    assert!(!v.show_cpu && v.show_mem && !v.show_net && v.show_proc && !v.show_gpu);
    assert_eq!(
        v.to_config(&Config::default()),
        Config {
            process_limit: Config::default().process_limit,
            ..cfg
        }
    );
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

#[test]
fn default_config_path_is_under_the_user_config_dir() {
    let p = Config::default_path().expect("config dir available");
    assert!(
        p.ends_with("mtop/config.toml") || p.ends_with("mtop/mtop.toml"),
        "{}",
        p.display()
    );
}

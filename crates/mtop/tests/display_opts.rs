//! btop display options that map 1:1 to config keys.
use mtop::config::Config;
use mtop::model::{BatterySnapshot, BatteryState, CpuSnapshot, NetworkSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, Menu, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

fn snap() -> SystemSnapshot {
    SystemSnapshot {
        uptime: std::time::Duration::from_secs(3 * 3600),
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 40.0,
            per_core_percent: vec![30.0, 50.0],
            performance_core_count: None,
            efficiency_core_count: None,
            per_core_temperature: vec![55.0, 60.0],
            temperature_celsius: Some(61.0),
            frequency_mhz: Some(3200),
            cpu_name: Some("Apple M5".into()),
        },
        networks: vec![
            NetworkSnapshot {
                interface: "en0".into(),
                received_bytes_per_second: 1,
                transmitted_bytes_per_second: 1,
                received_bytes_total: None,
                transmitted_bytes_total: None,
            },
            NetworkSnapshot {
                interface: "utun3".into(),
                received_bytes_per_second: 1,
                transmitted_bytes_per_second: 1,
                received_bytes_total: None,
                transmitted_bytes_total: None,
            },
        ],
        battery: Some(BatterySnapshot {
            percent: 70.0,
            state: BatteryState::Discharging,
            seconds_left: None,
        }),
        ..SystemSnapshot::default()
    }
}

fn render_with(cfg: Config) -> (String, Buffer) {
    let mut v = AppView::new(snap(), Theme::from_name("neon"));
    v.apply_config(&cfg);
    let mut t = Terminal::new(TestBackend::new(150, 44)).unwrap();
    t.draw(|f| draw_dashboard(f, &v)).unwrap();
    let b = t.backend().buffer().clone();
    mtop::ui::set_palette(mtop::ui::Palette::btop());
    (b.content().iter().map(|c| c.symbol()).collect(), b)
}

#[test]
fn rounded_corners_toggle_border_style() {
    let (t, _) = render_with(Config::default());
    assert!(t.contains('╭'));
    let (t, _) = render_with(Config {
        rounded_corners: false,
        ..Config::default()
    });
    assert!(!t.contains('╭') && t.contains('┌'), "square corners");
}

#[test]
fn theme_background_off_leaves_terminal_background() {
    let (_, b) = render_with(Config::default());
    assert_eq!(b[(70, 20)].bg, Color::Rgb(0, 0, 0));
    let (_, b) = render_with(Config {
        theme_background: false,
        ..Config::default()
    });
    assert_eq!(b[(70, 20)].bg, Color::Reset, "transparent background");
}

#[test]
fn cpu_box_info_toggles() {
    let (t, _) = render_with(Config::default());
    assert!(t.contains("up 3h") && t.contains("BAT") && t.contains("GHz") && t.contains("°"));
    assert!(
        t.contains("55°C") || t.contains("60°C"),
        "Celsius unit is explicit"
    );
    let (t, _) = render_with(Config {
        show_uptime: false,
        show_battery: false,
        show_cpu_freq: false,
        check_temp: false,
        ..Config::default()
    });
    assert!(!t.contains("up 3h"), "uptime hidden");
    assert!(!t.contains("BAT"), "battery hidden");
    assert!(!t.contains("GHz"), "freq hidden");
    assert!(!t.contains('°'), "temps hidden");
}

#[test]
fn custom_cpu_name_and_fahrenheit() {
    let (t, _) = render_with(Config {
        custom_cpu_name: "My Mac".into(),
        temp_scale: "fahrenheit".into(),
        ..Config::default()
    });
    assert!(t.contains("My Mac"));
    assert!(
        t.contains("131°F") || t.contains("140°F"),
        "55/60 °C → 131/140 °F"
    );
}

#[test]
fn kelvin_temperature_uses_k_unit_suffix() {
    let (t, _) = render_with(Config {
        temp_scale: "kelvin".into(),
        ..Config::default()
    });
    assert!(t.contains("328K") || t.contains("333K"));
}

#[test]
fn net_iface_picks_starting_interface() {
    let (t, _) = render_with(Config {
        net_iface: "utun3".into(),
        ..Config::default()
    });
    assert!(t.contains("³net utun3"));
}

#[test]
fn clock_format_is_strftime_like_btop() {
    let (_, b) = render_with(Config {
        clock_format: "%H:%M".into(),
        ..Config::default()
    });
    let top: String = (0..150).map(|x| b[(x, 1)].symbol()).collect();
    // "HH:MM" without seconds: find "┤¹cpu├  dd:dd  " pattern
    let after = top.split("¹cpu├").nth(1).unwrap().trim_start();
    let clock: String = after.chars().take(6).collect();
    assert!(
        clock.chars().nth(2) == Some(':') && clock.chars().nth(5) == Some(' '),
        "{top}"
    );
    let (_, b) = render_with(Config {
        clock_format: String::new(),
        ..Config::default()
    });
    let top: String = (0..150).map(|x| b[(x, 1)].symbol()).collect();
    assert!(
        !top.split("¹cpu├")
            .nth(1)
            .unwrap()
            .trim_start()
            .starts_with(char::is_numeric)
    );
}

#[test]
fn new_keys_round_trip_and_old_files_get_btop_defaults() {
    let cfg = Config {
        rounded_corners: false,
        theme_background: false,
        show_uptime: false,
        show_battery: false,
        check_temp: false,
        show_cpu_freq: false,
        disable_mouse: true,
        save_config_on_exit: false,
        custom_cpu_name: "x".into(),
        temp_scale: "kelvin".into(),
        net_iface: "en1".into(),
        clock_format: "%X".into(),
        ..Config::default()
    };
    let dir = std::env::temp_dir().join(format!("mtop-disp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("c.toml");
    cfg.save(&p).unwrap();
    assert_eq!(Config::load(Some(&p)).unwrap(), cfg);
    std::fs::write(&p, "interval_ms = 1000\n").unwrap();
    let d = Config::load(Some(&p)).unwrap();
    assert!(d.rounded_corners && d.theme_background && d.show_uptime && d.save_config_on_exit);
    assert!(!d.disable_mouse);
    assert_eq!(d.temp_scale, "celsius");
}

#[test]
fn temperature_unit_can_be_changed_from_options() {
    let mut view = AppView::new(snap(), Theme::from_name("neon"));
    view.menu = Some(Menu::Options { selected: 10 });
    view.feed_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(view.temp_scale, "fahrenheit");
    assert_eq!(view.option_value(10), "°F");
}

#[test]
fn disable_mouse_ignores_clicks() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    let mut v = AppView::new(snap(), Theme::from_name("neon"));
    v.apply_config(&Config {
        disable_mouse: true,
        ..Config::default()
    });
    v.feed_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(v.selected_process, 0);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

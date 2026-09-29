//! Load btop `.theme` files (theme[key]="#rrggbb") into a Palette.
use mtop::ui::{Palette, parse_btop_theme};
use ratatui::style::Color;

const SAMPLE: &str = r##"
# comment line
theme[main_bg]="#101418"
theme[main_fg]="#e0e0e0"
theme[title]="#ffffff"
theme[hi_fg]="#ff5577"
theme[selected_bg]="#334455"
theme[selected_fg]="#fafafa"
theme[inactive_fg]="#555555"
theme[graph_text]="#888888"
theme[meter_bg]="#222222"
theme[proc_misc]="#00ffaa"
theme[cpu_box]="#123456"
theme[mem_box]="#234567"
theme[net_box]="#345678"
theme[proc_box]="#456789"
theme[cpu_start]="#00ff00"
theme[cpu_mid]="#ffff00"
theme[cpu_end]="#ff0000"
theme[used_end]="#ff00ff"
theme[available_end]="#00ffff"
theme[download_end]="#0000ff"
theme[upload_end]="#ff8800"
"##;

#[test]
fn parses_hex_keys_into_palette_fields() {
    let p = parse_btop_theme(SAMPLE, Palette::btop());
    assert_eq!(p.main_bg, Color::Rgb(0x10, 0x14, 0x18));
    assert_eq!(p.cpu_box, Color::Rgb(0x12, 0x34, 0x56));
    assert_eq!(p.cpu_start, Color::Rgb(0, 0xff, 0));
    assert_eq!(p.cpu_mid, Color::Rgb(0xff, 0xff, 0));
    assert_eq!(p.cpu_end, Color::Rgb(0xff, 0, 0));
    assert_eq!(p.hi_fg, Color::Rgb(0xff, 0x55, 0x77));
    assert_eq!(p.inactive, Color::Rgb(0x55, 0x55, 0x55));
    assert_eq!(p.avail_end, Color::Rgb(0, 0xff, 0xff));
    assert_eq!(p.down_end, Color::Rgb(0, 0, 0xff));
    assert_eq!(p.up_end, Color::Rgb(0xff, 0x88, 0));
    assert_eq!(p.selected_bg, Color::Rgb(0x33, 0x44, 0x55));
}

#[test]
fn missing_keys_fall_back_to_base_palette() {
    let base = Palette::btop();
    let p = parse_btop_theme("theme[cpu_box]=\"#010203\"\n", base);
    assert_eq!(p.cpu_box, Color::Rgb(1, 2, 3));
    assert_eq!(p.mem_box, base.mem_box);
    assert_eq!(p.main_bg, base.main_bg);
}

#[test]
fn btop_short_forms_and_grayscale_are_supported() {
    // btop accepts "#rgb"-less forms: 2-digit grayscale "#80" and "r g b".
    let p = parse_btop_theme(
        "theme[cpu_box]=\"#80\"\ntheme[mem_box]=\"12 34 56\"\ntheme[net_box]=\"\"\n",
        Palette::btop(),
    );
    assert_eq!(p.cpu_box, Color::Rgb(0x80, 0x80, 0x80));
    assert_eq!(p.mem_box, Color::Rgb(12, 34, 56));
    assert_eq!(p.net_box, Palette::btop().net_box, "empty value keeps base");
}

#[test]
fn garbage_lines_are_ignored() {
    let p = parse_btop_theme(
        "nonsense\ntheme[cpu_box=#zz\ntheme[unknown]=\"#ffffff\"\n",
        Palette::btop(),
    );
    assert_eq!(p, Palette::btop());
}

#[test]
fn theme_dir_lists_user_themes() {
    let dir = std::env::temp_dir().join(format!("mtop-themes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("zeta.theme"), SAMPLE).unwrap();
    std::fs::write(dir.join("alpha.theme"), SAMPLE).unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    assert_eq!(mtop::ui::list_theme_files(&dir), vec!["alpha", "zeta"]);
    let p = mtop::ui::load_theme_file(&dir, "zeta").unwrap();
    assert_eq!(p.cpu_box, Color::Rgb(0x12, 0x34, 0x56));
    assert!(
        mtop::ui::load_theme_file(&dir, "../etc/passwd").is_none(),
        "no path traversal"
    );
}

#[test]
fn user_theme_round_trips_through_view_config() {
    use mtop::config::Config;
    use mtop::model::SystemSnapshot;
    use mtop::theme::Theme;
    use mtop::ui::{AppView, active, set_palette};
    let dir = std::env::temp_dir().join(format!("mtop-themes-rt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("dracula.theme"), "theme[cpu_box]=\"#bd93f9\"\n").unwrap();
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    v.theme_dir = Some(dir);
    assert_eq!(
        v.available_themes(),
        vec!["neon", "amber", "mono", "dracula"]
    );
    v.apply_config(&Config {
        color_theme: Some("dracula".into()),
        ..Config::default()
    });
    assert_eq!(v.theme_name, "dracula");
    assert_eq!(active().cpu_box, Color::Rgb(0xbd, 0x93, 0xf9));
    assert_eq!(
        v.to_config(&Config::default()).color_theme.as_deref(),
        Some("dracula")
    );
    set_palette(Palette::btop());
}

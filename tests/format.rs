use std::time::Duration;

use mtop::format::{
    clamp_percent, format_bytes, format_duration, format_percent, format_rate, truncate_text,
};
use mtop::theme::Theme;
use ratatui::style::Color;

#[test]
fn built_in_themes_have_distinct_palettes_and_unknown_names_fall_back_to_neon() {
    let neon = Theme::from_name("neon");
    let amber = Theme::from_name("amber");
    let mono = Theme::from_name("mono");

    assert_ne!(neon, amber);
    assert_ne!(amber, mono);
    assert_eq!(Theme::from_name("unknown"), neon);
    assert_eq!(neon.accent, Color::Rgb(0, 255, 255));
    assert_eq!(neon.graph, Color::Rgb(96, 160, 255));
    assert_violetish_and_distinct(neon.violet, neon.accent, neon.graph);

    assert_warm_palette(amber.accent);
    assert_warm_palette(amber.graph);
    assert_greenish(amber.good);

    for color in [
        mono.background,
        mono.panel,
        mono.border,
        mono.text,
        mono.muted,
        mono.accent,
        mono.good,
        mono.warn,
        mono.bad,
        mono.graph,
    ] {
        assert_grayscale(color);
    }
}

fn assert_violetish_and_distinct(violet: Color, accent: Color, graph: Color) {
    let Color::Rgb(red, green, blue) = violet else {
        panic!("violet must use an RGB color");
    };
    assert!(blue > red && red > green, "expected a violet RGB value");
    assert_ne!(violet, accent);
    assert_ne!(violet, graph);
}

fn assert_warm_palette(color: Color) {
    let Color::Rgb(red, green, blue) = color else {
        panic!("warm palette colors must use RGB values");
    };
    assert!(red > blue && green > blue, "expected a warm RGB value");
}

fn assert_greenish(color: Color) {
    let Color::Rgb(red, green, blue) = color else {
        panic!("good color must use an RGB value");
    };
    assert!(
        green > red && green > blue,
        "expected a green-ish RGB value"
    );
}

fn assert_grayscale(color: Color) {
    match color {
        Color::Rgb(red, green, blue) => assert_eq!((red, green), (blue, blue)),
        Color::Black | Color::DarkGray | Color::Gray | Color::White => {}
        other => panic!("expected grayscale color, got {other:?}"),
    }
}

#[test]
fn bytes_use_binary_units_and_stable_zero_format() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(999), "999 B");
    assert_eq!(format_bytes(1024), "1.0 KiB");
    assert_eq!(format_bytes(1536), "1.5 KiB");
    assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(format_bytes(1024_u64.pow(4)), "1.0 TiB");
}

#[test]
fn rates_use_binary_units_with_per_second_suffix() {
    assert_eq!(format_rate(0), "0 B/s");
    assert_eq!(format_rate(1024), "1.0 KiB/s");
    assert_eq!(format_rate(1536 * 1024), "1.5 MiB/s");
}

#[test]
fn durations_are_compact_and_omit_zero_leading_units() {
    assert_eq!(format_duration(Duration::ZERO), "0s");
    assert_eq!(format_duration(Duration::from_secs(59)), "59s");
    assert_eq!(format_duration(Duration::from_secs(60)), "1m");
    assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m 1s");
    assert_eq!(format_duration(Duration::from_secs(90_061)), "1d 1h 1m 1s");
}

#[test]
fn percentages_clamp_invalid_values_and_format_stably() {
    assert_eq!(clamp_percent(f32::NAN), 0.0);
    assert_eq!(clamp_percent(-1.0), 0.0);
    assert_eq!(clamp_percent(42.5), 42.5);
    assert_eq!(clamp_percent(f32::INFINITY), 100.0);
    assert_eq!(clamp_percent(101.0), 100.0);
    assert_eq!(format_percent(f32::NAN), "0.0%");
    assert_eq!(format_percent(42.56), "42.6%");
    assert_eq!(format_percent(f32::INFINITY), "100.0%");
}

#[test]
fn truncation_handles_small_widths_without_panicking() {
    assert_eq!(truncate_text("hello", 0), "");
    assert_eq!(truncate_text("hello", 1), "…");
    assert_eq!(truncate_text("hello world", 10), "hello wor…");
    assert_eq!(truncate_text("hello", 10), "hello");
}

#[test]
fn truncation_respects_unicode_scalar_boundaries() {
    let result = truncate_text("café au lait", 5);
    assert_eq!(result, "café…");
    assert!(result.is_char_boundary(result.len()));
    assert_eq!(truncate_text("🙂🙂🙂", 2), "🙂…");
}

#[test]
fn compact_bytes_fit_five_columns_like_btop() {
    use mtop::format::format_bytes_compact;
    assert_eq!(format_bytes_compact(512), "512B");
    assert_eq!(format_bytes_compact(3 * 1024 * 1024 + 900 * 1024), "3.9M");
    assert_eq!(format_bytes_compact(568 * 1024 * 1024 + 820 * 1024), "568M");
    assert_eq!(
        format_bytes_compact(1024 * 1024 * 1024 + 200 * 1024 * 1024),
        "1.2G"
    );
    assert_eq!(format_bytes_compact(1000 * 1024 * 1024), "1000M");
    for v in [
        0u64,
        1,
        1023,
        1024,
        99_999,
        10 << 20,
        999 << 20,
        1023 << 20,
        5 << 40,
    ] {
        assert!(format_bytes_compact(v).chars().count() <= 5, "{v}");
    }
}

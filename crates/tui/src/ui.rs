//! Terminal UI — mtop's Rust/Ratatui implementation, visually inspired by
//! btop++ (aristocratos/btop), with selected rendering details adapted.
//!
//! This file was modified and adapted by EvarinthoSec. Upstream attribution:
//!   Copyright (C) 2021 by Jakob P. Liljenberg <jakob@qvantnet.com>
//!   Source material is under Apache License, Version 2.0; see
//!   THIRD_PARTY_LICENSES/btop-Apache-2.0.txt.

use std::collections::VecDeque;
use std::io::{self, stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Table},
};

use crate::{
    app::{Action, CollectorCommandSender, SnapshotReceiver},
    config::ProcessSort,
    format::{
        format_bytes, format_bytes_compact, format_duration, format_percent, format_rate,
        truncate_text,
    },
    model::SystemSnapshot,
    process_control::{ProcessController, Signal},
    theme::Theme,
};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Palette — themeable color set (btop_theme.cpp is the default)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// Every color the dashboard draws with. The active palette is fixed once at
/// startup via [`set_palette`]; all draw code reads it through the `c_*()`
/// accessors so a `--theme` choice actually recolors the whole UI. The default
/// ([`Palette::btop`]) preserves btop's exact hex values that the pixel-assert
/// tests depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub cpu_box: Color,
    pub mem_box: Color,
    pub net_box: Color,
    pub proc_box: Color,
    pub title: Color,
    pub main_fg: Color,
    pub hi_fg: Color,
    pub graph_text: Color,
    pub inactive: Color,
    pub meter_bg: Color,
    pub cpu_start: Color,
    pub cpu_mid: Color,
    pub cpu_end: Color,
    pub used_end: Color,
    pub down_end: Color,
    pub up_end: Color,
    pub proc_misc: Color,
    pub swap: Color,
    pub avail_end: Color,
    pub gpu_box: Color,
    pub main_bg: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
}

impl Palette {
    /// btop's Default_theme — the reference look and the test baseline.
    pub const fn btop() -> Self {
        Self {
            cpu_box: Color::Rgb(0x55, 0x6d, 0x59),
            mem_box: Color::Rgb(0x6c, 0x6c, 0x4b),
            net_box: Color::Rgb(0x5c, 0x58, 0x8d),
            proc_box: Color::Rgb(0x80, 0x52, 0x52),
            title: Color::Rgb(0xee, 0xee, 0xee),
            main_fg: Color::Rgb(0xcc, 0xcc, 0xcc),
            hi_fg: Color::Rgb(0xb5, 0x40, 0x40),
            graph_text: Color::Rgb(0x60, 0x60, 0x60),
            inactive: Color::Rgb(0x40, 0x40, 0x40),
            meter_bg: Color::Rgb(0x30, 0x30, 0x30),
            cpu_start: Color::Rgb(0x77, 0xca, 0x9b),
            cpu_mid: Color::Rgb(0xf2, 0xe2, 0x6d),
            cpu_end: Color::Rgb(0xdc, 0x4c, 0x4c),
            used_end: Color::Rgb(0xff, 0x47, 0x69),
            down_end: Color::Rgb(0xb0, 0xa9, 0xde),
            up_end: Color::Rgb(0xdc, 0xaf, 0xde),
            proc_misc: Color::Rgb(0x0d, 0xe7, 0x56),
            swap: Color::Rgb(0x5c, 0xb8, 0x8e),
            avail_end: Color::Rgb(0x77, 0xca, 0x9b),
            gpu_box: Color::Rgb(0x6a, 0x5a, 0x8c),
            main_bg: Color::Rgb(0x00, 0x00, 0x00),
            selected_bg: Color::Rgb(0x6a, 0x2f, 0x2f),
            selected_fg: Color::Rgb(0xff, 0xff, 0xff),
        }
    }

    /// Amber / retro-terminal theme — warm oranges and browns.
    pub const fn amber() -> Self {
        Self {
            cpu_box: Color::Rgb(0x70, 0x4a, 0x1c),
            mem_box: Color::Rgb(0x7a, 0x5a, 0x1e),
            net_box: Color::Rgb(0x6a, 0x4a, 0x24),
            proc_box: Color::Rgb(0x80, 0x50, 0x20),
            title: Color::Rgb(0xff, 0xef, 0xcc),
            main_fg: Color::Rgb(0xe8, 0xc8, 0x90),
            hi_fg: Color::Rgb(0xff, 0xb0, 0x30),
            graph_text: Color::Rgb(0x8a, 0x6a, 0x3a),
            inactive: Color::Rgb(0x50, 0x3c, 0x1c),
            meter_bg: Color::Rgb(0x38, 0x2a, 0x12),
            cpu_start: Color::Rgb(0xff, 0xd0, 0x60),
            cpu_mid: Color::Rgb(0xff, 0x9c, 0x30),
            cpu_end: Color::Rgb(0xf0, 0x50, 0x20),
            used_end: Color::Rgb(0xff, 0x6a, 0x20),
            down_end: Color::Rgb(0xff, 0xc8, 0x70),
            up_end: Color::Rgb(0xff, 0x9c, 0x40),
            proc_misc: Color::Rgb(0xff, 0xc8, 0x40),
            swap: Color::Rgb(0xd0, 0x90, 0x40),
            avail_end: Color::Rgb(0xff, 0xd0, 0x60),
            gpu_box: Color::Rgb(0x74, 0x52, 0x20),
            main_bg: Color::Rgb(0x0c, 0x08, 0x02),
            selected_bg: Color::Rgb(0x5a, 0x38, 0x10),
            selected_fg: Color::Rgb(0xff, 0xef, 0xcc),
        }
    }

    /// Monochrome theme — grayscale for low-color or high-contrast terminals.
    pub const fn mono() -> Self {
        Self {
            cpu_box: Color::Rgb(0x80, 0x80, 0x80),
            mem_box: Color::Rgb(0x88, 0x88, 0x88),
            net_box: Color::Rgb(0x78, 0x78, 0x78),
            proc_box: Color::Rgb(0x90, 0x90, 0x90),
            title: Color::Rgb(0xf0, 0xf0, 0xf0),
            main_fg: Color::Rgb(0xcc, 0xcc, 0xcc),
            hi_fg: Color::Rgb(0xff, 0xff, 0xff),
            graph_text: Color::Rgb(0x70, 0x70, 0x70),
            inactive: Color::Rgb(0x44, 0x44, 0x44),
            meter_bg: Color::Rgb(0x2a, 0x2a, 0x2a),
            cpu_start: Color::Rgb(0x60, 0x60, 0x60),
            cpu_mid: Color::Rgb(0xa0, 0xa0, 0xa0),
            cpu_end: Color::Rgb(0xf0, 0xf0, 0xf0),
            used_end: Color::Rgb(0xe0, 0xe0, 0xe0),
            down_end: Color::Rgb(0xd0, 0xd0, 0xd0),
            up_end: Color::Rgb(0xb0, 0xb0, 0xb0),
            proc_misc: Color::Rgb(0xc0, 0xc0, 0xc0),
            swap: Color::Rgb(0x98, 0x98, 0x98),
            avail_end: Color::Rgb(0x60, 0x60, 0x60),
            gpu_box: Color::Rgb(0x84, 0x84, 0x84),
            main_bg: Color::Rgb(0x00, 0x00, 0x00),
            selected_bg: Color::Rgb(0x50, 0x50, 0x50),
            selected_fg: Color::Rgb(0xff, 0xff, 0xff),
        }
    }

    /// Resolve a theme name to a palette. Unknown names fall back to btop.
    pub fn from_name(name: &str) -> Self {
        match name {
            "amber" => Self::amber(),
            "mono" => Self::mono(),
            _ => Self::btop(),
        }
    }
}

/// Parse a btop color value: "#rrggbb", "#gg" (grayscale) or "r g b".
fn parse_btop_color(v: &str) -> Option<Color> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        return match hex.len() {
            6 => Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?)),
            2 => byte(0).map(|g| Color::Rgb(g, g, g)),
            _ => None,
        };
    }
    let parts: Vec<u8> = v
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [r, g, b] => Some(Color::Rgb(*r, *g, *b)),
        _ => None,
    }
}

/// Overlay a btop `.theme` file (`theme[key]="value"` lines) onto `base`.
/// Unknown keys and malformed lines are ignored, so any btop theme loads.
pub fn parse_btop_theme(text: &str, base: Palette) -> Palette {
    let mut p = base;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("theme[") else {
            continue;
        };
        let Some((key, value)) = rest.split_once("]=") else {
            continue;
        };
        let Some(color) = parse_btop_color(value.trim().trim_matches('"')) else {
            continue;
        };
        let slot = match key {
            "main_bg" => &mut p.main_bg,
            "main_fg" => &mut p.main_fg,
            "title" => &mut p.title,
            "hi_fg" => &mut p.hi_fg,
            "selected_bg" => &mut p.selected_bg,
            "selected_fg" => &mut p.selected_fg,
            "inactive_fg" => &mut p.inactive,
            "graph_text" => &mut p.graph_text,
            "meter_bg" => &mut p.meter_bg,
            "proc_misc" => &mut p.proc_misc,
            "cpu_box" => &mut p.cpu_box,
            "mem_box" => &mut p.mem_box,
            "net_box" => &mut p.net_box,
            "proc_box" => &mut p.proc_box,
            "cpu_start" => &mut p.cpu_start,
            "cpu_mid" => &mut p.cpu_mid,
            "cpu_end" => &mut p.cpu_end,
            "used_end" => &mut p.used_end,
            "available_end" => &mut p.avail_end,
            "download_end" => &mut p.down_end,
            "upload_end" => &mut p.up_end,
            "cached_end" => &mut p.swap,
            _ => continue,
        };
        *slot = color;
    }
    p
}

/// All box names in layout order.
const ALL_BOXES: [&str; 5] = ["cpu", "mem", "net", "proc", "gpu"];

/// Parse btop's `presets` string into box sets. Preset 0 is always every box;
/// then up to 9 user presets. Unknown box names are skipped and a preset that
/// ends up empty is dropped (btop rejects it).
pub fn parse_presets(text: &str) -> Vec<Vec<&'static str>> {
    preset_entries(text)
        .into_iter()
        .map(|(boxes, _)| boxes)
        .collect()
}

fn ratty_dashboard_panel_areas(area: Rect, view: &AppView) -> Vec<Rect> {
    if area.width < 30 || area.height < 9 {
        return Vec::new();
    }
    let body = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(area)[1];
    ratty_panel_areas(body, view)
}

/// Valid presets as (box set, raw group string); index 0 = all boxes, "".
fn preset_entries(text: &str) -> Vec<(Vec<&'static str>, String)> {
    let mut out = vec![(ALL_BOXES.to_vec(), String::new())];
    for group in text.split_whitespace() {
        if out.len() == 10 {
            break;
        }
        let boxes: Vec<&'static str> = group
            .split(',')
            .filter_map(|entry| {
                let name = entry.split(':').next()?;
                ALL_BOXES.iter().copied().find(|b| *b == name)
            })
            .collect();
        if !boxes.is_empty() {
            out.push((boxes, group.to_owned()));
        }
    }
    out
}

/// btop preset P flags: `cpu:1` → cpu_bottom, `proc:1` → proc_left,
/// `mem:1` → mem_below_net. Returns (cpu_bottom, proc_left, mem_below_net).
pub fn parse_preset_positions(group: &str) -> (bool, bool, bool) {
    let flag = |name: &str| {
        group.split(',').any(|entry| {
            let mut it = entry.split(':');
            it.next() == Some(name) && it.next() == Some("1")
        })
    };
    (flag("cpu"), flag("proc"), flag("mem"))
}

/// btop-style user theme directory: `<config dir>/mtop/themes`.
pub fn theme_dir() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("mtop").join("themes"))
}

/// Names (without `.theme`) of theme files in `dir`, sorted.
pub fn list_theme_files(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".theme").map(str::to_owned)
        })
        .collect();
    names.sort();
    names
}

/// Load `<dir>/<name>.theme`. Names with path separators are rejected.
pub fn load_theme_file(dir: &std::path::Path, name: &str) -> Option<Palette> {
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return None;
    }
    let text = std::fs::read_to_string(dir.join(format!("{name}.theme"))).ok()?;
    Some(parse_btop_theme(&text, Palette::btop()))
}

thread_local! {
    /// Active palette for the thread that draws. The TUI renders on a single
    /// thread, and a thread-local keeps parallel tests isolated: a test that
    /// switches theme cannot recolor another test's frame.
    static PALETTE: std::cell::Cell<Palette> = const { std::cell::Cell::new(Palette::btop()) };
}

/// Install the active palette (can be changed at runtime, e.g. from the
/// options menu).
pub fn set_palette(palette: Palette) {
    PALETTE.with(|p| p.set(palette));
}

/// The active palette (btop default until `set_palette` is called).
#[inline]
pub fn active() -> Palette {
    PALETTE.with(|p| p.get())
}

// Accessors — every draw site reads colors through these so the active theme
// takes effect everywhere without threading a palette parameter around.
#[inline]
fn c_cpu_box() -> Color {
    active().cpu_box
}
#[inline]
fn c_mem_box() -> Color {
    active().mem_box
}
#[inline]
fn c_net_box() -> Color {
    active().net_box
}
#[inline]
fn c_proc_box() -> Color {
    active().proc_box
}
#[inline]
fn c_title() -> Color {
    active().title
}
#[inline]
fn c_main_fg() -> Color {
    active().main_fg
}
#[inline]
fn c_hi_fg() -> Color {
    active().hi_fg
}
#[inline]
fn c_graph_text() -> Color {
    active().graph_text
}
#[inline]
fn c_inactive() -> Color {
    active().inactive
}
#[inline]
fn c_meter_bg() -> Color {
    active().meter_bg
}
#[inline]
fn c_cpu_start() -> Color {
    active().cpu_start
}
#[inline]
fn c_used_end() -> Color {
    active().used_end
}
#[inline]
fn c_down_end() -> Color {
    active().down_end
}
#[inline]
fn c_up_end() -> Color {
    active().up_end
}
#[inline]
fn c_proc_misc() -> Color {
    active().proc_misc
}
#[inline]
fn c_swap() -> Color {
    active().swap
}
#[inline]
fn c_avail_end() -> Color {
    active().avail_end
}
#[inline]
fn c_selected_bg() -> Color {
    active().selected_bg
}
#[inline]
fn c_selected_fg() -> Color {
    active().selected_fg
}
#[inline]
fn c_gpu_box() -> Color {
    active().gpu_box
}
#[inline]
fn c_main_bg() -> Color {
    // btop theme_background=false: let the terminal background show through.
    if STYLE.with(|s| s.get()).transparent_bg {
        Color::Reset
    } else {
        active().main_bg
    }
}

/// Theme background as a real color (for blending even when transparent).
fn c_main_bg_solid() -> Color {
    active().main_bg
}

/// Render-wide style switches from config (thread-local like the palette).
#[derive(Clone, Copy, Debug, Default)]
struct RenderStyle {
    square_corners: bool,
    transparent_bg: bool,
}

thread_local! {
    static STYLE: std::cell::Cell<RenderStyle> = const {
        std::cell::Cell::new(RenderStyle { square_corners: false, transparent_bg: false })
    };
}

/// Box border type honoring btop `rounded_corners`.
fn border_type() -> BorderType {
    if STYLE.with(|s| s.get()).square_corners {
        BorderType::Plain
    } else {
        BorderType::Rounded
    }
}

/// Convert a Celsius reading to btop's `temp_scale`, returning (value, unit).
pub fn scale_temp(celsius: f32, scale: &str) -> (f32, &'static str) {
    match scale {
        "fahrenheit" => (celsius * 9.0 / 5.0 + 32.0, "°F"),
        "kelvin" => (celsius + 273.15, "K"),
        "rankine" => ((celsius + 273.15) * 9.0 / 5.0, "°R"),
        _ => (celsius, "°C"),
    }
}

/// strftime for the local time of `at` (btop clock_format). Empty format →
/// empty string (btop hides the clock).
pub fn format_clock(at: std::time::SystemTime, fmt: &str) -> String {
    if fmt.is_empty() {
        return String::new();
    }
    chrono::DateTime::<chrono::Local>::from(at)
        .format(fmt)
        .to_string()
}

/// CPU gradient: start → mid → end, driven by the active palette
/// (btop's green → yellow → red by default).
pub fn cpu_gradient(pct: f32) -> Color {
    let pal = active();
    let p = pct.clamp(0.0, 100.0) / 100.0;
    if p <= 0.5 {
        lerp_color(pal.cpu_start, pal.cpu_mid, p * 2.0)
    } else {
        lerp_color(pal.cpu_mid, pal.cpu_end, (p - 0.5) * 2.0)
    }
}
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

/// Linear interpolation between two RGB colors. Non-RGB colors (e.g. named
/// ANSI) are returned as the endpoint, since they have no numeric channels.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            Color::Rgb(lerp(ar, br, t), lerp(ag, bg, t), lerp(ab, bb, t))
        }
        _ if t < 0.5 => a,
        _ => b,
    }
}

/// Dark base color used as the low end of the net/mem history gradients.
const GRAD_BASE: Color = Color::Rgb(0x30, 0x30, 0x30);

/// Net download gradient: dark -> palette down_end.
fn down_gradient(pct: f32) -> Color {
    lerp_color(GRAD_BASE, active().down_end, (pct / 100.0).clamp(0.0, 1.0))
}
/// Net upload gradient: dark -> palette up_end.
fn up_gradient(pct: f32) -> Color {
    lerp_color(GRAD_BASE, active().up_end, (pct / 100.0).clamp(0.0, 1.0))
}
/// Memory usage gradient: dark -> palette used_end.
fn mem_gradient(pct: f32) -> Color {
    lerp_color(GRAD_BASE, active().used_end, (pct / 100.0).clamp(0.0, 1.0))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// btop braille graph symbol tables  (btop_draw.cpp Symbols::graph_symbols)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const BRAILLE_UP: [&str; 25] = [
    " ", "⢀", "⢠", "⢰", "⢸", "⡀", "⣀", "⣠", "⣰", "⣸", "⡄", "⣄", "⣤", "⣴", "⣼", "⡆", "⣆", "⣦", "⣶",
    "⣾", "⡇", "⣇", "⣧", "⣷", "⣿",
];
const BRAILLE_DOWN: [&str; 25] = [
    " ", "⠈", "⠘", "⠸", "⢸", "⠁", "⠉", "⠙", "⠹", "⢹", "⠃", "⠋", "⠛", "⠻", "⢻", "⠇", "⠏", "⠟", "⠿",
    "⢿", "⡇", "⡏", "⡟", "⡿", "⣿",
];
// btop Symbols::graph_symbols "block_up" / "block_down" / "tty_up" / "tty_down".
const BLOCK_UP: [&str; 25] = [
    " ", "▗", "▗", "▐", "▐", "▖", "▄", "▄", "▟", "▟", "▖", "▄", "▄", "▟", "▟", "▌", "▙", "▙", "█",
    "█", "▌", "▙", "▙", "█", "█",
];
const BLOCK_DOWN: [&str; 25] = [
    " ", "▝", "▝", "▐", "▐", "▘", "▀", "▀", "▜", "▜", "▘", "▀", "▀", "▜", "▜", "▌", "▛", "▛", "█",
    "█", "▌", "▛", "▛", "█", "█",
];
const TTY: [&str; 25] = [
    " ", "░", "░", "▒", "▒", "░", "░", "▒", "▒", "█", "░", "▒", "▒", "▒", "█", "▒", "▒", "▒", "█",
    "█", "▒", "█", "█", "█", "█",
];

/// btop `graph_symbol`: glyph set used to draw history graphs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphSymbol {
    #[default]
    Braille,
    Block,
    Tty,
}

impl GraphSymbol {
    /// btop names; "default" (inherit) and unknown names return None.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "braille" => Some(Self::Braille),
            "block" => Some(Self::Block),
            "tty" => Some(Self::Tty),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Braille => "braille",
            Self::Block => "block",
            Self::Tty => "tty",
        }
    }

    fn table(self, invert: bool) -> &'static [&'static str; 25] {
        match (self, invert) {
            (Self::Braille, false) => &BRAILLE_UP,
            (Self::Braille, true) => &BRAILLE_DOWN,
            (Self::Block, false) => &BLOCK_UP,
            (Self::Block, true) => &BLOCK_DOWN,
            (Self::Tty, _) => &TTY,
        }
    }
}

thread_local! {
    /// Glyph set for graphs drawn right now. Each box sets it before drawing
    /// (btop graph_symbol_<box>), so the shared graph helpers stay unchanged.
    static GRAPH_SYMBOL: std::cell::Cell<GraphSymbol> = const { std::cell::Cell::new(GraphSymbol::Braille) };
}

fn with_graph_symbol<R>(sym: GraphSymbol, f: impl FnOnce() -> R) -> R {
    let prev = GRAPH_SYMBOL.with(|g| g.replace(sym));
    let out = f();
    GRAPH_SYMBOL.with(|g| g.set(prev));
    out
}

/// btop Graph::_create algorithm (btop_draw.cpp lines 422–488).
/// Returns one String per row (top-to-bottom), each is `width` braille chars.
pub fn braille_graph(
    values: &VecDeque<u64>,
    max_value: u64,
    width: usize,
    height: usize,
    invert: bool,
) -> Vec<String> {
    graph_rows(
        values,
        max_value,
        width,
        height,
        invert,
        GRAPH_SYMBOL.with(|g| g.get()),
    )
}

/// Graph rows using an explicit glyph set (same geometry for every set).
pub fn graph_rows(
    values: &VecDeque<u64>,
    max_value: u64,
    width: usize,
    height: usize,
    invert: bool,
    symbol: GraphSymbol,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return vec![String::new(); height];
    }
    let max = max_value.max(1);
    let normed: Vec<i64> = values
        .iter()
        .map(|&v| ((v.min(max) * 100 / max) as i64).clamp(0, 100))
        .collect();
    let need = width * 2;
    let normed_padded: Vec<i64> = if normed.len() >= need {
        normed[normed.len() - need..].to_vec()
    } else {
        // Left-pad missing history with the OLDEST sample (a flat baseline),
        // not zeros. Zeros render as blank braille, which left a cold-start
        // graph almost empty with the real data crammed at the right edge.
        // btop shows a full-width baseline that fills in with variation.
        let baseline = normed.first().copied().unwrap_or(0);
        let mut v = vec![baseline; need - normed.len()];
        v.extend_from_slice(&normed);
        v
    };
    let syms = symbol.table(invert);
    let mod_val: f32 = if height == 1 { 0.3 } else { 0.1 };
    let mut rows: Vec<String> = vec![String::new(); height];
    for col in 0..width {
        let left = normed_padded[col * 2];
        let right = normed_padded[col * 2 + 1];
        for (row, row_str) in rows.iter_mut().enumerate() {
            let cur_high = ((100.0 * (height - row) as f32 / height as f32).round()) as i64;
            let cur_low = ((100.0 * (height - row - 1) as f32 / height as f32).round()) as i64;
            let mut result = [0usize; 2];
            for (ai, &val) in [left, right].iter().enumerate() {
                result[ai] = if val >= cur_high {
                    4
                } else if val <= cur_low {
                    0
                } else {
                    ((val - cur_low) as f32 * 4.0 / (cur_high - cur_low) as f32 + mod_val)
                        .round()
                        .clamp(0.0, 4.0) as usize
                };
            }
            row_str.push_str(syms[result[0] * 5 + result[1]]);
        }
    }
    if invert {
        rows.reverse();
    }
    rows
}

/// Render braille rows directly into the frame.
pub fn draw_braille(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    values: &VecDeque<u64>,
    max_value: u64,
    color: Color,
    invert: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = braille_graph(
        values,
        max_value,
        area.width as usize,
        area.height as usize,
        invert,
    );
    let lines: Vec<Line> = rows
        .iter()
        .map(|r| Line::from(Span::styled(r.clone(), Style::default().fg(color))))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Like `draw_braille` but colors each ROW by a vertical gradient: the bottom
/// row uses `grad(0.0)` and the top row uses `grad(100.0)`, matching btop's
/// CPU graph (green at the base, red at the peak).
pub fn draw_braille_gradient(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    values: &VecDeque<u64>,
    max_value: u64,
    grad: fn(f32) -> Color,
    invert: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = braille_graph(
        values,
        max_value,
        area.width as usize,
        area.height as usize,
        invert,
    );
    let h = rows.len().max(1) as f32;
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(r, s)| {
            // r = 0 is the TOP row. Bottom row -> 0%, top row -> 100%.
            let denom = (h - 1.0).max(1.0);
            let pct = (h - 1.0 - r as f32) / denom * 100.0;
            Line::from(Span::styled(s.clone(), Style::default().fg(grad(pct))))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}
// Symbol::meter = "■"
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
const METER: &str = "■";

pub fn meter_spans(value: f32, width: usize, grad: fn(f32) -> Color) -> Vec<Span<'static>> {
    if width == 0 {
        return vec![];
    }
    let value = value.clamp(0.0, 100.0);
    let filled = ((value / 100.0) * width as f32).round() as usize;
    let mut spans = Vec::with_capacity(width + 1);
    for i in 1..=width {
        let y = (i as f32 * 100.0 / width as f32).round();
        if i <= filled {
            spans.push(Span::styled(METER, Style::default().fg(grad(y))));
        } else {
            let remaining = width + 1 - i;
            spans.push(Span::styled(
                METER.repeat(remaining),
                Style::default().fg(c_meter_bg()),
            ));
            break;
        }
    }
    spans
}

fn mem_meter_spans(value: f32, width: usize, color: Color) -> Vec<Span<'static>> {
    let value = value.clamp(0.0, 100.0);
    let filled = ((value / 100.0) * width as f32).round() as usize;
    vec![
        Span::styled(METER.repeat(filled), Style::default().fg(color)),
        Span::styled(
            METER.repeat(width.saturating_sub(filled)),
            Style::default().fg(c_meter_bg()),
        ),
    ]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// MetricHistory
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// Samples kept per metric. btop keeps enough history to fill the graph at any
/// width; a braille column consumes 2 samples, so this must exceed 2× the
/// widest graph area (a 200+ col terminal → ~180 col graph → 360 samples).
pub const HISTORY_LIMIT: usize = 400;

#[derive(Clone, Debug, Default)]
pub struct MetricHistory {
    pub cpu: VecDeque<u64>,
    pub memory: VecDeque<u64>,
    /// Per-interface (rx, tx) rate history, so switching interfaces with
    /// b/n shows that interface's past immediately (btop keeps all NICs).
    pub net: std::collections::HashMap<String, (VecDeque<u64>, VecDeque<u64>)>,
    pub disk_read: VecDeque<u64>,
    pub disk_write: VecDeque<u64>,
    pub gpu: VecDeque<u64>,
    pub npu_usage: VecDeque<u64>,
    pub npu_power_mw: VecDeque<u64>,
    pub cpu_power_mw: VecDeque<u64>,
    pub gpu_power_mw: VecDeque<u64>,
    pub dram_power_mw: VecDeque<u64>,
    pub package_power_mw: VecDeque<u64>,
    /// Swap used as % of swap total.
    pub swap: VecDeque<u64>,
    /// Per-pid cpu% history for the detail panel. Pids that disappear are
    /// pruned every sample, so this stays bounded by live processes.
    pub procs: std::collections::HashMap<u32, VecDeque<u64>>,
}

impl MetricHistory {
    /// Cpu% history of one process (oldest first).
    pub fn proc_cpu(&self, pid: u32) -> Vec<u64> {
        self.procs
            .get(&pid)
            .map(|q| q.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Download history for one interface (oldest first).
    pub fn net_rx(&self, iface: &str) -> Vec<u64> {
        self.net
            .get(iface)
            .map(|(rx, _)| rx.iter().copied().collect())
            .unwrap_or_default()
    }
    /// Upload history for one interface (oldest first).
    pub fn net_tx(&self, iface: &str) -> Vec<u64> {
        self.net
            .get(iface)
            .map(|(_, tx)| tx.iter().copied().collect())
            .unwrap_or_default()
    }
    fn push(q: &mut VecDeque<u64>, v: u64) {
        if q.len() == HISTORY_LIMIT {
            q.pop_front();
        }
        q.push_back(v);
    }
    fn record(&mut self, snap: &SystemSnapshot) {
        if snap.cpu.available {
            Self::push(
                &mut self.cpu,
                snap.cpu.overall_percent.clamp(0.0, 100.0) as u64,
            );
        }
        if snap.memory.total_bytes > 0 {
            let pct = (snap.memory.used_bytes as f64 / snap.memory.total_bytes as f64 * 100.0)
                .clamp(0.0, 100.0) as u64;
            Self::push(&mut self.memory, pct);
        }
        if snap.memory.swap_total_bytes > 0 {
            let pct = (snap.memory.swap_used_bytes as f64 / snap.memory.swap_total_bytes as f64
                * 100.0)
                .clamp(0.0, 100.0) as u64;
            Self::push(&mut self.swap, pct);
        }
        for n in &snap.networks {
            let (rx, tx) = self.net.entry(n.interface.clone()).or_default();
            Self::push(rx, n.received_bytes_per_second);
            Self::push(tx, n.transmitted_bytes_per_second);
        }
        let live: std::collections::HashSet<u32> = snap.processes.iter().map(|p| p.pid).collect();
        self.procs.retain(|pid, _| live.contains(pid));
        for p in &snap.processes {
            let q = self.procs.entry(p.pid).or_default();
            Self::push(q, p.cpu_percent.max(0.0).round() as u64);
        }
        if let Some(u) = snap.gpus.first().and_then(|g| g.utilization_percent) {
            Self::push(&mut self.gpu, u.clamp(0.0, 100.0) as u64);
        }
        if let Some(u) = snap.npus.first().and_then(|n| n.utilization_percent) {
            Self::push(&mut self.npu_usage, u.clamp(0.0, 100.0) as u64);
        }
        let push_power = |history: &mut VecDeque<u64>, watts: Option<f32>| {
            if let Some(milliwatts) = watts
                .filter(|w| w.is_finite() && *w >= 0.0)
                .map(|w| (w * 1000.0).round() as u64)
            {
                Self::push(history, milliwatts);
            }
        };
        let npu_power = snap
            .npus
            .first()
            .and_then(|n| n.power_watts)
            .or(snap.power.npu_watts);
        push_power(&mut self.npu_power_mw, npu_power);
        push_power(&mut self.cpu_power_mw, snap.power.cpu_watts);
        push_power(&mut self.gpu_power_mw, snap.power.gpu_watts);
        push_power(&mut self.dram_power_mw, snap.power.dram_watts);
        push_power(&mut self.package_power_mw, snap.power.package_watts);
        if let Some(d) = snap.disks.first() {
            if let Some(r) = d.read_bytes_per_second {
                Self::push(&mut self.disk_read, r);
            }
            if let Some(w) = d.write_bytes_per_second {
                Self::push(&mut self.disk_write, w);
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AppView
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Screen geometry of the process list, recorded on each draw so mouse clicks
/// can be mapped back to a visible row index. All coords are absolute cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProcHitZone {
    pub x0: u16,      // left edge of the table
    pub x1: u16,      // right edge (exclusive)
    pub data_y0: u16, // y of the first data row (below header)
    pub rows: u16,    // number of data rows drawn
    pub scroll: u16,  // vis index of the first data row
}

/// Result of routing one key event through `AppView::feed_key`, telling the
/// event loop what side effect (if any) to perform outside the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Nothing further; keep looping.
    Continue,
    /// The app should quit.
    Quit,
    /// A refresh should be wired to the collector.
    WireRefresh,
    /// The staged signal should be dispatched via the controller.
    DispatchSignal,
    /// The staged nice value should be applied via the controller.
    DispatchRenice,
    /// btop ctrl+z suspend on Unix; Windows reports that suspend is unavailable.
    Suspend,
    /// btop ctrl+r: re-read the config file from disk.
    ReloadConfig,
}

/// btop overlay menus. `Main` is the Esc/m menu, `Options` the o/F2 panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Main { selected: usize },
    Options { selected: usize },
}

/// Full-screen dashboard pages shown in the top navigation strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(usize)]
pub enum DashboardPage {
    #[default]
    Overview,
    Cpu,
    Memory,
    Gpu,
    Npu,
    Network,
    Processes,
    Storage,
    Power,
}

impl DashboardPage {
    pub const ALL: [Self; 9] = [
        Self::Overview,
        Self::Cpu,
        Self::Memory,
        Self::Gpu,
        Self::Npu,
        Self::Network,
        Self::Processes,
        Self::Storage,
        Self::Power,
    ];

    fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        Self::ALL[(self as usize + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    fn label(self, compact: bool) -> &'static str {
        match (self, compact) {
            (Self::Overview, _) => "Overview",
            (Self::Cpu, _) => "CPU",
            (Self::Memory, true) => "Mem",
            (Self::Memory, false) => "Memory",
            (Self::Gpu, _) => "GPU",
            (Self::Npu, _) => "NPU",
            (Self::Network, true) => "Net",
            (Self::Network, false) => "Network",
            (Self::Processes, true) => "Proc",
            (Self::Processes, false) => "Processes",
            (Self::Storage, true) => "Disk",
            (Self::Storage, false) => "Storage",
            (Self::Power, _) => "Power",
        }
    }
}

/// Main-menu entries (btop: Options / Help / Quit).
pub const MAIN_MENU_ITEMS: [&str; 3] = ["Options", "Help", "Quit"];

/// Options-panel rows, in display order.
pub const OPTION_ROWS: [(&str, &str); 11] = [
    (
        "Color theme",
        "neon/amber/mono or any btop .theme in <config>/mtop/themes.",
    ),
    (
        "Update ms",
        "Collector interval. Left/Right step 100ms (100..10000).",
    ),
    (
        "Vim keys",
        "h/j/k/l navigation; kill moves to K, help to H (btop vim_keys).",
    ),
    (
        "Per-core",
        "Show one meter per CPU core in the cpu box (key 2).",
    ),
    (
        "Show GPU",
        "Show the gpu box beside the cpu box when a GPU exists (key 5).",
    ),
    (
        "Tree view",
        "Show processes as a parent/child tree (key e).",
    ),
    (
        "Cpu bottom",
        "Show the cpu box at the bottom of the screen (btop cpu_bottom).",
    ),
    (
        "Proc left",
        "Show the proc box on the left side (btop proc_left).",
    ),
    (
        "Mem below net",
        "Show the mem box below the net box (btop mem_below_net).",
    ),
    (
        "Graph symbol",
        "braille (finest), block, or tty (works in any terminal font).",
    ),
    (
        "Temperature",
        "Cycle CPU/GPU temperature units: Celsius, Fahrenheit, Kelvin.",
    ),
];

/// Built-in theme names cycled by the options panel.
pub const THEME_NAMES: [&str; 3] = ["neon", "amber", "mono"];

#[derive(Clone, Debug)]
pub struct AppView {
    pub snapshot: SystemSnapshot,
    pub theme: Theme,
    pub page: DashboardPage,
    /// Emit Ratty Graphics Protocol panel frames when running in the Ratty host.
    pub ratty_3d: bool,
    pub show_help: bool,
    pub paused: bool,
    pub refresh_interval: Duration,
    pub selected_process: usize,
    pub proc_scroll: usize,  // first visible row in proc table
    pub show_detail: bool,   // btop show_detailed
    pub filter: String,      // btop proc filter
    pub filter_active: bool, // filter input mode
    pub sort: ProcessSort,
    pub sort_reverse: bool,
    pub tree: bool,
    pub show_cores: bool,
    pub show_gpu: bool,
    pub show_cpu: bool,
    pub show_mem: bool,
    pub show_net: bool,
    pub show_proc: bool,
    pub vim_keys: bool,
    /// Interface shown in the net box (name, so it survives reordering).
    pub net_selected: Option<String>,
    /// btop `z`: per-interface (rx, tx) totals captured at reset time.
    pub net_zero: std::collections::HashMap<String, (u64, u64)>,
    /// btop `a`: auto-scale graphs to history max (else fixed 100 Mbit).
    pub net_auto: bool,
    /// btop `y`: download and upload graphs share one ceiling.
    pub net_sync: bool,
    /// btop F: pid the selection follows across refreshes.
    pub followed_pid: Option<u32>,
    /// btop u: process list frozen while other boxes update.
    pub proc_paused: bool,
    /// btop d: disks view in the mem box area.
    pub show_disks: bool,
    /// Tree nodes whose children are hidden (btop collapse).
    pub collapsed: std::collections::HashSet<u32>,
    /// btop c: per-core process cpu% (true) or share of total cpu.
    pub proc_per_core: bool,
    /// btop %: memory column as percent of RAM instead of bytes.
    pub proc_mem_percent: bool,
    /// btop i: disks box shows big read/write graphs.
    pub disk_io_mode: bool,
    pub menu: Option<Menu>,
    pub theme_name: String,
    /// Directory scanned for btop `.theme` files (None disables).
    pub theme_dir: Option<std::path::PathBuf>,
    /// btop presets (box sets) and the active index for p / P.
    pub presets: Vec<Vec<&'static str>>,
    pub preset: usize,
    /// Raw preset strings (index-aligned with `presets`) for position flags.
    pub preset_groups: Vec<String>,
    /// btop cpu_bottom / proc_left / mem_below_net box positions.
    pub cpu_bottom: bool,
    pub proc_left: bool,
    pub mem_below_net: bool,
    /// btop graph_symbol (global default) and graph_symbol_<box> overrides.
    pub graph_symbol: GraphSymbol,
    /// btop display options (see Config for meaning).
    pub show_uptime: bool,
    pub show_battery: bool,
    pub check_temp: bool,
    pub show_cpu_freq: bool,
    pub disable_mouse: bool,
    pub custom_cpu_name: String,
    pub temp_scale: String,
    pub clock_format: String,
    /// btop.conf display keys applied at draw time.
    pub proc_colors: bool,
    pub proc_gradient: bool,
    pub proc_aggregate: bool,
    pub show_swap: bool,
    pub mem_graphs: bool,
    pub show_coretemp: bool,
    pub swap_upload_download: bool,
    /// btop net_download / net_upload in Mebibits (fixed scale when !net_auto).
    pub net_download_mbit: u64,
    pub net_upload_mbit: u64,
    pub disks_filter: String,
    truecolor: bool,
    force_tty: bool,
    pub box_symbols: std::collections::HashMap<&'static str, GraphSymbol>,
    /// Per-box symbols from the config; presets overlay these, preset 0
    /// restores them.
    cfg_box_symbols: std::collections::HashMap<&'static str, GraphSymbol>,
    pub pending_signal: Option<(u32, Signal)>,
    pub signal_picker: Option<String>,
    /// btop N: nice value being typed for the selected process.
    pub nice_picker: Option<String>,
    /// Renice staged by the picker, applied by the event loop.
    pub pending_nice: Option<(u32, i32)>,
    /// One-line result of the last process action (shown in the footer).
    pub status_msg: Option<String>, // Some(buffer) while the signal picker is open
    pub hostname: String,
    pub history: MetricHistory,
    proc_hit: std::cell::Cell<ProcHitZone>,
    /// Clickable regions recorded during the last draw (btop mouse_mappings).
    hits: std::cell::RefCell<Vec<(Rect, Action)>>,
    nav_hits: std::cell::RefCell<Vec<(Rect, DashboardPage)>>,
}

impl AppView {
    pub fn new(snapshot: SystemSnapshot, theme: Theme) -> Self {
        Self {
            snapshot,
            theme,
            page: DashboardPage::Overview,
            ratty_3d: false,
            show_help: false,
            paused: false,
            refresh_interval: Duration::from_secs(1),
            selected_process: 0,
            proc_scroll: 0,
            show_detail: false,
            filter: String::new(),
            filter_active: false,
            sort: ProcessSort::Cpu,
            sort_reverse: false,
            tree: false,
            show_cores: true,
            show_gpu: true,
            show_cpu: true,
            show_mem: true,
            show_net: true,
            show_proc: true,
            vim_keys: true,
            net_selected: None,
            net_zero: std::collections::HashMap::new(),
            net_auto: true,
            net_sync: false,
            followed_pid: None,
            proc_paused: false,
            show_disks: true,
            collapsed: std::collections::HashSet::new(),
            proc_per_core: true,
            proc_mem_percent: false,
            disk_io_mode: false,
            menu: None,
            theme_name: "neon".to_owned(),
            theme_dir: None,
            presets: parse_presets(""),
            preset: 0,
            preset_groups: vec![String::new()],
            cpu_bottom: false,
            proc_left: false,
            mem_below_net: false,
            graph_symbol: GraphSymbol::Braille,
            show_uptime: true,
            show_battery: true,
            check_temp: true,
            show_cpu_freq: true,
            disable_mouse: false,
            custom_cpu_name: String::new(),
            temp_scale: "celsius".to_owned(),
            clock_format: "%X".to_owned(),
            proc_colors: true,
            proc_gradient: true,
            proc_aggregate: false,
            show_swap: true,
            mem_graphs: true,
            show_coretemp: true,
            swap_upload_download: false,
            net_download_mbit: 100,
            net_upload_mbit: 100,
            disks_filter: String::new(),
            truecolor: true,
            force_tty: false,
            box_symbols: std::collections::HashMap::new(),
            cfg_box_symbols: std::collections::HashMap::new(),
            pending_signal: None,
            signal_picker: None,
            nice_picker: None,
            pending_nice: None,
            status_msg: None,
            hostname: hostname_or_default(),
            history: MetricHistory::default(),
            proc_hit: std::cell::Cell::new(ProcHitZone::default()),
            hits: std::cell::RefCell::new(Vec::new()),
            nav_hits: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// The process-list hit zone recorded on the last draw.
    pub fn proc_hit(&self) -> ProcHitZone {
        self.proc_hit.get()
    }

    /// Record the process-list geometry during draw (interior mutability so
    /// draw can keep taking &self).
    pub(crate) fn set_proc_hit(&self, zone: ProcHitZone) {
        self.proc_hit.set(zone);
    }

    /// Register a clickable region for this frame.
    pub(crate) fn add_hit(&self, rect: Rect, action: Action) {
        self.hits.borrow_mut().push((rect, action));
    }

    /// Route a mouse event: wheel scrolls, left click hits a registered
    /// button (title toggle / sort header) or selects a process row.
    /// Ignored while any overlay owns input, like btop.
    pub fn feed_mouse(&mut self, m: crossterm::event::MouseEvent) {
        let overlay = self.pending_signal.is_some()
            || self.disable_mouse
            || self.signal_picker.is_some()
            || self.nice_picker.is_some()
            || self.menu.is_some()
            || self.show_help;
        if overlay {
            return;
        }
        if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = m.kind {
            let page = self
                .nav_hits
                .borrow()
                .iter()
                .find(|(rect, _)| rect.contains(ratatui::layout::Position::new(m.column, m.row)))
                .map(|(_, page)| *page);
            if let Some(page) = page {
                self.page = page;
                return;
            }
        }
        match m.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let hit = self
                    .hits
                    .borrow()
                    .iter()
                    .find(|(r, _)| r.contains(ratatui::layout::Position::new(m.column, m.row)))
                    .map(|(_, a)| a.clone());
                if let Some(action) = hit {
                    self.apply(action);
                } else if let Some(vis) = self.proc_row_at(m.column, m.row) {
                    self.followed_pid = None;
                    self.select_visible(vis);
                }
            }
            _ => {
                if let Some(action) = mouse_to_action(m) {
                    self.apply(action);
                }
            }
        }
    }

    /// Translate mouse input over a menu into the existing keyboard reducer.
    /// Hit geometry follows `draw_main_menu` and `draw_options`, using the
    /// current terminal area so centered popups remain clickable after resize.
    pub fn menu_mouse_key(
        &mut self,
        m: crossterm::event::MouseEvent,
        area: Rect,
    ) -> Option<KeyEvent> {
        use crossterm::event::{KeyModifiers, MouseButton};

        if self.menu.is_none()
            || self.disable_mouse
            || self.pending_signal.is_some()
            || self.signal_picker.is_some()
            || self.nice_picker.is_some()
            || self.show_help
        {
            return None;
        }

        let key = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
        match m.kind {
            MouseEventKind::ScrollUp => return key(KeyCode::Up),
            MouseEventKind::ScrollDown => return key(KeyCode::Down),
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return None,
        }

        let position = ratatui::layout::Position::new(m.column, m.row);
        match self.menu.expect("menu was checked above") {
            Menu::Main { .. } => {
                let popup = centered(area, 36, 13);
                if !popup.contains(position) {
                    return key(KeyCode::Esc);
                }
                let inner = Rect {
                    x: popup.x.saturating_add(1),
                    y: popup.y.saturating_add(1),
                    width: popup.width.saturating_sub(2),
                    height: popup.height.saturating_sub(2),
                };
                for (index, _) in MAIN_MENU_ITEMS.iter().enumerate() {
                    let row = inner.y.saturating_add(4 + index as u16 * 2);
                    let hint_row = inner.y.saturating_add(inner.height.saturating_sub(1));
                    if m.row == row
                        && m.column >= inner.x
                        && m.column < inner.x.saturating_add(inner.width)
                        && row < inner.y.saturating_add(inner.height)
                        && row != hint_row
                    {
                        self.menu = Some(Menu::Main { selected: index });
                        return key(KeyCode::Enter);
                    }
                }
                None
            }
            Menu::Options { .. } => {
                let popup = centered(area, 64, OPTION_ROWS.len() as u16 + 7);
                if !popup.contains(position) {
                    return key(KeyCode::Esc);
                }
                let inner = Rect {
                    x: popup.x.saturating_add(1),
                    y: popup.y.saturating_add(1),
                    width: popup.width.saturating_sub(2),
                    height: popup.height.saturating_sub(2),
                };
                for (index, _) in OPTION_ROWS.iter().enumerate() {
                    let row = inner.y.saturating_add(1 + index as u16);
                    if m.row != row
                        || row >= inner.y.saturating_add(inner.height)
                        || m.column < inner.x
                        || m.column >= inner.x.saturating_add(inner.width)
                    {
                        continue;
                    }
                    self.menu = Some(Menu::Options { selected: index });
                    let offset = m.column.saturating_sub(inner.x);
                    // draw_options renders: " {label:<18}◂ {value:^10} ▸"
                    return match offset {
                        19..=25 => key(KeyCode::Left),
                        26..=32 => key(KeyCode::Right),
                        _ => None,
                    };
                }
                None
            }
        }
    }

    /// Map an absolute click (col,row) to a visible process index, or None if
    /// the click is outside the data rows of the process table.
    pub fn proc_row_at(&self, col: u16, row: u16) -> Option<usize> {
        let z = self.proc_hit.get();
        if z.rows == 0 || col < z.x0 || col >= z.x1 || row < z.data_y0 {
            return None;
        }
        let offset = row - z.data_y0;
        if offset >= z.rows {
            return None;
        }
        Some(z.scroll as usize + offset as usize)
    }

    /// Select a visible process by index, clamped to the visible count.
    pub fn select_visible(&mut self, vis_index: usize) {
        let max = self.visible_processes().len().saturating_sub(1);
        self.selected_process = vis_index.min(max);
    }

    /// Processes visible after applying the current filter
    pub fn visible_processes(&self) -> Vec<usize> {
        let procs = &self.snapshot.processes;
        if self.filter.is_empty() {
            return (0..procs.len()).collect();
        }
        // btop: a leading `!` makes the filter a (case-insensitive) regex.
        // Plain filters are case-insensitive substrings. Both match the
        // program name, full command line, user and pid.
        let matcher: Box<dyn Fn(&str) -> bool> = match self.filter.strip_prefix('!') {
            Some(pattern) => match regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
            {
                Ok(re) => Box::new(move |s: &str| re.is_match(s)),
                Err(_) => return Vec::new(),
            },
            None => {
                let needle = self.filter.to_lowercase();
                Box::new(move |s: &str| s.to_lowercase().contains(&needle))
            }
        };
        procs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                matcher(&p.name)
                    || matcher(&p.command)
                    || p.user.as_deref().is_some_and(&matcher)
                    || matcher(&p.pid.to_string())
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// True when the filter is a `!regex` that does not compile.
    pub fn filter_error(&self) -> bool {
        self.filter
            .strip_prefix('!')
            .is_some_and(|p| regex::Regex::new(p).is_err())
    }

    /// btop F: pin the selected process so the cursor tracks its pid.
    pub fn toggle_follow(&mut self) {
        if self.followed_pid.take().is_none() {
            let order = self.display_indices();
            self.followed_pid = order
                .get(self.selected_process)
                .map(|&i| self.snapshot.processes[i].pid);
        }
    }

    /// Move the selection onto the followed pid; drop follow if it exited.
    fn sync_follow(&mut self) {
        let Some(pid) = self.followed_pid else { return };
        let order = self.display_indices();
        match order
            .iter()
            .position(|&i| self.snapshot.processes[i].pid == pid)
        {
            Some(pos) => self.selected_process = pos,
            None => self.followed_pid = None,
        }
    }

    /// Process cpu% as displayed: btop per-core (can exceed 100) or share of
    /// the whole machine (divided by core count).
    pub fn proc_cpu(&self, raw: f32) -> f32 {
        if self.proc_per_core {
            raw
        } else {
            raw / self.snapshot.cpu.per_core_percent.len().max(1) as f32
        }
    }

    pub fn record_snapshot(&mut self) {
        self.history.record(&self.snapshot);
    }

    /// The order processes are DISPLAYED in — tree order when tree mode is on,
    /// otherwise the sorted order. Selection indices always refer to THIS list,
    /// so signal targeting and rendering never diverge.
    pub fn display_indices(&self) -> Vec<usize> {
        self.display_rows().into_iter().map(|r| r.0).collect()
    }

    /// Displayed rows as (snapshot index, tree depth, has children). Tree
    /// mode walks parent→child and skips descendants of collapsed pids; list
    /// mode is the sorted order at depth 0. Filter applies to both.
    pub fn display_rows(&self) -> Vec<(usize, usize, bool)> {
        if !self.tree {
            return self
                .sorted_process_indices()
                .into_iter()
                .map(|i| (i, 0, false))
                .collect();
        }
        let procs = &self.snapshot.processes;
        let visible: std::collections::HashSet<usize> =
            self.visible_processes().into_iter().collect();
        let parents: std::collections::HashSet<u32> =
            procs.iter().filter_map(|p| p.parent_pid).collect();
        let mut rows = Vec::new();
        // Depth of the collapsed ancestor currently hiding rows, if any.
        let mut hide_below: Option<usize> = None;
        for (i, depth) in tree_order(procs) {
            if let Some(d) = hide_below {
                if depth > d {
                    continue;
                }
                hide_below = None;
            }
            let pid = procs[i].pid;
            if self.collapsed.contains(&pid) {
                hide_below = Some(depth);
            }
            if visible.contains(&i) {
                rows.push((i, depth, parents.contains(&pid)));
            }
        }
        rows
    }

    /// btop space / + / -: collapse or expand the selected tree node.
    /// `want` = Some(true) collapse, Some(false) expand, None toggle.
    fn set_collapsed(&mut self, want: Option<bool>) {
        let rows = self.display_rows();
        let Some(&(i, _, has_kids)) = rows.get(self.selected_process) else {
            return;
        };
        if !has_kids {
            return;
        }
        let pid = self.snapshot.processes[i].pid;
        let collapse = want.unwrap_or(!self.collapsed.contains(&pid));
        if collapse {
            self.collapsed.insert(pid);
        } else {
            self.collapsed.remove(&pid);
        }
    }

    /// btop C: collapse (or expand, if all are collapsed) every child of
    /// the selected process that has children of its own.
    fn toggle_collapse_children(&mut self) {
        let rows = self.display_rows();
        let Some(&(i, _, _)) = rows.get(self.selected_process) else {
            return;
        };
        let pid = self.snapshot.processes[i].pid;
        let parents: std::collections::HashSet<u32> = self
            .snapshot
            .processes
            .iter()
            .filter_map(|p| p.parent_pid)
            .collect();
        let kids: Vec<u32> = self
            .snapshot
            .processes
            .iter()
            .filter(|p| p.parent_pid == Some(pid) && parents.contains(&p.pid))
            .map(|p| p.pid)
            .collect();
        if kids.iter().all(|k| self.collapsed.contains(k)) {
            for k in &kids {
                self.collapsed.remove(k);
            }
        } else {
            self.collapsed.extend(kids);
        }
    }

    /// btop E: collapse every parent, or expand all if anything is collapsed.
    fn toggle_collapse_all(&mut self) {
        if self.collapsed.is_empty() {
            self.collapsed = self
                .snapshot
                .processes
                .iter()
                .filter_map(|p| p.parent_pid)
                .collect();
            self.selected_process = 0;
        } else {
            self.collapsed.clear();
        }
    }

    /// Snapshot indices sorted by the current column/direction (btop view sort).
    pub fn sorted_process_indices(&self) -> Vec<usize> {
        let mut idx = self.visible_processes();
        let procs = &self.snapshot.processes;
        idx.sort_by(|&a, &b| {
            let (l, r) = (&procs[a], &procs[b]);
            let primary = match self.sort {
                ProcessSort::Cpu => r.cpu_percent.total_cmp(&l.cpu_percent),
                ProcessSort::Memory => r.memory_bytes.cmp(&l.memory_bytes),
                ProcessSort::Pid => l.pid.cmp(&r.pid),
                ProcessSort::Name => l.name.cmp(&r.name),
            };
            let ordered = primary
                .then_with(|| l.name.cmp(&r.name))
                .then_with(|| l.pid.cmp(&r.pid));
            if self.sort_reverse {
                ordered.reverse()
            } else {
                ordered
            }
        });
        idx
    }

    /// Cycle sort column Cpu->Memory->Pid->Name->Cpu (forward) or reverse.
    pub fn cycle_sort(&mut self, forward: bool) {
        let order = [
            ProcessSort::Cpu,
            ProcessSort::Memory,
            ProcessSort::Pid,
            ProcessSort::Name,
        ];
        let cur = order.iter().position(|s| *s == self.sort).unwrap_or(0);
        let n = order.len();
        let next = if forward {
            (cur + 1) % n
        } else {
            (cur + n - 1) % n
        };
        self.sort = order[next].clone();
        self.selected_process = 0;
        self.proc_scroll = 0;
    }

    pub fn toggle_sort_reverse(&mut self) {
        self.sort_reverse = !self.sort_reverse;
    }

    /// Public entry to the reducer (returns true if the app should quit).
    /// Used by the event loop and tests.
    pub fn apply_public(&mut self, action: Action) -> bool {
        self.apply(action)
    }

    /// Route a raw key event through the full input state machine: signal
    /// picker → confirmation overlay → filter input → global actions. This is
    /// the single source of truth for key handling, shared by `run_tui` and
    /// tests, so filter-mode text capture can never diverge from live behavior.
    pub fn feed_key(&mut self, key: KeyEvent) -> KeyOutcome {
        use crossterm::event::KeyModifiers;

        // 0. ctrl+z suspends from anywhere, like a shell job.
        if key.code == KeyCode::Char('z') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyOutcome::Suspend;
        }
        if key.code == KeyCode::Char('r')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && self.menu.is_none()
            && !self.filter_active
        {
            return KeyOutcome::ReloadConfig;
        }

        // 1. Signal picker owns the keyboard while open.
        if self.signal_picker.is_some() {
            match key.code {
                KeyCode::Char(c) if c.is_ascii_digit() => self.signal_picker_input(c),
                KeyCode::Enter => self.signal_picker_confirm(),
                KeyCode::Esc | KeyCode::Char('q') => self.signal_picker_cancel(),
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        // 1b. Nice picker: digits and a leading '-' only.
        if let Some(buf) = self.nice_picker.as_mut() {
            match key.code {
                KeyCode::Char('-') if buf.is_empty() => buf.push('-'),
                KeyCode::Char(c) if c.is_ascii_digit() && buf.len() < 3 => buf.push(c),
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Esc => self.nice_picker = None,
                KeyCode::Enter => {
                    let value = self.nice_picker.take().and_then(|b| b.parse::<i32>().ok());
                    let pid = self.selected_pid();
                    if let (Some(n), Some(pid)) = (value, pid) {
                        if (-20..=19).contains(&n) {
                            self.pending_nice = Some((pid, n));
                            return KeyOutcome::DispatchRenice;
                        }
                    }
                }
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        // 2. Confirmation overlay owns the keyboard while staged.
        if self.pending_signal.is_some() {
            match key.code {
                KeyCode::Enter => return KeyOutcome::DispatchSignal,
                KeyCode::Esc | KeyCode::Char('q') => self.cancel_signal(),
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        // 3. Filter input mode captures ALL printable keys as text (btop),
        //    so letters that also bind actions (e/t/s/2/R/K…) edit the filter.
        if self.filter_active {
            match key.code {
                KeyCode::Enter => self.filter_active = false, // commit, keep text
                KeyCode::Esc => {
                    self.filter_active = false;
                    self.filter.clear();
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => self.filter.push(c),
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        // 4. Menus own the keyboard while open.
        if let Some(menu) = self.menu {
            return self.menu_key(menu, key);
        }

        // 5. Esc closes the help overlay before it can open the menu.
        if self.show_help && matches!(key.code, KeyCode::Esc) {
            self.show_help = false;
            return KeyOutcome::Continue;
        }

        match key.code {
            KeyCode::Tab => {
                self.page = self.page.next();
                return KeyOutcome::Continue;
            }
            KeyCode::BackTab => {
                self.page = self.page.previous();
                return KeyOutcome::Continue;
            }
            _ => {}
        }

        // 6. Tree view reuses space/+/- for collapse (btop), E for all.
        if self.tree && key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
            match key.code {
                KeyCode::Char(' ') => {
                    self.set_collapsed(None);
                    return KeyOutcome::Continue;
                }
                KeyCode::Char('-') => {
                    self.set_collapsed(Some(true));
                    return KeyOutcome::Continue;
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.set_collapsed(Some(false));
                    return KeyOutcome::Continue;
                }
                KeyCode::Char('E') => {
                    self.toggle_collapse_all();
                    return KeyOutcome::Continue;
                }
                KeyCode::Char('C') => {
                    self.toggle_collapse_children();
                    return KeyOutcome::Continue;
                }
                _ => {}
            }
        }

        // 7. Global actions.
        let _ = KeyModifiers::NONE; // (modifiers handled inside key_to_action)
        if let Some(action) = key_to_action_with(key, self.vim_keys) {
            match action {
                Action::RequestTerminate => {
                    self.request_signal(Signal::Term);
                    KeyOutcome::Continue
                }
                Action::RequestKill => {
                    self.request_signal(Signal::Kill);
                    KeyOutcome::Continue
                }
                _ => {
                    let wants_wire = matches!(
                        action,
                        Action::RefreshNow
                            | Action::TogglePause
                            | Action::IncreaseInterval
                            | Action::DecreaseInterval
                    );
                    if self.apply(action) {
                        KeyOutcome::Quit
                    } else if wants_wire {
                        KeyOutcome::WireRefresh
                    } else {
                        KeyOutcome::Continue
                    }
                }
            }
        } else {
            KeyOutcome::Continue
        }
    }

    /// Keyboard handling while a menu is open. Up/Down (and j/k) move with
    /// wrap-around, Enter activates, Esc/m (main) or Esc/o/F2 (options) close.
    fn menu_key(&mut self, menu: Menu, key: KeyEvent) -> KeyOutcome {
        let step = |sel: usize, len: usize, down: bool| {
            if down {
                (sel + 1) % len
            } else {
                (sel + len - 1) % len
            }
        };
        match menu {
            Menu::Main { selected } => {
                let len = MAIN_MENU_ITEMS.len();
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                        self.menu = Some(Menu::Main {
                            selected: step(selected, len, true),
                        });
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                        self.menu = Some(Menu::Main {
                            selected: step(selected, len, false),
                        });
                    }
                    KeyCode::Esc | KeyCode::Char('m') => self.menu = None,
                    KeyCode::Char('q') => return KeyOutcome::Quit,
                    KeyCode::Enter | KeyCode::Char(' ') => match selected {
                        0 => self.menu = Some(Menu::Options { selected: 0 }),
                        1 => {
                            self.menu = None;
                            self.show_help = true;
                        }
                        _ => return KeyOutcome::Quit,
                    },
                    _ => {}
                }
                KeyOutcome::Continue
            }
            Menu::Options { selected } => {
                let len = OPTION_ROWS.len();
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                        self.menu = Some(Menu::Options {
                            selected: step(selected, len, true),
                        });
                        KeyOutcome::Continue
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                        self.menu = Some(Menu::Options {
                            selected: step(selected, len, false),
                        });
                        KeyOutcome::Continue
                    }
                    KeyCode::Esc | KeyCode::Char('o') | KeyCode::F(2) => {
                        self.menu = None;
                        KeyOutcome::Continue
                    }
                    KeyCode::Char('q') => KeyOutcome::Quit,
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                        self.adjust_option(selected, true)
                    }
                    KeyCode::Left | KeyCode::Char('h') => self.adjust_option(selected, false),
                    _ => KeyOutcome::Continue,
                }
            }
        }
    }

    /// Change one options row. Booleans toggle in either direction.
    fn adjust_option(&mut self, row: usize, forward: bool) -> KeyOutcome {
        match row {
            0 => {
                let names = self.available_themes();
                let n = names.len();
                let cur = names
                    .iter()
                    .position(|t| *t == self.theme_name)
                    .unwrap_or(0);
                let next = if forward {
                    (cur + 1) % n
                } else {
                    (cur + n - 1) % n
                };
                let name = names[next].clone();
                self.set_theme(&name);
                KeyOutcome::Continue
            }
            1 => {
                let action = if forward {
                    Action::IncreaseInterval
                } else {
                    Action::DecreaseInterval
                };
                self.apply(action);
                KeyOutcome::WireRefresh
            }
            2 => {
                self.vim_keys = !self.vim_keys;
                KeyOutcome::Continue
            }
            3 => {
                self.show_cores = !self.show_cores;
                KeyOutcome::Continue
            }
            4 => {
                self.show_gpu = !self.show_gpu;
                KeyOutcome::Continue
            }
            5 => {
                self.apply(Action::ToggleTree);
                KeyOutcome::Continue
            }
            6 => {
                self.cpu_bottom = !self.cpu_bottom;
                KeyOutcome::Continue
            }
            7 => {
                self.proc_left = !self.proc_left;
                KeyOutcome::Continue
            }
            8 => {
                self.mem_below_net = !self.mem_below_net;
                KeyOutcome::Continue
            }
            9 => {
                const ORDER: [GraphSymbol; 3] =
                    [GraphSymbol::Braille, GraphSymbol::Block, GraphSymbol::Tty];
                let cur = ORDER
                    .iter()
                    .position(|s| *s == self.graph_symbol)
                    .unwrap_or(0);
                let next = if forward {
                    (cur + 1) % 3
                } else {
                    (cur + 2) % 3
                };
                self.graph_symbol = ORDER[next];
                KeyOutcome::Continue
            }
            10 => {
                const SCALES: [&str; 3] = ["celsius", "fahrenheit", "kelvin"];
                let current = SCALES
                    .iter()
                    .position(|scale| *scale == self.temp_scale)
                    .unwrap_or(0);
                let next = if forward {
                    (current + 1) % SCALES.len()
                } else {
                    (current + SCALES.len() - 1) % SCALES.len()
                };
                self.temp_scale = SCALES[next].to_owned();
                KeyOutcome::Continue
            }
            _ => KeyOutcome::Continue,
        }
    }

    /// Name of the interface shown in the net box: the user's pick if it
    /// still exists, else the first reported interface.
    pub fn net_iface(&self) -> Option<&str> {
        let nets = &self.snapshot.networks;
        self.net_selected
            .as_deref()
            .filter(|want| nets.iter().any(|n| n.interface == *want))
            .or_else(|| nets.first().map(|n| n.interface.as_str()))
    }

    fn current_net(&self) -> Option<&crate::model::NetworkSnapshot> {
        let name = self.net_iface()?;
        self.snapshot.networks.iter().find(|n| n.interface == name)
    }

    /// btop b/n: step to the previous/next interface, wrapping.
    pub fn cycle_net(&mut self, forward: bool) {
        let names: Vec<String> = self
            .snapshot
            .networks
            .iter()
            .map(|n| n.interface.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        let cur = self
            .net_iface()
            .and_then(|c| names.iter().position(|n| n == c))
            .unwrap_or(0);
        let len = names.len();
        let next = if forward {
            (cur + 1) % len
        } else {
            (cur + len - 1) % len
        };
        self.net_selected = Some(names[next].clone());
    }

    /// btop z: toggle "totals since now" for the current interface.
    pub fn toggle_net_zero(&mut self) {
        let Some(n) = self.current_net() else { return };
        let key = n.interface.clone();
        let base = (
            n.received_bytes_total.unwrap_or(0),
            n.transmitted_bytes_total.unwrap_or(0),
        );
        if self.net_zero.remove(&key).is_none() {
            self.net_zero.insert(key, base);
        }
    }

    /// (rx total, tx total) for the current interface, minus any z baseline.
    pub fn net_totals(&self) -> (Option<u64>, Option<u64>) {
        let Some(n) = self.current_net() else {
            return (None, None);
        };
        let (brx, btx) = self.net_zero.get(&n.interface).copied().unwrap_or((0, 0));
        (
            n.received_bytes_total.map(|t| t.saturating_sub(brx)),
            n.transmitted_bytes_total.map(|t| t.saturating_sub(btx)),
        )
    }

    /// Graph ceilings (rx, tx) in bytes/s. Auto: history max; manual: btop's
    /// default 100 Mbit. Sync makes both graphs share the larger ceiling.
    pub fn net_scale(&self) -> (u64, u64) {
        // btop net_download/net_upload are Mebibits; history is bytes/s.
        const MIBIT: u64 = 1024 * 1024 / 8;
        let (rx, tx) = if self.net_auto {
            let iface = self.net_iface().unwrap_or("");
            let max = |v: Vec<u64>| v.into_iter().max().unwrap_or(1).max(1);
            (
                max(self.history.net_rx(iface)),
                max(self.history.net_tx(iface)),
            )
        } else {
            (
                self.net_download_mbit.max(1) * MIBIT,
                self.net_upload_mbit.max(1) * MIBIT,
            )
        };
        if self.net_sync {
            let m = rx.max(tx);
            (m, m)
        } else {
            (rx, tx)
        }
    }

    /// Restore persisted settings (btop reads btop.conf on start).
    pub fn apply_config(&mut self, cfg: &crate::config::Config) {
        use crate::config::ThemeName;
        self.refresh_interval = Duration::from_millis(cfg.interval_ms.clamp(100, 10_000));
        let builtin = match cfg.theme {
            ThemeName::Neon => "neon",
            ThemeName::Amber => "amber",
            ThemeName::Mono => "mono",
        };
        let name = cfg
            .color_theme
            .as_deref()
            .filter(|n| self.available_themes().iter().any(|a| a == n))
            .unwrap_or(builtin)
            .to_owned();
        self.set_theme(&name);
        self.vim_keys = cfg.vim_keys;
        self.show_cores = cfg.show_cores;
        self.tree = cfg.tree;
        self.proc_per_core = cfg.proc_per_core;
        self.net_auto = cfg.net_auto;
        self.net_sync = cfg.net_sync;
        self.cpu_bottom = cfg.cpu_bottom;
        self.proc_left = cfg.proc_left;
        self.mem_below_net = cfg.mem_below_net;
        self.graph_symbol = GraphSymbol::parse(&cfg.graph_symbol).unwrap_or_default();
        self.show_uptime = cfg.show_uptime;
        self.show_battery = cfg.show_battery;
        self.check_temp = cfg.check_temp;
        self.show_cpu_freq = cfg.show_cpu_freq;
        self.disable_mouse = cfg.disable_mouse;
        self.custom_cpu_name = cfg.custom_cpu_name.clone();
        self.temp_scale = cfg.temp_scale.clone();
        self.clock_format = cfg.clock_format.clone();
        self.sort_reverse = cfg.proc_reversed;
        self.proc_mem_percent = !cfg.proc_mem_bytes;
        self.proc_colors = cfg.proc_colors;
        self.proc_gradient = cfg.proc_gradient;
        self.proc_aggregate = cfg.proc_aggregate;
        self.show_swap = cfg.show_swap;
        self.mem_graphs = cfg.mem_graphs;
        self.show_disks = cfg.show_disks;
        self.disk_io_mode = cfg.io_mode;
        self.disks_filter = cfg.disks_filter.clone();
        self.show_coretemp = cfg.show_coretemp;
        self.swap_upload_download = cfg.swap_upload_download;
        self.net_download_mbit = cfg.net_download;
        self.net_upload_mbit = cfg.net_upload;
        self.truecolor = cfg.truecolor;
        self.force_tty = cfg.force_tty;
        if cfg.force_tty {
            self.graph_symbol = GraphSymbol::Tty;
        }
        if !cfg.net_iface.is_empty() {
            self.net_selected = Some(cfg.net_iface.clone());
        }
        STYLE.with(|s| {
            s.set(RenderStyle {
                // btop: rounded_corners is ignored in tty mode.
                square_corners: !cfg.rounded_corners || cfg.force_tty,
                transparent_bg: !cfg.theme_background,
            })
        });
        self.box_symbols.clear();
        for (name, value) in [
            ("cpu", &cfg.graph_symbol_cpu),
            ("mem", &cfg.graph_symbol_mem),
            ("net", &cfg.graph_symbol_net),
            ("proc", &cfg.graph_symbol_proc),
            ("gpu", &cfg.graph_symbol_gpu),
        ] {
            if let Some(sym) = GraphSymbol::parse(value) {
                self.box_symbols.insert(name, sym);
            }
        }
        self.cfg_box_symbols = self.box_symbols.clone();
        self.preset_groups = preset_entries(&cfg.presets)
            .into_iter()
            .map(|(_, g)| g)
            .collect();
        self.presets = parse_presets(&cfg.presets);
        let has = |b: &str| cfg.boxes.split_whitespace().any(|x| x == b);
        self.show_cpu = has("cpu");
        self.show_mem = has("mem");
        self.show_net = has("net");
        self.show_proc = has("proc");
        self.show_gpu = has("gpu") && cfg.show_gpu;
    }

    /// Current settings as a config, keeping non-view fields from `base`.
    pub fn to_config(&self, base: &crate::config::Config) -> crate::config::Config {
        use crate::config::ThemeName;
        let boxes: Vec<&str> = [
            ("cpu", self.show_cpu),
            ("mem", self.show_mem),
            ("net", self.show_net),
            ("proc", self.show_proc),
            ("gpu", self.show_gpu),
        ]
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(n, _)| n)
        .collect();
        crate::config::Config {
            interval_ms: self.refresh_interval.as_millis() as u64,
            theme: match self.theme_name.as_str() {
                "amber" => ThemeName::Amber,
                "mono" => ThemeName::Mono,
                _ => ThemeName::Neon,
            },
            vim_keys: self.vim_keys,
            show_cores: self.show_cores,
            tree: self.tree,
            proc_per_core: self.proc_per_core,
            net_auto: self.net_auto,
            net_sync: self.net_sync,
            cpu_bottom: self.cpu_bottom,
            proc_left: self.proc_left,
            mem_below_net: self.mem_below_net,
            // force_tty overrides symbol/corners at runtime only; keep the
            // user's own choice in the file.
            graph_symbol: if self.force_tty {
                base.graph_symbol.clone()
            } else {
                self.graph_symbol.name().to_owned()
            },
            show_uptime: self.show_uptime,
            show_battery: self.show_battery,
            check_temp: self.check_temp,
            show_cpu_freq: self.show_cpu_freq,
            disable_mouse: self.disable_mouse,
            custom_cpu_name: self.custom_cpu_name.clone(),
            temp_scale: self.temp_scale.clone(),
            clock_format: self.clock_format.clone(),
            net_iface: self.net_selected.clone().unwrap_or_default(),
            rounded_corners: if self.force_tty {
                base.rounded_corners
            } else {
                !STYLE.with(|s| s.get()).square_corners
            },
            theme_background: !STYLE.with(|s| s.get()).transparent_bg,
            proc_reversed: self.sort_reverse,
            proc_mem_bytes: !self.proc_mem_percent,
            proc_colors: self.proc_colors,
            proc_gradient: self.proc_gradient,
            proc_aggregate: self.proc_aggregate,
            show_swap: self.show_swap,
            mem_graphs: self.mem_graphs,
            show_disks: self.show_disks,
            io_mode: self.disk_io_mode,
            disks_filter: self.disks_filter.clone(),
            show_coretemp: self.show_coretemp,
            swap_upload_download: self.swap_upload_download,
            net_download: self.net_download_mbit,
            net_upload: self.net_upload_mbit,
            truecolor: self.truecolor,
            force_tty: self.force_tty,
            graph_symbol_cpu: self.box_symbol_name("cpu"),
            graph_symbol_mem: self.box_symbol_name("mem"),
            graph_symbol_net: self.box_symbol_name("net"),
            graph_symbol_proc: self.box_symbol_name("proc"),
            graph_symbol_gpu: self.box_symbol_name("gpu"),
            boxes: boxes.join(" "),
            color_theme: (!THEME_NAMES.contains(&self.theme_name.as_str()))
                .then(|| self.theme_name.clone()),
            ..base.clone()
        }
    }

    /// btop p / P: step through presets, showing exactly that box set.
    pub fn cycle_preset(&mut self, forward: bool) {
        let n = self.presets.len().max(1);
        self.preset = if forward {
            (self.preset + 1) % n
        } else {
            (self.preset + n - 1) % n
        };
        let set = self.presets.get(self.preset).cloned().unwrap_or_default();
        let has = |b: &str| set.contains(&b);
        self.show_cpu = has("cpu");
        self.show_mem = has("mem");
        self.show_net = has("net");
        self.show_proc = has("proc");
        self.show_gpu = has("gpu");
        // Preset 0 restores btop's default positions; others carry P flags.
        let group = self
            .preset_groups
            .get(self.preset)
            .cloned()
            .unwrap_or_default();
        (self.cpu_bottom, self.proc_left, self.mem_below_net) = parse_preset_positions(&group);
        // Preset G field: "cpu:0:block" sets that box's graph symbol;
        // "default" (or preset 0) falls back to the global graph_symbol.
        self.box_symbols = self.cfg_box_symbols.clone();
        for entry in group.split(',') {
            let mut it = entry.split(':');
            let (Some(name), _, Some(sym)) = (it.next(), it.next(), it.next()) else {
                continue;
            };
            if let (Some(b), Some(s)) = (
                ALL_BOXES.iter().copied().find(|b| *b == name),
                GraphSymbol::parse(sym),
            ) {
                self.box_symbols.insert(b, s);
            }
        }
    }

    /// Effective graph symbol for a box: per-box override, else global.
    pub fn box_symbol(&self, name: &str) -> GraphSymbol {
        self.box_symbols
            .get(name)
            .copied()
            .unwrap_or(self.graph_symbol)
    }

    /// btop disks_filter: space-separated mount points to show, or
    /// "exclude=<mounts>" to hide them. Empty shows every disk.
    pub fn filtered_disks(&self) -> Vec<&crate::model::DiskSnapshot> {
        let f = self.disks_filter.trim();
        let (exclude, list) = match f.strip_prefix("exclude=") {
            Some(rest) => (true, rest),
            None => (false, f),
        };
        let mounts: Vec<&str> = list.split_whitespace().collect();
        self.snapshot
            .disks
            .iter()
            .filter(|d| mounts.is_empty() || mounts.contains(&d.mount_point.as_str()) != exclude)
            .collect()
    }

    /// Process cpu% as shown: btop proc_aggregate (or a collapsed tree
    /// node) adds every descendant's usage to the parent.
    pub fn shown_cpu(&self, idx: usize) -> f32 {
        let p = &self.snapshot.processes[idx];
        let sum = self.tree && (self.proc_aggregate || self.collapsed.contains(&p.pid));
        let raw = if sum {
            self.subtree_cpu(p.pid)
        } else {
            p.cpu_percent
        };
        self.proc_cpu(raw)
    }

    fn subtree_cpu(&self, root: u32) -> f32 {
        let procs = &self.snapshot.processes;
        let mut total = 0.0;
        let mut stack = vec![root];
        let mut seen = std::collections::HashSet::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            for p in procs {
                if p.pid == pid {
                    total += p.cpu_percent;
                } else if p.parent_pid == Some(pid) {
                    stack.push(p.pid);
                }
            }
        }
        total
    }

    fn box_symbol_name(&self, name: &str) -> String {
        self.cfg_box_symbols
            .get(name)
            .map(|s| s.name().to_owned())
            .unwrap_or_else(|| "default".to_owned())
    }

    /// btop ctrl+r: re-read `path` and apply it. A broken file leaves the
    /// current settings untouched and reports the error in the footer.
    pub fn reload_config(&mut self, path: &std::path::Path) {
        match crate::config::Config::load(Some(path)) {
            Ok(cfg) => {
                self.apply_config(&cfg);
                self.status_msg = Some("config reloaded".to_owned());
            }
            Err(e) => self.status_msg = Some(format!("config error: {e}")),
        }
    }

    /// Switch the live palette and remember the name for the options panel.
    pub fn set_theme(&mut self, name: &str) {
        self.theme_name = name.to_owned();
        self.theme = Theme::from_name(name);
        let user = if THEME_NAMES.contains(&name) {
            None
        } else {
            self.theme_dir
                .as_deref()
                .and_then(|d| load_theme_file(d, name))
        };
        set_palette(user.unwrap_or_else(|| Palette::from_name(name)));
    }

    /// Built-in themes followed by any `.theme` files in the theme dir.
    pub fn available_themes(&self) -> Vec<String> {
        let mut names: Vec<String> = THEME_NAMES.iter().map(|s| s.to_string()).collect();
        if let Some(dir) = &self.theme_dir {
            names.extend(list_theme_files(dir));
        }
        names
    }

    /// Current value of an options row, as shown in the panel.
    pub fn option_value(&self, row: usize) -> String {
        let flag = |b: bool| if b { "On" } else { "Off" }.to_owned();
        match row {
            0 => self.theme_name.clone(),
            1 => self.refresh_interval.as_millis().to_string(),
            2 => flag(self.vim_keys),
            3 => flag(self.show_cores),
            4 => flag(self.show_gpu),
            5 => flag(self.tree),
            6 => flag(self.cpu_bottom),
            7 => flag(self.proc_left),
            8 => flag(self.mem_below_net),
            9 => self.graph_symbol.name().to_owned(),
            10 => match self.temp_scale.as_str() {
                "fahrenheit" => "°F".to_owned(),
                "kelvin" => "K".to_owned(),
                _ => "°C".to_owned(),
            },
            _ => String::new(),
        }
    }

    /// Stage a signal for the currently selected process (btop confirm box).
    pub fn request_signal(&mut self, sig: Signal) {
        let ordered = self.display_indices();
        if let Some(&snap_i) = ordered.get(self.selected_process) {
            let pid = self.snapshot.processes[snap_i].pid;
            self.pending_signal = Some((pid, sig));
        }
    }

    pub fn cancel_signal(&mut self) {
        self.pending_signal = None;
    }

    /// Open the signal picker with an empty number buffer.
    pub fn open_signal_picker(&mut self) {
        self.signal_picker = Some(String::new());
    }

    /// Feed a character into the signal picker (digits only, max 2).
    pub fn signal_picker_input(&mut self, c: char) {
        if let Some(buf) = self.signal_picker.as_mut() {
            if c.is_ascii_digit() && buf.len() < 2 {
                buf.push(c);
            }
        }
    }

    /// Close the picker; if the buffer is a valid signal number, stage a
    /// confirmation for the selected process. Invalid input just cancels.
    pub fn signal_picker_confirm(&mut self) {
        if let Some(buf) = self.signal_picker.take() {
            if let Ok(n) = buf.parse::<i32>() {
                if let Some(sig) = Signal::from_number(n) {
                    self.request_signal(sig);
                }
            }
        }
    }

    /// Discard the picker without staging anything.
    pub fn signal_picker_cancel(&mut self) {
        self.signal_picker = None;
    }

    /// Pid under the cursor in the displayed order.
    pub fn selected_pid(&self) -> Option<u32> {
        self.display_indices()
            .get(self.selected_process)
            .map(|&i| self.snapshot.processes[i].pid)
    }

    /// Apply the staged nice value; report the outcome in the footer.
    pub fn confirm_renice(&mut self, ctl: &dyn ProcessController) {
        if let Some((pid, nice)) = self.pending_nice.take() {
            self.status_msg = Some(match ctl.renice(pid, nice) {
                Ok(()) => format!("renice {pid} → {nice}"),
                Err(_) => format!("renice {pid} failed (lower values need root)"),
            });
        }
    }

    /// Dispatch the staged signal via the controller, then clear it.
    pub fn confirm_signal(&mut self, ctl: &dyn ProcessController) {
        if let Some((pid, sig)) = self.pending_signal.take() {
            let _ = ctl.send(pid, sig); // best-effort; btop ignores EPERM/ESRCH
        }
    }

    pub fn accept_snapshot(&mut self, snap: SystemSnapshot) -> bool {
        if snap.captured_at == self.snapshot.captured_at {
            return false;
        }
        let mut snap = snap;
        if self.proc_paused {
            // btop `u`: keep showing the frozen process list while every
            // other box keeps updating.
            snap.processes = std::mem::take(&mut self.snapshot.processes);
        }
        self.snapshot = snap;
        let vis = self.visible_processes().len();
        self.selected_process = self.selected_process.min(vis.saturating_sub(1));
        self.sync_follow();
        self.record_snapshot();
        true
    }
    fn apply(&mut self, action: Action) -> bool {
        // btop: moving the cursor by hand stops following.
        if matches!(
            action,
            Action::SelectNext
                | Action::SelectPrevious
                | Action::PageDown
                | Action::PageUp
                | Action::SelectFirst
                | Action::SelectLast
        ) {
            self.followed_pid = None;
        }
        match action {
            Action::Quit => return true,
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::TogglePause => self.paused = !self.paused,
            Action::IncreaseInterval => {
                self.refresh_interval = (self.refresh_interval + Duration::from_millis(100))
                    .min(Duration::from_secs(10));
            }
            Action::DecreaseInterval => {
                self.refresh_interval = self
                    .refresh_interval
                    .checked_sub(Duration::from_millis(100))
                    .unwrap_or(Duration::from_millis(100))
                    .max(Duration::from_millis(100));
            }
            Action::SelectNext => {
                let max = self.visible_processes().len().saturating_sub(1);
                self.selected_process = (self.selected_process + 1).min(max);
            }
            Action::SelectPrevious => {
                self.selected_process = self.selected_process.saturating_sub(1);
            }
            Action::PageDown => {
                let vis = self.visible_processes().len();
                self.selected_process = (self.selected_process + 15).min(vis.saturating_sub(1));
            }
            Action::PageUp => {
                self.selected_process = self.selected_process.saturating_sub(15);
            }
            Action::SelectFirst => {
                self.selected_process = 0;
            }
            Action::SelectLast => {
                self.selected_process = self.visible_processes().len().saturating_sub(1);
            }
            Action::ToggleDetail => {
                if self.show_help {
                    self.show_help = false;
                }
                self.show_detail = !self.show_detail;
            }
            Action::ToggleFilter => {
                if self.show_help {
                    self.show_help = false;
                }
                self.filter_active = !self.filter_active;
                if !self.filter_active {
                    self.filter.clear();
                }
            }
            Action::FilterChar(c) => {
                if self.filter_active {
                    self.filter.push(c);
                }
            }
            Action::FilterClear => {
                self.filter.clear();
            }
            Action::SortNext => self.cycle_sort(true),
            Action::SortPrev => self.cycle_sort(false),
            Action::ToggleTree => {
                self.tree = !self.tree;
                self.selected_process = 0;
                self.proc_scroll = 0;
            }
            Action::SignalPicker => self.open_signal_picker(),
            Action::ToggleReverse => self.toggle_sort_reverse(),
            Action::ToggleCores => self.show_cores = !self.show_cores,
            Action::ToggleFollow => self.toggle_follow(),
            Action::SortBy(sort) => {
                // btop: clicking the active column flips direction.
                if self.sort == sort {
                    self.sort_reverse = !self.sort_reverse;
                } else {
                    self.sort = sort;
                    self.sort_reverse = false;
                }
                self.selected_process = 0;
                self.proc_scroll = 0;
            }
            Action::ToggleProcPause => self.proc_paused = !self.proc_paused,
            Action::ToggleDisks => self.show_disks = !self.show_disks,
            Action::PresetNext => self.cycle_preset(true),
            Action::PresetPrev => self.cycle_preset(false),
            Action::NicePicker => {
                if self.selected_pid().is_some() {
                    self.nice_picker = Some(String::new());
                }
            }
            Action::ToggleDiskIo => self.disk_io_mode = !self.disk_io_mode,
            Action::TogglePerCoreProc => self.proc_per_core = !self.proc_per_core,
            Action::ToggleMemPercent => self.proc_mem_percent = !self.proc_mem_percent,
            Action::NetNext => self.cycle_net(true),
            Action::NetPrev => self.cycle_net(false),
            Action::NetZero => self.toggle_net_zero(),
            Action::NetAuto => self.net_auto = !self.net_auto,
            Action::NetSync => self.net_sync = !self.net_sync,
            Action::ToggleBox(n) => match n {
                1 => self.show_cpu = !self.show_cpu,
                2 => self.show_mem = !self.show_mem,
                3 => self.show_net = !self.show_net,
                4 => self.show_proc = !self.show_proc,
                5 => self.show_gpu = !self.show_gpu,
                _ => {}
            },
            Action::OpenMenu => {
                self.show_help = false;
                self.menu = Some(Menu::Main { selected: 0 });
            }
            Action::OpenOptions => {
                self.show_help = false;
                self.menu = Some(Menu::Options { selected: 0 });
            }
            // Signal actions are handled in the event loop (needs controller);
            // the reducer treats them as no-ops.
            Action::RequestTerminate
            | Action::RequestKill
            | Action::ConfirmSignal
            | Action::CancelSignal => {}
            Action::RefreshNow => {}
        }
        false
    }
}

fn hostname_or_default() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .and_then(|s| s.lines().next().map(str::to_owned))
        .unwrap_or_else(|| "localhost".to_owned())
}

/// Key → action with mtop's default layout (btop `vim_keys` on).
pub fn key_to_action(key: KeyEvent) -> Option<Action> {
    key_to_action_with(key, true)
}

/// Key → action following btop's keymap (btop_input.cpp / help_text).
/// `vim` mirrors btop's `vim_keys`: h/j/k/l/g/G navigate, which moves help to
/// `H` and kill to `K`. Printable filter text is handled by `feed_key`, never
/// here, so unbound letters map to `None`.
pub fn key_to_action_with(key: KeyEvent, vim: bool) -> Option<Action> {
    use crossterm::event::KeyModifiers;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl {
        return match key.code {
            KeyCode::Char('c') => Some(Action::Quit),
            // ctrl+r (config reload) is handled in feed_key as an outcome.
            _ => None,
        };
    }
    let (help_key, kill_key) = if vim { ('H', 'K') } else { ('h', 'k') };
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Esc | KeyCode::Char('m') => Some(Action::OpenMenu),
        KeyCode::Char('o') | KeyCode::F(2) => Some(Action::OpenOptions),
        KeyCode::F(1) | KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char(c) if c == help_key => Some(Action::ToggleHelp),
        KeyCode::F(5) => Some(Action::RefreshNow),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::IncreaseInterval),
        KeyCode::Char('-') => Some(Action::DecreaseInterval),
        // Process list navigation.
        KeyCode::Down => Some(Action::SelectNext),
        KeyCode::Up => Some(Action::SelectPrevious),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::Home => Some(Action::SelectFirst),
        KeyCode::End => Some(Action::SelectLast),
        KeyCode::Char('j') if vim => Some(Action::SelectNext),
        KeyCode::Char('k') if vim => Some(Action::SelectPrevious),
        KeyCode::Char('g') if vim => Some(Action::SelectFirst),
        KeyCode::Char('G') if vim => Some(Action::SelectLast),
        // Sorting column / direction.
        KeyCode::Left => Some(Action::SortPrev),
        KeyCode::Right => Some(Action::SortNext),
        KeyCode::Char('h') if vim => Some(Action::SortPrev),
        KeyCode::Char('l') if vim => Some(Action::SortNext),
        KeyCode::Char('r') | KeyCode::Char('R') => Some(Action::ToggleReverse),
        // Process box.
        KeyCode::Enter => Some(Action::ToggleDetail),
        KeyCode::Char('f') | KeyCode::Char('/') => Some(Action::ToggleFilter),
        KeyCode::Delete => Some(Action::FilterClear),
        KeyCode::Char('e') => Some(Action::ToggleTree),
        KeyCode::Char(' ') => Some(Action::TogglePause),
        KeyCode::Char('u') => Some(Action::ToggleProcPause),
        KeyCode::Char('t') => Some(Action::RequestTerminate),
        KeyCode::Char(c) if c == kill_key => Some(Action::RequestKill),
        KeyCode::Char('s') => Some(Action::SignalPicker),
        // Box toggles.
        // Process box extras (btop).
        KeyCode::Char('F') => Some(Action::ToggleFollow),
        KeyCode::Char('d') => Some(Action::ToggleDisks),
        KeyCode::Char('p') => Some(Action::PresetNext),
        KeyCode::Char('P') => Some(Action::PresetPrev),
        KeyCode::Char('N') => Some(Action::NicePicker),
        KeyCode::Char('i') => Some(Action::ToggleDiskIo),
        KeyCode::Char('c') => Some(Action::TogglePerCoreProc),
        KeyCode::Char('%') => Some(Action::ToggleMemPercent),
        // Net box: interface, totals reset, scaling (btop).
        KeyCode::Char('n') => Some(Action::NetNext),
        KeyCode::Char('b') => Some(Action::NetPrev),
        KeyCode::Char('z') => Some(Action::NetZero),
        KeyCode::Char('a') => Some(Action::NetAuto),
        KeyCode::Char('y') => Some(Action::NetSync),
        // Box toggles: 1 cpu, 2 mem, 3 net, 4 proc, 5 gpu (btop).
        KeyCode::Char('1') => Some(Action::ToggleBox(1)),
        KeyCode::Char('2') => Some(Action::ToggleBox(2)),
        KeyCode::Char('3') => Some(Action::ToggleBox(3)),
        KeyCode::Char('4') => Some(Action::ToggleBox(4)),
        KeyCode::Char('5') => Some(Action::ToggleBox(5)),
        _ => None,
    }
}

/// Map a mouse event to an action. Wheel scroll moves the process selection;
/// other events are ignored (btop uses the wheel over the proc list).
pub fn mouse_to_action(mouse: crossterm::event::MouseEvent) -> Option<Action> {
    match mouse.kind {
        MouseEventKind::ScrollDown => Some(Action::SelectNext),
        MouseEventKind::ScrollUp => Some(Action::SelectPrevious),
        _ => None,
    }
}
// Title format: " ²cpu " embedded in top border
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// btop createBox — rounded corners + title embedded in border
//
// From btop_draw.cpp lines 279–331:
//   round_left_up = ╭  round_right_up = ╮
//   round_left_down = ╰  round_right_down = ╯
//   title_left = ┐  title_right = ┌  (reversed to create the gap look)
//   h_line = ─   v_line = │
//
// Ratatui's Rounded border type gives us ╭╮╰╯ automatically.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// btop Symbols::superscript — matches btop_draw.cpp line 87
fn superscript(n: u8) -> &'static str {
    match n {
        0 => "⁰",
        1 => "¹",
        2 => "²",
        3 => "³",
        4 => "⁴",
        5 => "⁵",
        6 => "⁶",
        7 => "⁷",
        8 => "⁸",
        9 => "⁹",
        _ => "",
    }
}

// btop createBox: "╭──┤ ¹cpu ├──╮"
// Title format: "┤ ¹title ├" embedded by Ratatui at the rounded top-left.
fn btop_block_n<'a>(num: u8, title: &'a str, border_color: Color) -> Block<'a> {
    // We cannot center the title via Ratatui Block API, so we put superscript
    // inline. The ┤/├ bracket chars visually break the border line.
    let sup = superscript(num);
    Block::default()
        .title(format!("┤{sup}{title}├"))
        .title_style(Style::default().fg(c_title()).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(c_main_bg()))
}

fn btop_block<'a>(title: &'a str, border_color: Color) -> Block<'a> {
    Block::default()
        .title(format!("┤{title}├"))
        .title_style(Style::default().fg(c_title()).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(c_main_bg()))
}

fn style_bold(fg: Color) -> Style {
    Style::default().fg(fg).add_modifier(Modifier::BOLD)
}
fn style_fg(fg: Color) -> Style {
    Style::default().fg(fg)
}

/// Screen rects for each visible box (btop `Draw::calcSizes`).
#[derive(Clone, Copy, Debug, Default)]
struct BoxLayout {
    cpu: Option<Rect>,
    gpu: Option<Rect>,
    mem: Option<Rect>,
    disk: Option<Rect>,
    net: Option<Rect>,
    proc: Option<Rect>,
}

impl BoxLayout {
    fn is_empty(&self) -> bool {
        self.cpu.is_none()
            && self.gpu.is_none()
            && self.mem.is_none()
            && self.disk.is_none()
            && self.net.is_none()
            && self.proc.is_none()
    }
}

/// Lay out the visible boxes. The top row holds cpu (+gpu beside it); the
/// lower area holds a left column (mem, disks, net) and proc on the right.
/// Any hidden box hands its space to the remaining ones, like btop.
fn layout_boxes(body: Rect, view: &AppView) -> BoxLayout {
    let mut out = BoxLayout::default();
    let has_gpu = view.show_gpu && !view.snapshot.gpus.is_empty();
    let top_visible = view.show_cpu || has_gpu;
    let left_visible = view.show_mem || view.show_net;
    let lower_visible = left_visible || view.show_proc;

    let (top, lower) = match (top_visible, lower_visible) {
        (false, false) => return out,
        (false, true) => (None, Some(body)),
        (true, false) => (Some(body), None),
        (true, true) => {
            let top_h = ((body.height as u32 * 32 / 100) as u16)
                .max(8)
                .min(body.height.saturating_sub(6));
            if view.cpu_bottom {
                // btop cpu_bottom: the cpu row sits under the lower boxes.
                let rows =
                    Layout::vertical([Constraint::Min(6), Constraint::Length(top_h)]).split(body);
                (Some(rows[1]), Some(rows[0]))
            } else {
                let rows =
                    Layout::vertical([Constraint::Length(top_h), Constraint::Min(6)]).split(body);
                (Some(rows[0]), Some(rows[1]))
            }
        }
    };

    if let Some(top) = top {
        match (
            view.show_cpu,
            has_gpu && top.width >= 90 || has_gpu && !view.show_cpu,
        ) {
            (true, true) => {
                let gpu_w = (top.width * 30 / 100).clamp(34, 60);
                let split =
                    Layout::horizontal([Constraint::Min(40), Constraint::Length(gpu_w)]).split(top);
                out.cpu = Some(split[0]);
                out.gpu = Some(split[1]);
            }
            (true, false) => out.cpu = Some(top),
            (false, _) => out.gpu = Some(top),
        }
    }

    let Some(lower) = lower else { return out };
    let (left, proc) = match (left_visible, view.show_proc) {
        (true, true) => {
            if view.proc_left {
                let cols =
                    Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                        .split(lower);
                (Some(cols[1]), Some(cols[0]))
            } else {
                let cols =
                    Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                        .split(lower);
                (Some(cols[0]), Some(cols[1]))
            }
        }
        (true, false) => (Some(lower), None),
        (false, true) => (None, Some(lower)),
        (false, false) => (None, None),
    };
    out.proc = proc;

    if let Some(left) = left {
        match (view.show_mem, view.show_net) {
            (true, true) => {
                let mem_h = ((left.height as u32 * 50 / 100) as u16)
                    .max(5)
                    .min(left.height.saturating_sub(8));
                // btop `d` hides disks; mem absorbs their rows.
                let disk_h = if view.show_disks {
                    ((left.height as u32 * 25 / 100) as u16)
                        .max(3)
                        .min(left.height.saturating_sub(mem_h + 4))
                } else {
                    0
                };
                let mem_h = if view.show_disks {
                    mem_h
                } else {
                    ((left.height as u32 * 60 / 100) as u16).max(5)
                };
                if view.mem_below_net {
                    // btop mem_below_net: net on top, then mem + disks.
                    let rows = Layout::vertical([
                        Constraint::Min(1),
                        Constraint::Length(mem_h),
                        Constraint::Length(disk_h),
                    ])
                    .split(left);
                    out.net = Some(rows[0]);
                    out.mem = Some(rows[1]);
                    out.disk = Some(rows[2]);
                } else {
                    let rows = Layout::vertical([
                        Constraint::Length(mem_h),
                        Constraint::Length(disk_h),
                        Constraint::Min(1),
                    ])
                    .split(left);
                    out.mem = Some(rows[0]);
                    out.disk = Some(rows[1]);
                    out.net = Some(rows[2]);
                }
                if !view.show_disks {
                    out.disk = None;
                }
            }
            (true, false) => {
                // btop keeps disks inside the mem box area (unless `d`).
                if view.show_disks {
                    let rows = Layout::vertical([Constraint::Percentage(62), Constraint::Min(3)])
                        .split(left);
                    out.mem = Some(rows[0]);
                    out.disk = Some(rows[1]);
                } else {
                    out.mem = Some(left);
                }
            }
            (false, true) => out.net = Some(left),
            (false, false) => {}
        }
    }
    out
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DASHBOARD layout from btop calcSizes()
//
//  CPU  100% wide, 32% tall
//  ┌─────────────────────────────────────────────────────────────┐
//  │  45% left (Mem + Disk + Net stacked)   │ 55% right (Proc)  │
//  └─────────────────────────────────────────────────────────────┘
//  Footer 1 row
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
pub fn draw_dashboard(frame: &mut ratatui::Frame<'_>, view: &AppView) {
    let area = frame.area();
    view.hits.borrow_mut().clear();
    view.nav_hits.borrow_mut().clear();
    frame.render_widget(
        Block::default().style(Style::default().bg(c_main_bg())),
        area,
    );
    if area.width < 30 || area.height < 9 {
        if view.ratty_3d {
            crate::ratty::emit_panel_frames(frame.buffer_mut(), &[]);
        }
        frame.render_widget(
            Paragraph::new("Terminal too small").style(style_fg(c_inactive())),
            area,
        );
        return;
    }

    let vert = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(area);
    draw_navigation(frame, vert[0], view);
    let body = vert[1];
    draw_dashboard_page(frame, body, view);
    if view.ratty_3d {
        let panels = ratty_panel_areas(body, view);
        crate::ratty::emit_panel_frames(frame.buffer_mut(), &panels);
    }
    draw_footer(frame, vert[2], view);
    if view.show_help {
        draw_help(frame, area, view.vim_keys);
    }
    match view.menu {
        Some(Menu::Main { selected }) => draw_main_menu(frame, area, selected),
        Some(Menu::Options { selected }) => draw_options(frame, area, view, selected),
        None => {}
    }
    if view.pending_signal.is_some() {
        draw_signal_confirm(frame, area, view);
    }
    if view.nice_picker.is_some() {
        draw_nice_picker(frame, area, view);
    }
    if view.signal_picker.is_some() {
        draw_signal_picker(frame, area, view);
    }
    // btop truecolor=false: approximate every 24-bit color in the 256 palette.
    if !view.truecolor {
        let buf = frame.buffer_mut();
        for cell in buf.content.iter_mut() {
            cell.fg = to_256(cell.fg);
            cell.bg = to_256(cell.bg);
        }
    }
}

fn draw_navigation(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let compact = area.width < 75;
    let pages = DashboardPage::ALL;
    let labels: Vec<&str> = pages.iter().map(|page| page.label(compact)).collect();
    let selected = view.page as usize;
    let mut start = selected;
    let mut end = selected + 1;
    let visible_width = |start: usize, end: usize| -> usize {
        let labels_width: usize = labels[start..end].iter().map(|label| label.len()).sum();
        labels_width
            + end.saturating_sub(start + 1) * 3
            + usize::from(start > 0) * 2
            + usize::from(end < labels.len()) * 2
    };
    while start > 0 || end < labels.len() {
        let can_left = start > 0 && visible_width(start - 1, end) <= area.width as usize;
        let can_right = end < labels.len() && visible_width(start, end + 1) <= area.width as usize;
        match (can_left, can_right) {
            (true, true) => {
                if selected - start < end - selected {
                    end += 1;
                } else {
                    start -= 1;
                }
            }
            (true, false) => start -= 1,
            (false, true) => end += 1,
            (false, false) => break,
        }
    }

    let mut spans = Vec::new();
    let mut x = area.x;
    if start > 0 {
        spans.push(Span::styled("… ", style_fg(c_inactive())));
        x = x.saturating_add(2);
    }
    let mut hits = view.nav_hits.borrow_mut();
    for index in start..end {
        let label = labels[index];
        let style = if index == selected {
            style_bold(c_title())
        } else {
            style_fg(c_graph_text())
        };
        spans.push(Span::styled(label, style));
        hits.push((
            Rect {
                x,
                y: area.y,
                width: label.len() as u16,
                height: 1,
            },
            pages[index],
        ));
        x = x.saturating_add(label.len() as u16);
        if index + 1 < end {
            spans.push(Span::styled(" | ", style_fg(c_inactive())));
            x = x.saturating_add(3);
        }
    }
    if end < labels.len() {
        spans.push(Span::styled(" …", style_fg(c_inactive())));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_dashboard_page(frame: &mut ratatui::Frame<'_>, body: Rect, view: &AppView) {
    match view.page {
        DashboardPage::Overview => draw_overview(frame, body, view),
        DashboardPage::Cpu => {
            with_graph_symbol(view.box_symbol("cpu"), || draw_cpu(frame, body, view));
        }
        DashboardPage::Memory => {
            with_graph_symbol(view.box_symbol("mem"), || draw_mem(frame, body, view));
        }
        DashboardPage::Gpu => {
            with_graph_symbol(view.box_symbol("gpu"), || draw_gpu(frame, body, view));
        }
        DashboardPage::Npu => {
            with_graph_symbol(view.box_symbol("gpu"), || draw_npu(frame, body, view));
        }
        DashboardPage::Network => {
            with_graph_symbol(view.box_symbol("net"), || draw_net(frame, body, view));
        }
        DashboardPage::Processes => {
            with_graph_symbol(view.box_symbol("proc"), || draw_proc(frame, body, view))
        }
        DashboardPage::Storage => {
            with_graph_symbol(view.box_symbol("mem"), || draw_disk(frame, body, view));
        }
        DashboardPage::Power => {
            with_graph_symbol(view.box_symbol("cpu"), || draw_power(frame, body, view));
        }
    }
}

fn draw_overview(frame: &mut ratatui::Frame<'_>, body: Rect, view: &AppView) {
    let boxes = layout_boxes(body, view);
    if let Some(r) = boxes.cpu {
        with_graph_symbol(view.box_symbol("cpu"), || draw_cpu(frame, r, view));
    }
    if let Some(r) = boxes.gpu {
        with_graph_symbol(view.box_symbol("gpu"), || draw_gpu(frame, r, view));
    }
    if let Some(r) = boxes.mem {
        with_graph_symbol(view.box_symbol("mem"), || draw_mem(frame, r, view));
    }
    if let Some(r) = boxes.disk {
        with_graph_symbol(view.box_symbol("mem"), || draw_disk(frame, r, view));
    }
    if let Some(r) = boxes.net {
        with_graph_symbol(view.box_symbol("net"), || draw_net(frame, r, view));
    }
    if let Some(r) = boxes.proc {
        with_graph_symbol(view.box_symbol("proc"), || draw_proc(frame, r, view));
    }
    if boxes.is_empty() {
        frame.render_widget(
            Paragraph::new("No boxes shown — press 1-5 to toggle cpu/mem/net/proc/gpu")
                .style(style_fg(c_inactive()))
                .centered(),
            Rect {
                y: body.y + body.height / 2,
                height: 1,
                ..body
            },
        );
    }
}

fn ratty_panel_areas(body: Rect, view: &AppView) -> Vec<Rect> {
    if view.page != DashboardPage::Overview {
        return (!body.is_empty()).then_some(body).into_iter().collect();
    }

    let boxes = layout_boxes(body, view);
    [
        boxes.cpu, boxes.gpu, boxes.mem, boxes.disk, boxes.net, boxes.proc,
    ]
    .into_iter()
    .flatten()
    .filter(|area| !area.is_empty())
    .collect()
}

/// btop `Theme::hex_to_color` 256 fallback: grayscale ramp for greys,
/// otherwise the 6×6×6 color cube.
pub fn to_256(c: Color) -> Color {
    let Color::Rgb(r, g, b) = c else {
        return c;
    };
    if r == g && g == b {
        return match r {
            0..=7 => Color::Indexed(16),
            249..=255 => Color::Indexed(231),
            v => Color::Indexed(232 + ((v as u16 - 8) * 24 / 241) as u8),
        };
    }
    let q = |v: u8| (v as u16 * 5 / 255) as u8;
    Color::Indexed(16 + 36 * q(r) + 6 * q(g) + q(b))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GPU box  (btop Gpu::draw)
//
//   ╭┤⁵gpu├──────────────────────────╮
//   │Apple M5 (10-core)              │
//   │GPU  ■■■■■■■■■■·········  46%   │
//   │⣀⣠⣴⣾⣿ braille history (cpu grad) │
//   │VRAM ■■■·············  1.0 GiB  │
//   ╰────────────────────────────────╯
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_gpu(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let block = btop_block_n(5, "gpu", c_gpu_box());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(gpu) = view.snapshot.gpus.first() else {
        frame.render_widget(
            Paragraph::new("GPU telemetry unavailable").style(style_fg(c_inactive())),
            inner,
        );
        return;
    };
    if inner.width < 10 || inner.height < 2 {
        return;
    }
    let line = |y: u16| Rect {
        y,
        height: 1,
        ..inner
    };
    let mut y = inner.y;
    let y_end = inner.y + inner.height;

    // Name (+ temperature when the platform reports one).
    let mut header = gpu.name.clone();
    if let Some(t) = gpu.temperature_celsius {
        let (value, unit) = scale_temp(t as f32, &view.temp_scale);
        header.push_str(&format!("  {value:.0}{unit}"));
    }
    frame.render_widget(
        Paragraph::new(truncate_text(&header, inner.width as usize)).style(style_bold(c_title())),
        line(y),
    );
    y += 1;

    // Utilization meter.
    let meter_w = (inner.width as usize).saturating_sub(10).max(2);
    if y < y_end {
        let spans = match gpu.utilization_percent {
            Some(u) => {
                let mut v = vec![Span::styled("GPU ", style_fg(c_main_fg()))];
                v.extend(meter_spans(u, meter_w, cpu_gradient));
                v.push(Span::styled(
                    format!("{:>4.0}%", u),
                    style_fg(cpu_gradient(u)),
                ));
                v
            }
            None => vec![Span::styled("GPU N/A", style_fg(c_inactive()))],
        };
        frame.render_widget(Paragraph::new(Line::from(spans)), line(y));
        y += 1;
    }

    // Engine split (Apple: renderer = shading/compute, tiler = geometry).
    if y + 2 < y_end {
        let mut parts = Vec::new();
        if let Some(r) = gpu.renderer_percent {
            parts.push(Span::styled(
                format!("Render {r:.0}%"),
                style_fg(cpu_gradient(r)),
            ));
        }
        if let Some(t) = gpu.tiler_percent {
            if !parts.is_empty() {
                parts.push(Span::raw("  "));
            }
            parts.push(Span::styled(
                format!("Tiler {t:.0}%"),
                style_fg(cpu_gradient(t)),
            ));
        }
        if !parts.is_empty() {
            frame.render_widget(Paragraph::new(Line::from(parts)), line(y));
            y += 1;
        }
    }

    // Memory meter goes on the last row; the graph fills the space between.
    let mem_row = match (gpu.memory_used_bytes, gpu.memory_total_bytes) {
        (Some(used), total) if y + 1 < y_end => Some((used, total)),
        _ => None,
    };
    let graph_bottom = if mem_row.is_some() { y_end - 1 } else { y_end };
    let graph_h = graph_bottom.saturating_sub(y);
    if graph_h >= 1 && gpu.utilization_percent.is_some() && !view.history.gpu.is_empty() {
        draw_braille_gradient(
            frame,
            Rect {
                y,
                height: graph_h,
                ..inner
            },
            &view.history.gpu,
            100,
            cpu_gradient,
            false,
        );
    }

    if let Some((used, total)) = mem_row {
        let label = format_bytes(used);
        let spans = match total.filter(|t| *t > 0) {
            Some(total) => {
                let pct = used as f32 / total as f32 * 100.0;
                let w = (inner.width as usize)
                    .saturating_sub(label.len() + 6)
                    .max(2);
                let mut v = vec![Span::styled("MEM ", style_fg(c_main_fg()))];
                v.extend(meter_spans(pct, w, mem_gradient));
                v.push(Span::styled(format!(" {label}"), style_fg(c_main_fg())));
                v
            }
            None => vec![Span::styled(format!("MEM {label}"), style_fg(c_main_fg()))],
        };
        frame.render_widget(Paragraph::new(Line::from(spans)), line(y_end - 1));
    }
}

fn format_watts(watts: Option<f32>) -> String {
    watts
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| format!("{value:.2} W"))
        .unwrap_or_else(|| "N/A".to_owned())
}

fn draw_npu(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let block = btop_block("npu", c_gpu_box());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 14 || inner.height < 4 {
        return;
    }

    let npu = view.snapshot.npus.first();
    let utilization = npu
        .and_then(|npu| npu.utilization_percent)
        .filter(|value| value.is_finite() && (0.0..=100.0).contains(value));
    let power = npu
        .and_then(|npu| npu.power_watts)
        .or(view.snapshot.power.npu_watts)
        .filter(|value| value.is_finite() && *value >= 0.0);
    let header = npu
        .map(|npu| npu.name.as_str())
        .unwrap_or("Neural processor");
    let label = |y: u16| Rect {
        y,
        height: 1,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(truncate_text(header, inner.width as usize)).style(style_bold(c_title())),
        label(inner.y),
    );
    let utilization_text = utilization
        .map(|value| format!("Utilization  {value:>5.1}%"))
        .unwrap_or_else(|| "Utilization    N/A".to_owned());
    frame.render_widget(
        Paragraph::new(utilization_text).style(style_fg(c_main_fg())),
        label(inner.y + 1),
    );
    frame.render_widget(
        Paragraph::new(format!("Power        {}", format_watts(power)))
            .style(style_fg(c_main_fg())),
        label(inner.y + 2),
    );

    let graph = Rect {
        y: inner.y + 3,
        height: inner.height.saturating_sub(3),
        ..inner
    };
    if graph.height == 0 {
        return;
    }
    if utilization.is_some() && !view.history.npu_usage.is_empty() {
        draw_braille_gradient(
            frame,
            graph,
            &view.history.npu_usage,
            100,
            cpu_gradient,
            false,
        );
        return;
    }
    if power.is_some() && !view.history.npu_power_mw.is_empty() {
        let max = view
            .history
            .npu_power_mw
            .iter()
            .copied()
            .max()
            .unwrap_or(1)
            .max(1);
        draw_braille_gradient(
            frame,
            graph,
            &view.history.npu_power_mw,
            max.saturating_add(max / 4).max(1),
            cpu_gradient,
            false,
        );
    } else if let Some(note) = &view.snapshot.power.note {
        frame.render_widget(
            Paragraph::new(truncate_text(note, graph.width as usize)).style(style_fg(c_inactive())),
            graph,
        );
    } else {
        frame.render_widget(
            Paragraph::new("No current NPU graph data").style(style_fg(c_inactive())),
            graph,
        );
    }
}

fn draw_power(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let block = btop_block("power", c_gpu_box());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 18 || inner.height == 0 {
        return;
    }

    let power = &view.snapshot.power;
    let rows = [
        ("CPU", power.cpu_watts),
        ("GPU", power.gpu_watts),
        ("NPU", power.npu_watts),
        ("DRAM", power.dram_watts),
        ("Package", power.package_watts),
    ];
    let summary_w = (inner.width / 3).max(18).min(inner.width);
    let summary = Rect {
        width: summary_w,
        ..inner
    };
    for (index, (name, watts)) in rows.iter().enumerate() {
        let y = inner.y + index as u16;
        if y >= inner.y + inner.height {
            break;
        }
        frame.render_widget(
            Paragraph::new(format!("{name:<8} {}", format_watts(*watts)))
                .style(style_fg(c_main_fg())),
            Rect {
                y,
                height: 1,
                ..summary
            },
        );
    }

    let (graph_history, current_watts, graph_label) = if power.package_watts.is_some() {
        (
            &view.history.package_power_mw,
            power.package_watts,
            "Package power (W)",
        )
    } else if power.cpu_watts.is_some() {
        (&view.history.cpu_power_mw, power.cpu_watts, "CPU power (W)")
    } else if power.gpu_watts.is_some() {
        (&view.history.gpu_power_mw, power.gpu_watts, "GPU power (W)")
    } else if power.npu_watts.is_some() {
        (&view.history.npu_power_mw, power.npu_watts, "NPU power (W)")
    } else {
        (&view.history.package_power_mw, None, "Power history")
    };
    if inner.width > summary_w + 4 {
        let graph = Rect {
            x: inner.x + summary_w + 1,
            width: inner.width.saturating_sub(summary_w + 1),
            ..inner
        };
        frame.render_widget(
            Paragraph::new(graph_label).style(style_fg(c_graph_text())),
            Rect { height: 1, ..graph },
        );
        let chart = Rect {
            y: graph.y + 1,
            height: graph.height.saturating_sub(1),
            ..graph
        };
        if current_watts.is_some() && !graph_history.is_empty() && chart.height > 0 {
            let max = graph_history.iter().copied().max().unwrap_or(1).max(1);
            draw_braille_gradient(
                frame,
                chart,
                graph_history,
                max.saturating_add(max / 4).max(1),
                cpu_gradient,
                false,
            );
        }
    }
    if let Some(note) = &power.note {
        let y = inner.y + inner.height.saturating_sub(1);
        frame.render_widget(
            Paragraph::new(truncate_text(note, inner.width as usize))
                .style(style_fg(c_graph_text())),
            Rect {
                y,
                height: 1,
                ..inner
            },
        );
    }
}

/// Local wall-clock time of a snapshot as HH:MM:SS (btop cpu box clock).
pub fn clock_string(at: std::time::SystemTime) -> String {
    format_clock(at, "%H:%M:%S")
}

/// btop battery title segment: "BAT▼ 64% ■■■□□ 3:12  ".
fn battery_label(b: &crate::model::BatterySnapshot) -> String {
    use crate::model::BatteryState;
    let arrow = match b.state {
        BatteryState::Charging => "▲",
        BatteryState::Discharging => "▼",
        BatteryState::Full => "■",
        BatteryState::Unknown => "",
    };
    let filled = ((b.percent / 20.0).round() as usize).min(5);
    let meter = format!("{}{}", "■".repeat(filled), "□".repeat(5 - filled));
    let eta = b
        .seconds_left
        .map(|s| format!(" {}:{:02}", s / 3600, s / 60 % 60))
        .unwrap_or_default();
    format!("BAT{arrow} {:.0}% {meter}{eta}  ", b.percent)
}

/// btop `i` io mode: read graph on top, mirrored write graph below, with
/// current rate labels. Both share one ceiling so heights compare honestly.
fn draw_disk_io(frame: &mut ratatui::Frame<'_>, inner: Rect, view: &AppView) {
    let h = &view.history;
    let max = h
        .disk_read
        .iter()
        .chain(h.disk_write.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    let d = view.filtered_disks().into_iter().next();
    let rate = |v: Option<u64>| v.map(format_rate).unwrap_or_else(|| "N/A".into());
    let upper = (inner.height / 2).max(1);
    let lower = inner.height.saturating_sub(upper);
    if !h.disk_read.is_empty() {
        draw_braille_gradient(
            frame,
            Rect {
                height: upper,
                ..inner
            },
            &h.disk_read,
            max,
            down_gradient,
            false,
        );
    }
    if lower > 0 && !h.disk_write.is_empty() {
        draw_braille_gradient(
            frame,
            Rect {
                y: inner.y + upper,
                height: lower,
                ..inner
            },
            &h.disk_write,
            max,
            up_gradient,
            true,
        );
    }
    let row = |y: u16| Rect {
        y,
        height: 1,
        ..inner
    };
    let read = format!("▼ read {}", rate(d.and_then(|d| d.read_bytes_per_second)));
    let write = format!("▲ write {}", rate(d.and_then(|d| d.write_bytes_per_second)));
    frame.render_widget(
        Paragraph::new(truncate_text(&read, inner.width as usize)).style(style_bold(c_down_end())),
        row(inner.y),
    );
    frame.render_widget(
        Paragraph::new(truncate_text(&write, inner.width as usize)).style(style_bold(c_up_end())),
        row(inner.y + inner.height - 1),
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CPU box  (btop Cpu::draw)
//
// Border: " ¹cpu " left, "─── HH:MM ───" center, "2000ms─┐" right
// Left region: braille graph (upper + inverted lower)
//   "up Nd HH:MM" bottom left of graph
// Right rail: model/hostname, CPU ■■ meter 8%, Cn meters per core, Load AVG
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_cpu(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let cpu = &view.snapshot.cpu;

    // btop cpu title: "¹cpu  HH:MM:SS  BAT▼ 64% ■■■■□ 3:12  1000ms".
    let time_str = format_clock(view.snapshot.captured_at, &view.clock_format);
    let ms_str = format!("{}ms", view.refresh_interval.as_millis());
    let bat_str = view
        .snapshot
        .battery
        .as_ref()
        .filter(|_| view.show_battery)
        .map(battery_label)
        .unwrap_or_default();
    let clock = if time_str.is_empty() {
        String::new()
    } else {
        format!("{time_str}  ")
    };
    let title = format!("┤¹cpu├  {clock}{bat_str}{ms_str}");
    let block = Block::default()
        .title(title.as_str())
        .title_style(style_bold(c_title()))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(style_fg(c_cpu_box()))
        .style(Style::default().bg(c_main_bg()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 12 || inner.height < 3 {
        return;
    }

    // Rail width: btop uses ~30 cols, plus its own box + padding (4) and a gap (1).
    let rail_w: u16 = if inner.width >= 100 {
        32 + 5
    } else if inner.width >= 60 {
        24 + 5
    } else {
        0
    }
    .min(inner.width.saturating_sub(12));

    let cols = Layout::horizontal([Constraint::Min(8), Constraint::Length(rail_w)]).split(inner);
    let graph_area = cols[0];
    // btop draws the info rail in its own box (cpu_box color), so a border
    // line separates the graph from the meters.
    let rail_area = if rail_w > 5 {
        let outer = Rect {
            x: cols[1].x + 1,
            width: cols[1].width - 1,
            ..cols[1]
        };
        let rail_block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type())
            .border_style(style_fg(c_cpu_box()))
            .padding(ratatui::widgets::Padding::horizontal(1))
            .style(Style::default().bg(c_main_bg()));
        let inner_rail = rail_block.inner(outer);
        frame.render_widget(rail_block, outer);
        inner_rail
    } else {
        cols[1]
    };

    // "up Nd HH:MM" bottom-left of graph region  (btop: last row of cpu box)
    let uptime_str = format!("up {}", format_duration(view.snapshot.uptime));
    if view.show_uptime && graph_area.height >= 2 {
        frame.render_widget(
            Paragraph::new(uptime_str).style(style_fg(c_graph_text())),
            Rect {
                y: graph_area.y + graph_area.height - 1,
                height: 1,
                ..graph_area
            },
        );
    }

    // btop-style single vertical-gradient CPU graph (green base -> red peak).
    let gh = graph_area.height.saturating_sub(1);
    if gh >= 1 && cpu.available && !view.history.cpu.is_empty() {
        draw_braille_gradient(
            frame,
            Rect {
                height: gh,
                ..graph_area
            },
            &view.history.cpu,
            100,
            cpu_gradient,
            false,
        );
    } else if !cpu.available {
        frame.render_widget(
            Paragraph::new("CPU N/A").style(style_fg(c_inactive())),
            Rect {
                height: gh.max(1),
                ..graph_area
            },
        );
    }

    if rail_w == 0 {
        return;
    }

    // Line 0: chip name (right-aligned) + freq — btop: "i7-5775C  3.3 GHz"
    {
        let freq_str = cpu
            .frequency_mhz
            .filter(|_| view.show_cpu_freq)
            .map(|f| format!("{:.1} GHz", f as f32 / 1000.0))
            .unwrap_or_default();
        // btop shows chip model name, not hostname, in the CPU rail
        let chip = Some(view.custom_cpu_name.as_str())
            .filter(|n| !n.is_empty())
            .or_else(|| {
                cpu.cpu_name.as_deref().and_then(|n| {
                    // Shorten "Intel(R) Core(TM) i7-5775C CPU @ 3.30GHz" → "i7-5775C":
                    // the first token mixing digits and letters is the model.
                    n.split_whitespace()
                        .find(|w| {
                            w.chars().any(|c| c.is_ascii_digit())
                                && w.chars().any(|c| c.is_alphabetic())
                                && !w.ends_with("GHz")
                                && !w.ends_with("MHz")
                                && !w.starts_with('@')
                        })
                        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()))
                })
            })
            .unwrap_or(&view.hostname);
        let chip_part = truncate_text(
            chip,
            (rail_area.width as usize)
                .saturating_sub(freq_str.len() + 2)
                .max(1),
        );
        let hdr = if freq_str.is_empty() {
            chip_part.to_owned()
        } else {
            format!("{chip_part}  {freq_str}")
        };
        frame.render_widget(
            Paragraph::new(truncate_text(&hdr, rail_area.width as usize))
                .style(style_fg(c_graph_text())),
            Rect {
                height: 1,
                ..rail_area
            },
        );
    }

    // Line 1: CPU ■■■■ 8%
    if rail_area.height >= 2 {
        let pct = cpu.overall_percent;
        let meter_w = (rail_area.width as usize).saturating_sub(10).max(1);
        let mut spans = vec![Span::styled("CPU ", style_bold(c_main_fg()))];
        // btop show_coretemp=false: only the package temp, on the CPU line.
        let pkg_temp = cpu
            .temperature_celsius
            .filter(|_| view.check_temp && !view.show_coretemp)
            .map(|t| {
                let (value, unit) = scale_temp(t, &view.temp_scale);
                format!(" {value:>4.0}{unit}")
            })
            .unwrap_or_default();
        let meter_w = meter_w.saturating_sub(pkg_temp.chars().count()).max(1);
        spans.extend(meter_spans(pct, meter_w, cpu_gradient));
        spans.push(Span::styled(
            format!("{:>5}", format_percent(pct)),
            style_fg(cpu_gradient(pct)),
        ));
        spans.push(Span::styled(pkg_temp, style_fg(c_graph_text())));
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                y: rail_area.y + 1,
                height: 1,
                ..rail_area
            },
        );
    }

    // Core rows: C0 ■■· 6%  [temp]
    let meta_rows = 2u16;
    let load_row = 1u16;
    let avail = rail_area.height.saturating_sub(meta_rows + load_row) as usize;
    let has_core_temps = !cpu.per_core_temperature.is_empty();
    let temp_w: u16 = if view.check_temp
        && view.show_coretemp
        && (has_core_temps || cpu.temperature_celsius.is_some())
    {
        6
    } else {
        0
    };
    let core_iter: &[f32] = if view.show_cores {
        &cpu.per_core_percent
    } else {
        &[]
    };
    for (i, &pct) in core_iter.iter().take(avail).enumerate() {
        let row_y = rail_area.y + meta_rows + i as u16;
        if row_y >= rail_area.y + rail_area.height {
            break;
        }
        let label = format!("C{i:<2} ");
        let lw = label.len();
        let mw = (rail_area.width as usize)
            .saturating_sub(lw + 6 + temp_w as usize)
            .max(1);
        let mut spans = vec![Span::styled(label, style_fg(c_main_fg()))];
        spans.extend(meter_spans(pct, mw, cpu_gradient));
        spans.push(Span::styled(
            format!("{:>5}", format_percent(pct)),
            style_fg(cpu_gradient(pct)),
        ));
        if temp_w > 0 {
            let temp_str = cpu
                .per_core_temperature
                .get(i)
                .copied()
                .or(cpu.temperature_celsius)
                .map(|t| {
                    let (value, unit) = scale_temp(t, &view.temp_scale);
                    format!("{value:>4.0}{unit}")
                })
                .unwrap_or_default();
            spans.push(Span::styled(
                format!(" {temp_str}"),
                style_fg(c_graph_text()),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                y: row_y,
                height: 1,
                ..rail_area
            },
        );
    }

    // Load AVG at bottom of rail
    if rail_area.height >= 1 {
        let s = if view.snapshot.load_average_available {
            let [l1, l2, l3] = view.snapshot.load_average;
            format!("Load avg:{l1:.2} {l2:.2} {l3:.2}")
        } else {
            "Load avg: N/A".to_owned()
        };
        frame.render_widget(
            Paragraph::new(truncate_text(&s, rail_area.width as usize))
                .style(style_fg(c_graph_text())),
            Rect {
                y: rail_area.y + rail_area.height - 1,
                height: 1,
                ..rail_area
            },
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// MEM box  (btop Mem::draw)
//
// Layout: left column = mem labels+meters, right column = swap meters
// Reference (normal.png):
//   Total:         15.5 GiB
//   Used:           8.90 GiB
//     57% ███████████████████████
//   Available:      6.66 GiB
//     43% ···················
//   Cached:         4.03 GiB
//     26% ██████
//   Free:           1.54 GiB
//     10% ··
//   Swap section (same pattern)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_mem(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let block = btop_block_n(2, "mem", c_mem_box());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 6 || inner.height == 0 {
        return;
    }

    let m = &view.snapshot.memory;
    if m.total_bytes == 0 {
        frame.render_widget(
            Paragraph::new("Used: N/A").style(style_fg(c_inactive())),
            inner,
        );
        return;
    }

    fn pct(used: u64, total: u64) -> f32 {
        if total == 0 {
            return 0.0;
        }
        (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32
    }

    let meter_w = (inner.width as usize).saturating_sub(2).max(2);
    let mut y = inner.y;
    let y_end = inner.y + inner.height;

    // Total line
    if y < y_end {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Total:     ", style_bold(c_main_fg())),
                Span::styled(format_bytes(m.total_bytes), style_fg(c_main_fg())),
            ])),
            Rect {
                y,
                height: 1,
                ..inner
            },
        );
        y += 1;
    }

    type MemSection = (&'static str, u64, u64, Color);
    let used_bytes = m.used_bytes;
    let avail_bytes = m.total_bytes.saturating_sub(m.used_bytes);
    let cached_bytes = m.cached_bytes.unwrap_or(0);
    // btop: Used, Available, Cached, Free
    // Free = total - used - cached (Linux); on macOS cached=0 so Free ≈ Available
    let free_bytes = avail_bytes.saturating_sub(cached_bytes);
    let sections: &[MemSection] = &[
        ("Used:", used_bytes, m.total_bytes, c_used_end()),
        ("Available:", avail_bytes, m.total_bytes, c_avail_end()),
        ("Cached:", cached_bytes, m.total_bytes, c_avail_end()),
        ("Free:", free_bytes, m.total_bytes, c_avail_end()),
    ];
    for (label, used, total, color) in sections {
        let p = pct(*used, *total);
        if y < y_end {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{label:<11}"), style_fg(c_main_fg())),
                    Span::styled(format_bytes(*used), style_fg(*color)),
                ])),
                Rect {
                    y,
                    height: 1,
                    ..inner
                },
            );
            y += 1;
        }
        if y < y_end {
            let mut spans = vec![Span::styled(format!("{p:>4.0}% "), style_fg(*color))];
            spans.extend(mem_meter_spans(p, meter_w, *color));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect {
                    y,
                    height: 1,
                    ..inner
                },
            );
            y += 1;
        }
    }

    // Swap
    if view.show_swap && m.swap_total_bytes > 0 && y < y_end {
        let sp = pct(m.swap_used_bytes, m.swap_total_bytes);
        if y < y_end {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Swap:      ", style_bold(c_swap())),
                    Span::styled(
                        format!(
                            "{} / {}",
                            format_bytes(m.swap_used_bytes),
                            format_bytes(m.swap_total_bytes)
                        ),
                        style_fg(c_swap()),
                    ),
                ])),
                Rect {
                    y,
                    height: 1,
                    ..inner
                },
            );
            y += 1;
        }
        if y < y_end {
            let mut spans = vec![Span::styled(format!("{sp:>4.0}% "), style_fg(c_swap()))];
            spans.extend(mem_meter_spans(sp, meter_w, c_swap()));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect {
                    y,
                    height: 1,
                    ..inner
                },
            );
            y += 1;
        }
    }

    // btop-style history graphs fill any remaining rows: memory used on top,
    // swap (mirrored) below when the machine has swap and room allows.
    let graph_h = y_end.saturating_sub(y);
    // btop mem_graphs=false: meters only.
    let graph_h = if view.mem_graphs { graph_h } else { 0 };
    let swap_h = if view.show_swap && graph_h >= 3 && !view.history.swap.is_empty() {
        (graph_h / 3).max(1)
    } else {
        0
    };
    let used_h = graph_h - swap_h;
    if used_h >= 2 && !view.history.memory.is_empty() {
        draw_braille_gradient(
            frame,
            Rect {
                y,
                height: used_h,
                ..inner
            },
            &view.history.memory,
            100,
            mem_gradient,
            false,
        );
    }
    if swap_h > 0 {
        let swap_area = Rect {
            y: y + used_h,
            height: swap_h,
            ..inner
        };
        draw_braille_gradient(frame, swap_area, &view.history.swap, 100, up_gradient, true);
        let pct = view.history.swap.back().copied().unwrap_or(0);
        frame.render_widget(
            Paragraph::new(format!("swap {pct}%")).style(style_bold(c_swap())),
            Rect {
                y: swap_area.y + swap_area.height - 1,
                height: 1,
                width: 10.min(swap_area.width),
                ..swap_area
            },
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DISK box  (btop Mem::draw disk section)
//
// Reference (normal.png) — disk section inside mem border:
//   root ▼44K  226 GiB
//   IO%
//   Used: 39% ██████████  88.8 GiB
//   Free: 61% ██████████  137 GiB
//   swap         4.95 GiB
//   Free: 52%   2.56 GiB
//   Media2       3.58 TiB
//   IO%
//   Used: 25%   930 GiB
//   Free: 75%   2.67 GiB
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_disk(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let title = if view.disk_io_mode {
        "disks  io■"
    } else {
        "disks  io□"
    };
    let block = btop_block(title, c_mem_box());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 6 || inner.height == 0 {
        return;
    }
    if view.disk_io_mode {
        draw_disk_io(frame, inner, view);
        return;
    }

    let disks = view.filtered_disks();
    if disks.is_empty() {
        frame.render_widget(
            Paragraph::new("No disks").style(style_fg(c_inactive())),
            inner,
        );
        return;
    }

    let meter_w = (inner.width as usize).saturating_sub(12).max(2);
    let mut y = inner.y;
    let y_end = inner.y + inner.height;

    for disk in disks {
        if y >= y_end {
            break;
        }
        // Mount + rates
        let mnt = truncate_text(
            &disk.mount_point,
            (inner.width as usize).saturating_sub(20).max(4),
        );
        // btop only shows IO rates it actually has; never print "▼? ▲?".
        let mut hdr = mnt;
        if let Some(r) = disk.read_bytes_per_second {
            hdr.push_str(&format!(" ▼{}", format_rate(r)));
        }
        if let Some(w) = disk.write_bytes_per_second {
            hdr.push_str(&format!(" ▲{}", format_rate(w)));
        }
        frame.render_widget(
            Paragraph::new(truncate_text(&hdr, inner.width as usize))
                .style(style_bold(c_main_fg())),
            Rect {
                y,
                height: 1,
                ..inner
            },
        );
        y += 1;
        if y >= y_end {
            break;
        }

        // Used + free bars
        if disk.total_bytes > 0 {
            let used = disk.total_bytes.saturating_sub(disk.available_bytes);
            let up = (used as f64 / disk.total_bytes as f64 * 100.0).clamp(0.0, 100.0) as f32;
            let fp = 100.0 - up;
            for (label, p, color) in [("Used:", up, c_used_end()), ("Free:", fp, c_avail_end())] {
                if y >= y_end {
                    break;
                }
                let mut spans = vec![Span::styled(
                    format!("{label} {p:>3.0}% "),
                    style_fg(c_main_fg()),
                )];
                spans.extend(mem_meter_spans(p, meter_w, color));
                spans.push(Span::styled(
                    format!(
                        " {}",
                        format_bytes(if label == "Used:" {
                            used
                        } else {
                            disk.available_bytes
                        })
                    ),
                    style_fg(color),
                ));
                frame.render_widget(
                    Paragraph::new(Line::from(spans)),
                    Rect {
                        y,
                        height: 1,
                        ..inner
                    },
                );
                y += 1;
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// NET box  (btop Net::draw)
//
// Reference (normal.png):
//   Border title: "⁵net □192.168.1.11  │ sync□ auto□ zero □<b br0 n>"
//   Left half: braille download graph (full height)
//   Right half:
//     2M  (graph scale)
//     download section:
//       ▼ 1.30 KiB/s (10.4 Kbps)
//       ▼ Top: (153 Mbps)
//       ▼ Total: 898 GiB
//     upload section:
//       ▲ 346 Byte/s (2.70 Kbps)
//       ▲ Top: (1.59 Mbps)
//       ▲ Total: 486 GiB
//   21K (bottom-left scale)
//   Bottom: upload braille graph
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_net(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let iface = view.net_iface().unwrap_or("");
    let net = view.snapshot.networks.iter().find(|n| n.interface == iface);
    let flag = |on: bool| if on { "■" } else { "□" };
    let zeroed = view.net_zero.contains_key(iface);
    let title_str = if iface.is_empty() {
        "³net".to_owned()
    } else {
        format!(
            "³net {iface}  sync{} auto{} zero{}  ‹b n›",
            flag(view.net_sync),
            flag(view.net_auto),
            flag(zeroed)
        )
    };
    let rx_hist: VecDeque<u64> = view.history.net_rx(iface).into();
    let tx_hist: VecDeque<u64> = view.history.net_tx(iface).into();
    let (rx_max, tx_max) = view.net_scale();
    let (rx_t, tx_t) = view.net_totals();
    let rate = |v: Option<u64>| v.map(format_rate).unwrap_or_else(|| "N/A".into());
    // One graph + stats block per direction; btop swap_upload_download
    // puts upload on top.
    let down = NetHalf {
        label: "download",
        arrow: "▼",
        color: c_down_end(),
        grad: down_gradient,
        max: rx_max,
        rate: rate(net.map(|n| n.received_bytes_per_second)),
        total: rx_t.map(format_bytes).unwrap_or_default(),
        hist: rx_hist,
    };
    let up = NetHalf {
        label: "upload",
        arrow: "▲",
        color: c_up_end(),
        grad: up_gradient,
        max: tx_max,
        rate: rate(net.map(|n| n.transmitted_bytes_per_second)),
        total: tx_t.map(format_bytes).unwrap_or_default(),
        hist: tx_hist,
    };
    let (top, bottom) = if view.swap_upload_download {
        (up, down)
    } else {
        (down, up)
    };

    let block = Block::default()
        .title(format!("┤{title_str}├"))
        .title_style(style_bold(c_title()))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(style_fg(c_net_box()))
        .style(Style::default().bg(c_main_bg()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 8 || inner.height == 0 {
        return;
    }

    // Split: left=graph, right=stats panel
    let stats_w: u16 = if inner.width >= 50 {
        24
    } else if inner.width >= 30 {
        18
    } else {
        0
    }
    .min(inner.width.saturating_sub(8));
    let hcols = Layout::horizontal([Constraint::Min(4), Constraint::Length(stats_w)]).split(inner);
    let graph_area = hcols[0];
    let stats_area = hcols[1];

    // Graph region: upper half = top (invert=false), lower = bottom (invert=true)
    if inner.height >= 2 {
        let upper_h = (inner.height / 2).max(1);
        let lower_h = inner.height.saturating_sub(upper_h);

        // btop: scale labels at top-left and bottom-left of graph (e.g. "2M", "21K")
        let scale_w = 4u16;
        let gx = graph_area.x + scale_w;
        let gw = graph_area.width.saturating_sub(scale_w);

        // Draw scale top
        if graph_area.width > scale_w {
            frame.render_widget(
                Paragraph::new(truncate_text(&format_rate(top.max), scale_w as usize))
                    .style(style_fg(c_graph_text())),
                Rect {
                    x: graph_area.x,
                    y: graph_area.y,
                    width: scale_w,
                    height: 1,
                },
            );
        }

        if !top.hist.is_empty() && gw > 0 {
            draw_braille_gradient(
                frame,
                Rect {
                    x: gx,
                    width: gw,
                    height: upper_h,
                    ..graph_area
                },
                &top.hist,
                top.max,
                top.grad,
                false,
            );
        } else {
            frame.render_widget(
                Paragraph::new("N/A").style(style_fg(c_inactive())),
                Rect {
                    height: 1,
                    ..graph_area
                },
            );
        }

        if lower_h > 0 {
            // Draw scale bottom
            if graph_area.width > scale_w {
                frame.render_widget(
                    Paragraph::new(truncate_text(&format_rate(bottom.max), scale_w as usize))
                        .style(style_fg(c_graph_text())),
                    Rect {
                        x: graph_area.x,
                        y: graph_area.y + upper_h + lower_h - 1,
                        width: scale_w,
                        height: 1,
                    },
                );
            }
            if !bottom.hist.is_empty() && gw > 0 {
                draw_braille_gradient(
                    frame,
                    Rect {
                        x: gx,
                        width: gw,
                        y: graph_area.y + upper_h,
                        height: lower_h,
                    },
                    &bottom.hist,
                    bottom.max,
                    bottom.grad,
                    true,
                );
            }
        }
    }

    if stats_w == 0 {
        return;
    }

    // Stats panel — btop exact format:
    //   download
    //   ▼ 1.30 KiB/s (10.4 Kbps)
    //   ▼ Top: (153 Mbps)
    //   ▼ Total: 898 GiB
    //   upload
    //   ▲ 346 Byte/s (2.70 Kbps)
    //   ▲ Top: (1.59 Mbps)
    //   ▲ Total: 486 GiB
    let y_end = stats_area.y + stats_area.height;
    let mut sy = stats_area.y;
    let mut emit = |text: String, color: Color| {
        if sy < y_end {
            frame.render_widget(
                Paragraph::new(truncate_text(&text, stats_area.width as usize))
                    .style(style_fg(color)),
                Rect {
                    y: sy,
                    height: 1,
                    ..stats_area
                },
            );
            sy += 1;
        }
    };

    for half in [&top, &bottom] {
        let arrow = half.arrow;
        emit(half.label.into(), c_graph_text());
        emit(format!("{arrow} {}", half.rate), half.color);
        // Peak = max seen in history (as rate)
        let peak = half.hist.iter().copied().max().unwrap_or(0);
        if peak > 0 {
            emit(
                format!("{arrow} Top: ({})", format_rate(peak)),
                c_graph_text(),
            );
        }
        if !half.total.is_empty() {
            emit(format!("{arrow} Total: {}", half.total), c_graph_text());
        }
    }
}

/// One direction of the net box (graph + stats), so upload/download can
/// swap places (btop swap_upload_download).
struct NetHalf {
    label: &'static str,
    arrow: &'static str,
    color: Color,
    grad: fn(f32) -> Color,
    max: u64,
    rate: String,
    total: String,
    hist: VecDeque<u64>,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PROC box  (btop Proc::draw)
//
// Border title: " ⁴proc □filter ─── per-core□ reverse□ tree□ < cpu lazy >"
// Top of inner: selected process title bar  "1210131  btop   0.0%"
//   then status/elapsed/io lines
// Table header: "Pid:  Program:  Command:  Threads:  User:  MemB  Cpu% ↑"
// Bottom border: "↓ select ↓ info □ terminate □ kill □ signals  0/455 ↓"
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
/// Flatten processes into (index, depth) display order by parent_pid.
/// Roots = no parent, or parent not present in the set. Deterministic:
/// children sorted by pid. Cycle-safe via a visited set.
pub fn tree_order(procs: &[crate::model::ProcessSnapshot]) -> Vec<(usize, usize)> {
    use std::collections::{HashMap, HashSet};
    let index_of: HashMap<u32, usize> = procs.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, p) in procs.iter().enumerate() {
        match p.parent_pid {
            Some(ppid) if index_of.contains_key(&ppid) && ppid != p.pid => {
                children.entry(ppid).or_default().push(i)
            }
            _ => roots.push(i),
        }
    }
    for v in children.values_mut() {
        v.sort_by_key(|&i| procs[i].pid);
    }
    roots.sort_by_key(|&i| procs[i].pid);
    let mut out = Vec::with_capacity(procs.len());
    let mut visited = HashSet::new();
    fn walk(
        i: usize,
        depth: usize,
        procs: &[crate::model::ProcessSnapshot],
        children: &std::collections::HashMap<u32, Vec<usize>>,
        visited: &mut std::collections::HashSet<usize>,
        out: &mut Vec<(usize, usize)>,
    ) {
        if !visited.insert(i) {
            return;
        }
        out.push((i, depth));
        if let Some(kids) = children.get(&procs[i].pid) {
            for &k in kids {
                walk(k, depth + 1, procs, children, visited, out);
            }
        }
    }
    for r in roots {
        walk(r, 0, procs, &children, &mut visited, &mut out);
    }
    out
}

fn draw_proc(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    // In tree mode, order by parent/child and carry a depth per row; otherwise
    // use the flat sorted order with depth 0. Both respect the active filter.
    let rows_meta = view.display_rows();
    let vis_indices: Vec<usize> = rows_meta.iter().map(|r| r.0).collect();
    let vis_count = vis_indices.len();

    // Map visible selected index → snapshot index
    let snap_idx = vis_indices.get(view.selected_process).copied();
    let sel_proc = snap_idx.and_then(|i| view.snapshot.processes.get(i));

    // Border title: btop format "⁴proc ⌐ filter ─── per-core ⌐ reverse ⌐ tree ⌐ < cpu lazy >"
    // Toggle boxes reflect live state: ■ when active, □ when off.
    let bx = |on: bool| if on { '■' } else { '□' };
    let mut toggles = format!(
        "per-core{} reverse{} tree{}",
        bx(view.proc_per_core),
        bx(view.sort_reverse),
        bx(view.tree)
    );
    if view.followed_pid.is_some() {
        toggles.push_str("  follow■");
    }
    if view.proc_paused {
        toggles.push_str("  paused■");
    }
    if view.filter_error() {
        toggles.push_str("  bad regex");
    }
    let proc_title = if view.filter_active {
        format!("⁴proc filter:{}_  {toggles}  cpu lazy", view.filter)
    } else if !view.filter.is_empty() {
        format!("⁴proc filter:{}  {toggles}  cpu lazy", view.filter)
    } else if let Some(p) = sel_proc {
        format!(
            "⁴proc  {}  {}  {toggles}",
            truncate_text(&p.name, 20),
            format_percent(view.proc_cpu(p.cpu_percent))
        )
    } else {
        format!("⁴proc filter  {toggles}  cpu lazy")
    };
    // Clickable title toggles (btop: every "name■/□" in a title is a button).
    {
        let full = format!("┤{proc_title}├");
        for (label, action) in [
            ("per-core", Action::TogglePerCoreProc),
            ("reverse", Action::ToggleReverse),
            ("tree", Action::ToggleTree),
        ] {
            if let Some(byte) = full.find(label) {
                let col = full[..byte].chars().count() as u16;
                view.add_hit(
                    Rect {
                        x: area.x + 1 + col,
                        y: area.y,
                        width: label.chars().count() as u16 + 1,
                        height: 1,
                    },
                    action,
                );
            }
        }
    }

    let block = Block::default()
        .title(format!("┤{proc_title}├"))
        .title_style(style_bold(c_title()))
        .borders(Borders::ALL)
        .border_type(border_type())
        .border_style(style_fg(c_proc_box()))
        .style(Style::default().bg(c_main_bg()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Bottom border: btop exact action bar
    // "↕ select ↕↑info ∫terminate ∫kill ∫signals  filter:/  0/455"
    if area.height >= 2 {
        let sel = if vis_count > 0 {
            view.selected_process + 1
        } else {
            0
        };
        let total = view.snapshot.processes.len();
        // Action bar with real mtop key hints (matches key_to_action bindings).
        let kill = if view.vim_keys { 'K' } else { 'k' };
        let bar = format!(
            " ↕ select  ↵ info  t term  {kill} kill  s sig  e tree  f filter  esc menu  {sel}/{total}"
        );
        frame.render_widget(
            Paragraph::new(truncate_text(&bar, area.width.saturating_sub(2) as usize))
                .style(style_fg(c_hi_fg())),
            Rect {
                y: area.y + area.height - 1,
                height: 1,
                ..area
            },
        );
    }

    if inner.width < 10 || inner.height == 0 {
        return;
    }

    // --- btop show_detailed panel (Enter) ---
    // Left: braille cpu graph of this pid. Right: status/user/threads/nice
    // /parent, memory meter and the full command line.
    let detail_rows: u16 = if view.show_detail && sel_proc.is_some() && inner.height >= 14 {
        8
    } else {
        0
    };
    if let (true, Some(p)) = (detail_rows > 0, sel_proc) {
        draw_proc_detail(
            frame,
            Rect {
                height: detail_rows,
                ..inner
            },
            view,
            p,
        );
    }

    let body_area = Rect {
        y: inner.y + detail_rows,
        height: inner.height.saturating_sub(detail_rows),
        ..inner
    };
    if body_area.height == 0 {
        return;
    }

    // Filter bar inside body
    let filter_h: u16 = if !view.filter.is_empty() || view.filter_active {
        1
    } else {
        0
    };
    if filter_h > 0 && body_area.height >= 1 {
        let fbar = if view.filter_active {
            format!(" 🔍 filter: {}_ ({vis_count} matches)", view.filter)
        } else {
            format!(" filter: {}  ({vis_count} matches)", view.filter)
        };
        frame.render_widget(
            Paragraph::new(truncate_text(&fbar, body_area.width as usize))
                .style(style_bold(c_hi_fg())),
            Rect {
                height: 1,
                ..body_area
            },
        );
    }

    let table_area = Rect {
        y: body_area.y + filter_h,
        height: body_area.height.saturating_sub(filter_h),
        ..body_area
    };
    if table_area.height == 0 {
        return;
    }

    // Column widths (btop Proc::draw): fixed columns first, then Command
    // absorbs every leftover cell so the table always spans the box.
    let w = inner.width;
    let pid_w = 8u16;
    let prog_w: u16 = if w >= 100 {
        20
    } else if w >= 70 {
        16
    } else {
        12
    };
    let user_w: u16 = if w >= 50 { 9 } else { 0 };
    let thr_w: u16 = if w >= 110 { 8 } else { 0 };
    let cpu_w = 6u16;
    // mem: dot meter + compact 5-cell label ("568M") whose unit never clips.
    let mem_bytes_w: u16 = 5;
    let dot_w: u16 = if w >= 90 {
        10
    } else if w >= 60 {
        5
    } else {
        0
    };
    let fixed_cols = [pid_w, prog_w, user_w, thr_w, cpu_w, dot_w, mem_bytes_w];
    let fixed: u16 = fixed_cols.iter().sum();
    let gaps = fixed_cols.iter().filter(|c| **c > 0).count() as u16; // incl. gap before cmd
    let spare = w.saturating_sub(fixed + gaps);
    let cmd_w: u16 = if spare >= 8 { spare } else { 0 };

    // Scroll: keep selected visible
    let table_rows = table_area.height.saturating_sub(1) as usize;
    let scroll_start = view
        .selected_process
        .saturating_sub(table_rows.saturating_sub(1));

    // Total memory for dot-meter normalization
    let total_mem = view.snapshot.memory.total_bytes.max(1);

    let rows: Vec<Row> = vis_indices
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(table_rows)
        .map(|(vis_i, &snap_i)| {
            let p = &view.snapshot.processes[snap_i];
            let selected = vis_i == view.selected_process;
            let cpu_shown = view.shown_cpu(snap_i);
            // btop proc_gradient: rows fade toward the background further
            // down the visible list (selected row keeps its own colors).
            let fade = |c: Color| -> Color {
                if !view.proc_gradient || table_rows <= 1 {
                    return c;
                }
                let pos = (vis_i - scroll_start) as f32 / (table_rows - 1) as f32;
                lerp_color(c, c_main_bg_solid(), pos * 0.6)
            };
            let name_color = if selected {
                active().selected_fg
            } else if view.proc_colors {
                // btop colors process name by cpu% — green→yellow→red
                fade(cpu_gradient(cpu_shown))
            } else {
                fade(c_main_fg())
            };
            let row_modifier = if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            let fg = if selected {
                active().selected_fg
            } else {
                fade(c_main_fg())
            };

            // Build dotted memory meter: "·········  5.4 MiB"
            let mem_dot_str = if dot_w > 0 {
                let pct = (p.memory_bytes as f64 / total_mem as f64).clamp(0.0, 1.0);
                let filled = ((pct * dot_w as f64).round() as usize).min(dot_w as usize);
                let empty = dot_w as usize - filled;
                format!("{}{}", "■".repeat(filled), "·".repeat(empty))
            } else {
                String::new()
            };

            // Tree-view indentation prefix on the program name.
            let (_, depth, has_kids) = rows_meta.get(vis_i).copied().unwrap_or((0, 0, false));
            let name_cell = if view.tree {
                // btop tree glyphs: [+] collapsed, [-] expanded parent.
                let marker = match (has_kids, view.collapsed.contains(&p.pid)) {
                    (true, true) => "[+]",
                    (true, false) => "[-]",
                    _ => "",
                };
                let indent = if depth > 0 {
                    format!("{}└─ ", "  ".repeat(depth - 1))
                } else {
                    String::new()
                };
                truncate_text(&format!("{indent}{marker}{}", p.name), prog_w as usize)
            } else {
                truncate_text(&p.name, prog_w as usize)
            };
            let mut str_cells = vec![format!("{:>7}", p.pid), name_cell];
            if cmd_w > 0 {
                // btop Command: full cmdline; kernel threads / denied
                // processes have none, so fall back to the program name.
                let cmd = if p.command.is_empty() {
                    &p.name
                } else {
                    &p.command
                };
                str_cells.push(truncate_text(cmd, cmd_w as usize));
            }
            if thr_w > 0 {
                let t = p.threads.map(|n| n.to_string()).unwrap_or_default();
                str_cells.push(format!("{:>7}", t));
            }
            if user_w > 0 {
                str_cells.push(truncate_text(
                    p.user.as_deref().unwrap_or(""),
                    user_w as usize,
                ));
            }
            // CPU%: right-aligned
            str_cells.push(format!("{:>5}", format_percent(cpu_shown)));
            // Memory: dotted meter + bytes
            if dot_w > 0 {
                str_cells.push(mem_dot_str);
            }
            // Compact "568M"/"3.9M" so the unit survives narrow columns.
            if view.proc_mem_percent {
                let pct = p.memory_bytes as f64 / total_mem as f64 * 100.0;
                str_cells.push(format!("{:>5.1}", pct));
            } else {
                str_cells.push(format!("{:>5}", format_bytes_compact(p.memory_bytes)));
            }

            // Column index for color — name is col 1, cpu% is dynamic
            let name_col = 1usize;
            let cpu_col = 2 + (cmd_w > 0) as usize + (thr_w > 0) as usize + (user_w > 0) as usize;
            let cells_styled: Vec<ratatui::widgets::Cell> = str_cells
                .into_iter()
                .enumerate()
                .map(|(col, s)| {
                    let cell_color = if col == name_col || selected {
                        name_color
                    } else if col == cpu_col && view.proc_colors {
                        fade(cpu_gradient(cpu_shown))
                    } else {
                        fg
                    };
                    ratatui::widgets::Cell::from(s)
                        .style(Style::default().fg(cell_color).add_modifier(row_modifier))
                })
                .collect();
            let row = Row::new(cells_styled);
            if selected {
                // btop: the selected row is a full-width highlight bar.
                row.style(Style::default().bg(active().selected_bg))
            } else {
                row
            }
        })
        .collect();

    let mut constraints = vec![Constraint::Length(pid_w), Constraint::Length(prog_w)];
    if cmd_w > 0 {
        constraints.push(Constraint::Length(cmd_w));
    }
    if thr_w > 0 {
        constraints.push(Constraint::Length(thr_w));
    }
    if user_w > 0 {
        constraints.push(Constraint::Length(user_w));
    }
    constraints.push(Constraint::Length(cpu_w));
    if dot_w > 0 {
        constraints.push(Constraint::Length(dot_w));
    }
    constraints.push(Constraint::Length(mem_bytes_w));

    // btop header: active sort column carries the direction arrow.
    // Default (not reversed) sorts descending -> down arrow.
    let arrow = if view.sort_reverse { "↑" } else { "↓" };
    let tag = |col: ProcessSort, base: &str| -> String {
        if view.sort == col {
            format!("{base}{arrow}")
        } else {
            base.to_owned()
        }
    };
    let mut hdr_cells: Vec<String> = vec![
        tag(ProcessSort::Pid, "Pid:"),
        tag(ProcessSort::Name, "Program:"),
    ];
    if cmd_w > 0 {
        hdr_cells.push("Command:".to_owned());
    }
    if thr_w > 0 {
        hdr_cells.push("Threads:".to_owned());
    }
    if user_w > 0 {
        hdr_cells.push("User:".to_owned());
    }
    hdr_cells.push(tag(ProcessSort::Cpu, "Cpu%"));
    if dot_w > 0 {
        hdr_cells.push(String::new());
    }
    hdr_cells.push(tag(
        ProcessSort::Memory,
        if view.proc_mem_percent {
            "Mem%"
        } else {
            "MemB"
        },
    ));
    let header = Row::new(hdr_cells).style(style_bold(c_title()));

    // Clickable sort headers: same geometry the Table uses (1-cell spacing).
    {
        let sortable: Vec<Option<ProcessSort>> = {
            let mut v = vec![Some(ProcessSort::Pid), Some(ProcessSort::Name)];
            if cmd_w > 0 {
                v.push(None);
            }
            if thr_w > 0 {
                v.push(None);
            }
            if user_w > 0 {
                v.push(None);
            }
            v.push(Some(ProcessSort::Cpu));
            if dot_w > 0 {
                v.push(Some(ProcessSort::Memory));
            }
            v.push(Some(ProcessSort::Memory));
            v
        };
        let cols = Layout::horizontal(constraints.clone())
            .spacing(1)
            .split(table_area);
        for (rect, sort) in cols.iter().zip(sortable) {
            if let Some(sort) = sort {
                view.add_hit(Rect { height: 1, ..*rect }, Action::SortBy(sort));
            }
        }
    }

    // Record the data-row hit zone for mouse click mapping. The header
    // occupies table_area.y; data rows begin one line below it.
    let drawn_rows = rows.len().min(table_rows) as u16;
    view.set_proc_hit(ProcHitZone {
        x0: table_area.x,
        x1: table_area.x + table_area.width,
        data_y0: table_area.y + 1,
        rows: drawn_rows,
        scroll: scroll_start as u16,
    });

    frame.render_widget(Table::new(rows, constraints).header(header), table_area);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Footer
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_footer(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let (label, color) = if view.paused {
        ("⏸ PAUSED", Color::Rgb(0xff, 0xd0, 0x40))
    } else {
        ("● LIVE", c_cpu_start())
    };
    let uptime = if view.show_uptime {
        format!("  up {}", format_duration(view.snapshot.uptime))
    } else {
        String::new()
    };
    let mut left = format!(
        " {}{uptime}  {}ms  q quit  ? help",
        view.hostname,
        view.refresh_interval.as_millis(),
    );
    if let Some(msg) = &view.status_msg {
        left.push_str(&format!("  │ {msg}"));
    }
    let line = Line::from(vec![
        Span::styled(
            truncate_text(&left, area.width.saturating_sub(12) as usize),
            style_fg(c_graph_text()),
        ),
        Span::raw("  "),
        Span::styled(label, style_bold(color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Help overlay
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
fn draw_signal_confirm(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let Some((pid, sig)) = view.pending_signal else {
        return;
    };
    let name = view
        .sorted_process_indices()
        .iter()
        .map(|&i| &view.snapshot.processes[i])
        .find(|p| p.pid == pid)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let w = 52u16.min(area.width);
    let h = 7u16.min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(
            format!("Send {} to process?", sig.label()),
            style_bold(c_hi_fg()),
        )),
        Line::from(""),
        Line::from(format!("  PID {pid}  {name}")),
        Line::from(""),
        Line::from(Span::styled(
            "  Enter = confirm    Esc = cancel",
            style_fg(c_main_fg()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(btop_block("signal", c_hi_fg())),
        popup,
    );
}

fn draw_signal_picker(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let Some(buf) = view.signal_picker.as_ref() else {
        return;
    };
    let w = 52u16.min(area.width);
    let h = 8u16.min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, popup);
    let named = buf
        .parse::<i32>()
        .ok()
        .and_then(Signal::from_number)
        .map(|s| s.label())
        .unwrap_or_else(|| "—".to_owned());
    let lines = vec![
        Line::from(Span::styled(
            "Enter signal number to send",
            style_bold(c_hi_fg()),
        )),
        Line::from(""),
        Line::from(format!("  signal: {buf}_   ({named})")),
        Line::from(""),
        Line::from(Span::styled(
            "  1=HUP 2=INT 9=KILL 15=TERM 19=STOP",
            style_fg(c_main_fg()),
        )),
        Line::from(Span::styled(
            "  Enter = send    Esc = cancel",
            style_fg(c_main_fg()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(btop_block("signal", c_hi_fg())),
        popup,
    );
}

fn draw_proc_detail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    view: &AppView,
    p: &crate::model::ProcessSnapshot,
) {
    let graph_w = (area.width * 40 / 100).clamp(10, 60);
    let cols = Layout::horizontal([Constraint::Length(graph_w), Constraint::Min(10)]).split(area);
    let (graph, info) = (cols[0], cols[1]);
    let body_h = area.height.saturating_sub(1); // last row = separator

    let hist: VecDeque<u64> = view.history.proc_cpu(p.pid).into();
    if !hist.is_empty() && body_h > 0 {
        let peak = hist.iter().copied().max().unwrap_or(0).max(100);
        draw_braille_gradient(
            frame,
            Rect {
                height: body_h,
                ..graph
            },
            &hist,
            peak,
            cpu_gradient,
            false,
        );
    }

    let dash = || "-".to_owned();
    let total_mem = view.snapshot.memory.total_bytes.max(1);
    let mem_pct = p.memory_bytes as f64 / total_mem as f64 * 100.0;
    // POSIX nice values have no exact Windows equivalent; the field is shown
    // as unavailable there rather than presenting a misleading value.
    let nice = process_nice(p.pid);
    let kv = |k: &str, v: String, c: Color| {
        vec![
            Span::styled(format!("{k} "), style_bold(c_main_fg())),
            Span::styled(v, style_fg(c)),
            Span::raw("  "),
        ]
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" PID {}  ", p.pid), style_bold(c_proc_misc())),
        Span::styled(truncate_text(&p.name, 28), style_bold(c_title())),
        Span::styled(
            format!("  cpu {}", format_percent(view.proc_cpu(p.cpu_percent))),
            style_bold(cpu_gradient(p.cpu_percent)),
        ),
    ])];
    let mut row1 = kv("Status:", p.status.clone(), c_avail_end());
    row1.extend(kv(
        "Elapsed:",
        p.elapsed_secs
            .map(|s| format_duration(Duration::from_secs(s)))
            .unwrap_or_else(dash),
        c_avail_end(),
    ));
    row1.extend(kv(
        "User:",
        p.user.clone().unwrap_or_else(dash),
        c_down_end(),
    ));
    lines.push(Line::from(row1));
    let mut row2 = kv(
        "Threads:",
        p.threads.map(|t| t.to_string()).unwrap_or_else(dash),
        c_main_fg(),
    );
    row2.extend(kv("Nice:", nice, c_main_fg()));
    row2.extend(kv(
        "Parent:",
        p.parent_pid.map(|x| x.to_string()).unwrap_or_else(dash),
        c_graph_text(),
    ));
    lines.push(Line::from(row2));
    let meter_w = (info.width as usize).saturating_sub(24).clamp(4, 40);
    let mut mem = vec![Span::styled("Memory: ", style_bold(c_main_fg()))];
    mem.extend(meter_spans(mem_pct as f32, meter_w, mem_gradient));
    mem.push(Span::styled(
        format!(" {:.1}%  {}", mem_pct, format_bytes(p.memory_bytes)),
        style_fg(c_used_end()),
    ));
    lines.push(Line::from(mem));
    let cmd = if p.command.is_empty() {
        &p.name
    } else {
        &p.command
    };
    lines.push(Line::from(vec![
        Span::styled("Cmd: ", style_bold(c_main_fg())),
        Span::styled(
            truncate_text(cmd, (info.width as usize).saturating_sub(5)),
            style_fg(c_graph_text()),
        ),
    ]));
    frame.render_widget(
        Paragraph::new(lines),
        Rect {
            x: info.x + 1,
            width: info.width.saturating_sub(1),
            height: body_h,
            ..info
        },
    );
    frame.render_widget(
        Paragraph::new("─".repeat(area.width as usize)).style(style_fg(c_proc_box())),
        Rect {
            y: area.y + body_h,
            height: 1,
            ..area
        },
    );
}

fn draw_nice_picker(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView) {
    let Some(buf) = view.nice_picker.as_ref() else {
        return;
    };
    let pid = view.selected_pid().unwrap_or(0);
    let popup = centered(area, 52, 8);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(
            format!("New nice value for pid {pid}"),
            style_bold(c_hi_fg()),
        )),
        Line::from(""),
        Line::from(format!("  nice: {buf}_")),
        Line::from(""),
        Line::from(Span::styled(
            "  -20 = highest priority … 19 = lowest",
            style_fg(c_main_fg()),
        )),
        Line::from(Span::styled(
            "  Enter = apply    Esc = cancel",
            style_fg(c_main_fg()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(btop_block("nice", c_hi_fg()).style(Style::default().bg(c_main_bg()))),
        popup,
    );
}

/// Centered popup rect clamped to `area`.
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// btop main menu: logo banner above three large selectable entries.
fn draw_main_menu(frame: &mut ratatui::Frame<'_>, area: Rect, selected: usize) {
    const LOGO: [&str; 3] = [
        "█▀▄▀█ ▀█▀ █▀█ █▀█",
        "█ ▀ █  █  █▄█ █▀▀",
        "▀   ▀  ▀  ▀▀▀ ▀  ",
    ];
    let popup = centered(area, 36, 13);
    frame.render_widget(Clear, popup);
    let block = btop_block("menu", c_hi_fg()).style(Style::default().bg(c_main_bg()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line> = LOGO
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let color = cpu_gradient(35.0 + i as f32 * 30.0);
            Line::from(Span::styled(*row, style_bold(color))).centered()
        })
        .collect();
    lines.push(Line::from(""));
    for (i, item) in MAIN_MENU_ITEMS.iter().enumerate() {
        let label = format!("  {item:^20}  ");
        let style = if i == selected {
            Style::default()
                .fg(c_selected_fg())
                .bg(c_selected_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            style_fg(c_main_fg())
        };
        lines.push(Line::from(Span::styled(label, style)).centered());
        lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    let hint = "↑↓ select  ↵ open  esc close";
    if inner.height >= 1 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, style_fg(c_inactive()))).centered()),
            Rect {
                y: inner.y + inner.height - 1,
                height: 1,
                ..inner
            },
        );
    }
}

/// btop options panel: one row per setting with ◂ value ▸ and a help line.
fn draw_options(frame: &mut ratatui::Frame<'_>, area: Rect, view: &AppView, selected: usize) {
    let popup = centered(area, 64, OPTION_ROWS.len() as u16 + 7);
    frame.render_widget(Clear, popup);
    let block = btop_block("options", c_hi_fg()).style(Style::default().bg(c_main_bg()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = vec![Line::from("")];
    for (i, (label, _)) in OPTION_ROWS.iter().enumerate() {
        let value = view.option_value(i);
        let row = format!(" {label:<18}◂ {value:^10} ▸");
        let style = if i == selected {
            Style::default()
                .fg(c_selected_fg())
                .bg(c_selected_bg())
                .add_modifier(Modifier::BOLD)
        } else {
            style_fg(c_main_fg())
        };
        let pad = (inner.width as usize).saturating_sub(row.chars().count());
        lines.push(Line::from(Span::styled(
            format!("{row}{}", " ".repeat(pad)),
            style,
        )));
    }
    lines.push(Line::from(""));
    let desc = OPTION_ROWS.get(selected).map(|(_, d)| *d).unwrap_or("");
    lines.push(Line::from(Span::styled(
        truncate_text(&format!(" {desc}"), inner.width as usize),
        style_fg(c_graph_text()),
    )));
    lines.push(Line::from(Span::styled(
        " ↑↓ select  ←→ change  esc close",
        style_fg(c_inactive()),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// btop help rows (key, description) for the active layout. Built from the
/// same vim flag as `key_to_action_with`, so the overlay cannot drift from
/// the real bindings.
pub fn help_rows(vim: bool) -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![
        ("Esc, m", "Toggles main menu."),
        ("F2, o", "Shows options."),
        (
            if vim { "F1, ?, H" } else { "F1, ?, h" },
            "Shows this window.",
        ),
        ("q, ctrl + c", "Quits program."),
        (
            "ctrl + z",
            "Suspend to shell on Unix; unavailable on Windows.",
        ),
        ("+, -", "Add/Subtract 100ms to/from update timer."),
        ("F5", "Refresh now."),
        ("ctrl + r", "Reloads config file from disk."),
        (
            "Tab, Shift+Tab",
            "Switch dashboard pages; click a tab to open it.",
        ),
        ("Spacebar", "Pause / resume all updates."),
        ("u", "Pause process list (other boxes keep updating)."),
        ("p, P", "Cycle view presets forwards/backwards."),
        ("1, 2, 3, 4, 5", "Toggle CPU, MEM, NET, PROC, GPU box."),
        ("i", "Toggle disks io mode with big graphs."),
        ("d", "Toggle disks view in MEM box."),
        ("b, n", "Select previous/next network device."),
        ("z", "Toggle totals reset for current network device."),
        ("a", "Toggle auto scaling for the network graphs."),
        ("y", "Toggle synced scaling mode for network graphs."),
        ("Up, Down", "Select in process list."),
    ];
    if vim {
        rows.push(("j, k, g, G", "Vim: down / up / first / last."));
        rows.push(("h, l", "Vim: previous / next sorting column."));
    }
    rows.extend([
        ("Enter", "Show detailed information for selected process."),
        ("Pg Up, Pg Down", "Jump 1 page in process list."),
        ("Home, End", "Jump to first or last process."),
        ("Left, Right", "Select previous/next sorting column."),
        ("f, /", "Enter a process filter. Start with ! for regex."),
        ("F", "Follow selected process."),
        ("c", "Toggle per-core cpu usage of processes."),
        ("%", "Toggles memory display mode in processes box."),
        ("delete", "Clear any entered filter."),
        ("r", "Reverse sorting order in processes box."),
        ("e", "Toggle processes tree view."),
        ("E", "Collapse/expand all processes in tree view."),
        ("C", "Expand/collapse the selected process' children."),
        (
            "Spacebar, +, -",
            "Tree view: toggle/expand/collapse selected process.",
        ),
        (
            "Selected t",
            "Terminate selected process with SIGTERM - 15.",
        ),
        (
            if vim { "Selected K" } else { "Selected k" },
            "Kill selected process with SIGKILL - 9.",
        ),
        ("Selected s", "Select or enter signal to send to process."),
        ("Selected N", "Select new nice value for selected process."),
        ("Mouse scroll", "Scroll the process list."),
        ("Mouse 1", "Select in process list."),
    ]);
    rows
}

fn draw_help(frame: &mut ratatui::Frame<'_>, area: Rect, vim: bool) {
    let rows = help_rows(vim);
    let popup = centered(area, 78, rows.len() as u16 + 4);
    frame.render_widget(Clear, popup);
    let block = btop_block("help", c_hi_fg()).style(Style::default().bg(c_main_bg()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" {:<16}", "Key:"), style_bold(c_title())),
        Span::styled("Description:", style_bold(c_title())),
    ])];
    for (k, d) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!(" {k:<16}"), style_bold(c_hi_fg())),
            Span::styled(d, style_fg(c_main_fg())),
        ]));
    }
    lines.push(Line::from(Span::styled(
        if vim {
            " vim keys: On  (change in o → Vim keys)"
        } else {
            " vim keys: Off (change in o → Vim keys)"
        },
        style_fg(c_inactive()),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Terminal lifecycle
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        Ok(Self)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableMouseCapture);
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

/// Leave raw mode / alt screen, stop ourselves with SIGTSTP (the shell
/// takes over until `fg`), then restore the TUI. Mirrors btop's ctrl+z.
#[cfg(unix)]
fn suspend_to_background() -> io::Result<()> {
    execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    // SAFETY: raising SIGTSTP on ourselves; execution resumes on SIGCONT.
    unsafe {
        libc::raise(libc::SIGTSTP);
    }
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    Ok(())
}

#[cfg(unix)]
fn process_nice(pid: u32) -> String {
    // SAFETY: getpriority reads scheduler state for a pid; it does not access memory.
    unsafe { libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t) }.to_string()
}

#[cfg(windows)]
fn process_nice(_pid: u32) -> String {
    "-".to_owned()
}

#[cfg(not(any(unix, windows)))]
fn process_nice(_pid: u32) -> String {
    "-".to_owned()
}

pub fn run_tui(
    receiver: SnapshotReceiver,
    commands: CollectorCommandSender,
    mut view: AppView,
    controller: Box<dyn ProcessController>,
    config_path: Option<&std::path::Path>,
) -> Result<AppView> {
    let _guard = match TerminalGuard::enter() {
        Ok(g) => g,
        Err(e) => {
            let _ = commands.try_shutdown();
            return Err(e.into());
        }
    };
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            let _ = commands.try_shutdown();
            return Err(e.into());
        }
    };
    let result = (|| {
        if view.ratty_3d {
            crate::ratty::write_panel_registrations(stdout())?;
        }
        let mut previous_ratty_panels = 0;
        loop {
            if !view.paused {
                let _ = view.accept_snapshot(receiver.latest());
            }
            let mut current_ratty_panels = 0;
            terminal.draw(|f| {
                if view.ratty_3d {
                    current_ratty_panels = ratty_dashboard_panel_areas(f.area(), &view).len();
                }
                draw_dashboard(f, &view);
            })?;
            if view.ratty_3d && current_ratty_panels < previous_ratty_panels {
                crate::ratty::write_panel_deletes(
                    stdout(),
                    current_ratty_panels,
                    previous_ratty_panels,
                )?;
            }
            previous_ratty_panels = current_ratty_panels;
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Mouse(m) => {
                        let size = terminal.size()?;
                        let area = Rect::new(0, 0, size.width, size.height);
                        if let Some(key) = view.menu_mouse_key(m, area) {
                            match view.feed_key(key) {
                                KeyOutcome::Quit => quit = true,
                                KeyOutcome::WireRefresh => {
                                    let _ = commands
                                        .try_sync_interval(view.refresh_interval, view.paused);
                                }
                                // Menu mouse keys cannot dispatch process actions,
                                // reload config, or suspend the terminal.
                                _ => {}
                            }
                        } else {
                            view.feed_mouse(m);
                        }
                    }
                    Event::Key(key) => match view.feed_key(key) {
                        KeyOutcome::Quit => break,
                        KeyOutcome::WireRefresh => {
                            let _ = commands.try_sync_interval(view.refresh_interval, view.paused);
                        }
                        KeyOutcome::DispatchSignal => view.confirm_signal(controller.as_ref()),
                        KeyOutcome::DispatchRenice => view.confirm_renice(controller.as_ref()),
                        KeyOutcome::ReloadConfig => {
                            if let Some(path) = config_path {
                                view.reload_config(path);
                                let _ =
                                    commands.try_sync_interval(view.refresh_interval, view.paused);
                            } else {
                                view.status_msg = Some("no config file".to_owned());
                            }
                        }
                        KeyOutcome::Suspend => {
                            #[cfg(unix)]
                            {
                                suspend_to_background()?;
                                // Force a full repaint without Terminal::clear():
                                // clear() queries the cursor position (DSR), and
                                // that read times out right after SIGCONT. For a
                                // fullscreen viewport resize() clears and resets
                                // the back buffer with no terminal round-trip.
                                let size = terminal.size()?;
                                terminal.resize(ratatui::layout::Rect::new(
                                    0,
                                    0,
                                    size.width,
                                    size.height,
                                ))?;
                            }
                            #[cfg(windows)]
                            {
                                view.status_msg =
                                    Some("Ctrl+Z suspend is not available on Windows".to_owned());
                            }
                            #[cfg(not(any(unix, windows)))]
                            {
                                view.status_msg = Some(
                                    "terminal suspend is not supported on this platform".to_owned(),
                                );
                            }
                        }
                        KeyOutcome::Continue => {}
                    },
                    _ => {}
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    })();
    let _ = commands.try_shutdown();
    let cleanup = if view.ratty_3d {
        crate::ratty::write_panel_cleanup(stdout()).map_err(anyhow::Error::from)
    } else {
        Ok(())
    };
    // Hand the final view back so the caller can persist settings.
    result.and(cleanup).map(|()| view)
}

//! Remaining btop.conf keys that apply to mtop (btop_config.cpp).
use mtop::config::Config;
use mtop::format::format_percent;
use mtop::model::{DiskSnapshot, MemorySnapshot, NetworkSnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};
use std::time::{Duration, SystemTime};

fn proc(pid: u32, ppid: Option<u32>, name: &str, cpu: f32) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        parent_pid: ppid,
        name: name.into(),
        cpu_percent: cpu,
        memory_bytes: 1 << 20,
        ..ProcessSnapshot::default()
    }
}

fn disk(mount: &str) -> DiskSnapshot {
    DiskSnapshot {
        name: mount.into(),
        mount_point: mount.into(),
        total_bytes: 100 << 30,
        available_bytes: 40 << 30,
        read_bytes_per_second: None,
        write_bytes_per_second: None,
    }
}

fn snap(t: u64) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        memory: MemorySnapshot {
            total_bytes: 16 << 30,
            used_bytes: (6 + t % 4) << 30,
            swap_total_bytes: 4 << 30,
            swap_used_bytes: 1 << 30,
            ..MemorySnapshot::default()
        },
        networks: vec![NetworkSnapshot {
            interface: "en0".into(),
            received_bytes_per_second: 1000 * (t % 5 + 1),
            transmitted_bytes_per_second: 500,
            received_bytes_total: None,
            transmitted_bytes_total: None,
        }],
        disks: vec![disk("/"), disk("/Volumes/Data")],
        processes: vec![
            proc(1, None, "launchd", 1.0),
            proc(10, Some(1), "sshd", 10.0),
            proc(11, Some(10), "zsh", 20.0),
            proc(20, Some(1), "cron", 2.0),
            proc(21, Some(1), "syslogd", 3.0),
            proc(22, Some(1), "mDNSResp", 4.0),
        ],
        ..SystemSnapshot::default()
    }
}

fn view(cfg: Config) -> AppView {
    let mut v = AppView::new(snap(0), Theme::from_name("neon"));
    v.apply_config(&cfg);
    for t in 1..30 {
        v.accept_snapshot(snap(t));
    }
    v
}

fn draw(v: &AppView) -> (String, Buffer) {
    let mut t = Terminal::new(TestBackend::new(140, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    let b = t.backend().buffer().clone();
    (b.content().iter().map(|c| c.symbol()).collect(), b)
}

fn render(cfg: Config) -> (String, Buffer) {
    let out = draw(&view(cfg));
    mtop::ui::set_palette(mtop::ui::Palette::btop());
    out
}

fn braille(t: &str) -> usize {
    t.chars()
        .filter(|c| ('\u{2801}'..='\u{28FF}').contains(c))
        .count()
}

fn only(boxes: &str) -> Config {
    Config {
        boxes: boxes.into(),
        ..Config::default()
    }
}

#[test]
fn btop_key_names_are_accepted_as_aliases() {
    let c: Config =
        toml::from_str("update_ms = 1500\nproc_tree = true\nshown_boxes = \"cpu proc\"\n").unwrap();
    assert_eq!(c.interval_ms, 1500);
    assert!(c.tree);
    assert_eq!(c.boxes, "cpu proc");
}

#[test]
fn new_keys_round_trip_and_default_like_btop() {
    let d: Config = toml::from_str("").unwrap();
    assert!(d.show_swap && d.mem_graphs && d.show_disks && d.proc_mem_bytes);
    assert!(d.show_coretemp && d.proc_colors && d.proc_gradient && d.truecolor);
    assert!(!d.proc_reversed && !d.io_mode && !d.swap_upload_download);
    assert!(!d.proc_aggregate && !d.force_tty);
    assert_eq!((d.net_download, d.net_upload), (100, 100));
    assert_eq!(d.disks_filter, "");

    let v = view(Config {
        proc_reversed: true,
        proc_mem_bytes: false,
        io_mode: true,
        show_disks: false,
        ..Config::default()
    });
    assert!(v.sort_reverse && v.proc_mem_percent && v.disk_io_mode && !v.show_disks);
    let back = v.to_config(&Config::default());
    assert!(back.proc_reversed && !back.proc_mem_bytes && back.io_mode && !back.show_disks);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

#[test]
fn show_swap_false_hides_swap_meter() {
    assert!(render(only("mem")).0.contains("Swap:"));
    let (t, _) = render(Config {
        show_swap: false,
        ..only("mem")
    });
    assert!(!t.contains("Swap:"));
}

#[test]
fn mem_graphs_false_draws_meters_only() {
    let base = Config {
        show_disks: false,
        ..only("mem")
    };
    assert!(braille(&render(base.clone()).0) > 0);
    let (t, _) = render(Config {
        mem_graphs: false,
        ..base
    });
    assert_eq!(braille(&t), 0);
}

#[test]
fn swap_upload_download_puts_upload_first() {
    let (t, _) = render(only("net"));
    assert!(t.find("download").unwrap() < t.find("upload").unwrap());
    let (t, _) = render(Config {
        swap_upload_download: true,
        ..only("net")
    });
    assert!(t.find("upload").unwrap() < t.find("download").unwrap());
}

#[test]
fn fixed_net_scale_uses_net_download_upload_mebibits() {
    let v = view(Config {
        net_auto: false,
        net_sync: false,
        net_download: 10,
        net_upload: 5,
        ..Config::default()
    });
    assert_eq!(v.net_scale(), (10 * 131_072, 5 * 131_072));
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

#[test]
fn show_coretemp_false_moves_temp_to_cpu_line() {
    let mut s = snap(0);
    s.cpu.available = true;
    s.cpu.per_core_percent = vec![30.0, 50.0];
    s.cpu.per_core_temperature = vec![55.0, 57.0];
    s.cpu.temperature_celsius = Some(61.0);
    let mut v = AppView::new(s, Theme::from_name("neon"));
    v.apply_config(&Config {
        show_coretemp: false,
        ..only("cpu")
    });
    let (t, _) = draw(&v);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
    assert!(t.contains("61°"), "package temp on the CPU line");
    assert!(
        !t.contains("55°") && !t.contains("57°"),
        "no per-core temps"
    );
}

/// fg of the first cell of the row showing `name`.
fn row_fg(b: &Buffer, t: &str, name: &str) -> Color {
    let w = b.area.width as usize;
    let chars: Vec<char> = t.chars().collect();
    let text: String = chars.iter().collect();
    let idx = text.find(name).unwrap();
    let char_idx = text[..idx].chars().count();
    let y = (char_idx / w) as u16;
    // PID column: first digit on the row.
    let x = (0..w as u16)
        .find(|&x| {
            b[(x, y)].symbol().chars().all(|c| c.is_ascii_digit())
                && !b[(x, y)].symbol().trim().is_empty()
        })
        .unwrap();
    b[(x, y)].fg
}

#[test]
fn proc_gradient_darkens_lower_rows() {
    let (t, b) = render(only("proc"));
    assert_ne!(row_fg(&b, &t, "sshd"), row_fg(&b, &t, "mDNSResp"));
    let (t, b) = render(Config {
        proc_gradient: false,
        ..only("proc")
    });
    assert_eq!(row_fg(&b, &t, "sshd"), row_fg(&b, &t, "mDNSResp"));
}

#[test]
fn proc_colors_false_uses_plain_text_color() {
    let (t, b) = render(Config {
        proc_colors: false,
        proc_gradient: false,
        ..only("proc")
    });
    let w = b.area.width as usize;
    let idx = t.find("zsh").unwrap();
    let ci = t[..idx].chars().count();
    let name_fg = b[((ci % w) as u16, (ci / w) as u16)].fg;
    assert_eq!(name_fg, row_fg(&b, &t, "zsh"), "name same color as pid");
}

#[test]
fn proc_aggregate_sums_children_into_parent_in_tree() {
    let cfg = Config {
        tree: true,
        proc_aggregate: true,
        ..only("proc")
    };
    let (t, _) = render(cfg);
    // launchd 1 + sshd 10 + zsh 20 + cron 2 + syslogd 3 + mDNSResp 4 = 40.
    assert!(t.contains(&format_percent(40.0)), "aggregated parent cpu");
    assert!(t.contains(&format_percent(30.0)), "sshd = 10 + zsh 20");
}

#[test]
fn collapsed_parent_shows_subtree_usage_like_btop() {
    let mut v = view(Config {
        tree: true,
        ..only("proc")
    });
    v.collapsed.insert(10);
    let (t, _) = draw(&v);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
    assert!(
        t.contains(&format_percent(30.0)),
        "collapsed sshd carries zsh"
    );
    assert!(
        !t.contains(&format_percent(40.0)),
        "expanded launchd not summed"
    );
}

#[test]
fn disks_filter_includes_or_excludes_mounts() {
    let base = Config {
        mem_graphs: false,
        show_swap: false,
        ..only("mem")
    };
    let (t, _) = render(Config {
        disks_filter: "/Volumes/Data".into(),
        ..base.clone()
    });
    assert!(t.contains("Data") && !t.contains("│/ "), "{t}");
    let (t, _) = render(Config {
        disks_filter: "exclude=/Volumes/Data".into(),
        ..base
    });
    assert!(!t.contains("Data"));
}

#[test]
fn truecolor_false_maps_every_color_to_256_palette() {
    let (_, b) = render(Config::default());
    assert!(b.content().iter().any(|c| matches!(c.fg, Color::Rgb(..))));
    let (_, b) = render(Config {
        truecolor: false,
        ..Config::default()
    });
    assert!(
        !b.content()
            .iter()
            .any(|c| matches!(c.fg, Color::Rgb(..)) || matches!(c.bg, Color::Rgb(..)))
    );
}

#[test]
fn force_tty_uses_tty_graphs_and_square_corners() {
    let (t, _) = render(Config {
        force_tty: true,
        ..Config::default()
    });
    assert_eq!(braille(&t), 0);
    assert!(!t.contains('╭') && t.contains('┌'));
}

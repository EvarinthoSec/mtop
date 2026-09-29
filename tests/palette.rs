use mtop::ui::Palette;
use ratatui::style::Color;

#[test]
fn default_palette_matches_btop_hex() {
    // The default palette must preserve the exact btop colors that the
    // dashboard pixel-assert tests depend on. Changing these breaks fidelity.
    let p = Palette::btop();
    assert_eq!(p.cpu_box, Color::Rgb(0x55, 0x6d, 0x59));
    assert_eq!(p.mem_box, Color::Rgb(0x6c, 0x6c, 0x4b));
    assert_eq!(p.net_box, Color::Rgb(0x5c, 0x58, 0x8d));
    assert_eq!(p.proc_box, Color::Rgb(0x80, 0x52, 0x52));
    assert_eq!(p.cpu_start, Color::Rgb(0x77, 0xca, 0x9b));
    assert_eq!(p.used_end, Color::Rgb(0xff, 0x47, 0x69));
}

#[test]
fn from_name_selects_distinct_palettes() {
    // btop/neon is the default; amber and mono must differ visibly so that
    // `--theme` actually changes the rendered colors.
    let btop = Palette::from_name("neon");
    let amber = Palette::from_name("amber");
    let mono = Palette::from_name("mono");

    assert_eq!(btop, Palette::btop(), "neon maps to the btop palette");
    assert_ne!(amber.cpu_box, btop.cpu_box, "amber box color differs");
    assert_ne!(amber.cpu_start, btop.cpu_start, "amber graph color differs");
    assert_ne!(mono.cpu_box, btop.cpu_box, "mono box color differs");
    assert_ne!(mono.cpu_start, amber.cpu_start, "mono differs from amber");

    // Unknown names fall back to the btop default.
    assert_eq!(Palette::from_name("nonexistent"), Palette::btop());
}

#[test]
fn dashboard_background_is_true_rgb_black_not_ansi_black() {
    // ANSI Color::Black is remapped by terminal themes (Warp renders it gray).
    // btop paints main_bg as an RGB color so the dashboard looks identical
    // everywhere.
    use mtop::model::SystemSnapshot;
    use mtop::theme::Theme;
    use mtop::ui::{AppView, draw_dashboard};
    use ratatui::{Terminal, backend::TestBackend};
    let view = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, &view)).unwrap();
    let buf = t.backend().buffer();
    let ansi = buf
        .content()
        .iter()
        .filter(|c| c.bg == Color::Black)
        .count();
    assert_eq!(ansi, 0, "no cell may use ANSI black as background");
    assert_eq!(buf.cell((60, 20)).unwrap().bg, Palette::btop().main_bg);
}

#[test]
fn selected_process_row_has_full_width_highlight_background() {
    use mtop::model::{ProcessSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::{AppView, draw_dashboard};
    use ratatui::{Terminal, backend::TestBackend};
    let snapshot = SystemSnapshot {
        processes: (0..5)
            .map(|i| ProcessSnapshot {
                pid: 100 + i,
                name: format!("p{i}"),
                cpu_percent: 50.0 - i as f32,
                memory_bytes: 1 << 20,
                status: "Run".into(),
                user: Some("root".into()),
                elapsed_secs: None,
                threads: None,
                parent_pid: None,
                command: String::new(),
            })
            .collect(),
        ..SystemSnapshot::default()
    };
    let view = AppView::new(snapshot, Theme::from_name("neon"));
    let mut t = Terminal::new(TestBackend::new(150, 45)).unwrap();
    t.draw(|f| draw_dashboard(f, &view)).unwrap();
    let z = view.proc_hit();
    let buf = t.backend().buffer();
    let sel_bg = Palette::btop().selected_bg;
    let row = z.data_y0;
    let lit = (z.x0..z.x1)
        .filter(|&x| buf.cell((x, row)).unwrap().bg == sel_bg)
        .count();
    assert!(
        lit as u16 >= (z.x1 - z.x0) * 9 / 10,
        "selected row should be highlighted across the table, got {lit}/{}",
        z.x1 - z.x0
    );
    // Non-selected rows stay on main_bg.
    assert_ne!(buf.cell((z.x0 + 2, row + 1)).unwrap().bg, sel_bg);
}

#[test]
fn proc_table_memory_column_keeps_its_unit() {
    use mtop::model::{ProcessSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::{AppView, draw_dashboard};
    use ratatui::{Terminal, backend::TestBackend};
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 38215,
            name: "Discord Helper".into(),
            cpu_percent: 16.2,
            memory_bytes: 568 * 1024 * 1024 + 820 * 1024,
            status: "Run".into(),
            user: Some("ronnakon".into()),
            elapsed_secs: None,
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let view = AppView::new(snapshot, Theme::from_name("neon"));
    for w in [80u16, 120, 150, 200] {
        let mut t = Terminal::new(TestBackend::new(w, 40)).unwrap();
        t.draw(|f| draw_dashboard(f, &view)).unwrap();
        let text: String = t
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("568M"), "width {w}: memory should read 568M");
    }
}

#[test]
fn disk_header_omits_unknown_io_instead_of_question_marks() {
    use mtop::model::{DiskSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::{AppView, draw_dashboard};
    use ratatui::{Terminal, backend::TestBackend};
    let snapshot = SystemSnapshot {
        disks: vec![DiskSnapshot {
            name: "disk0".into(),
            mount_point: "/".into(),
            total_bytes: 100 << 30,
            available_bytes: 25 << 30,
            read_bytes_per_second: None,
            write_bytes_per_second: None,
        }],
        ..SystemSnapshot::default()
    };
    let view = AppView::new(snapshot, Theme::from_name("neon"));
    let mut t = Terminal::new(TestBackend::new(150, 45)).unwrap();
    t.draw(|f| draw_dashboard(f, &view)).unwrap();
    let text: String = t
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        !text.contains("▼?"),
        "unknown read rate must not render as ?"
    );
    assert!(
        !text.contains("▲?"),
        "unknown write rate must not render as ?"
    );
}

#[test]
fn command_column_shows_command_line_not_status() {
    use mtop::model::{ProcessSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::{AppView, draw_dashboard};
    use ratatui::{Terminal, backend::TestBackend};
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 42,
            name: "python".into(),
            cpu_percent: 1.0,
            memory_bytes: 1 << 20,
            status: "Sleep".into(),
            user: Some("root".into()),
            elapsed_secs: None,
            threads: None,
            parent_pid: None,
            command: "python3 -m http.server".into(),
        }],
        ..SystemSnapshot::default()
    };
    let view = AppView::new(snapshot, Theme::from_name("neon"));
    let mut t = Terminal::new(TestBackend::new(200, 45)).unwrap();
    t.draw(|f| draw_dashboard(f, &view)).unwrap();
    let text: String = t
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        text.contains("python3 -m http"),
        "Command column should show the cmdline"
    );
}

#[test]
fn palette_can_be_switched_at_runtime() {
    // The options menu cycles themes live, so set_palette must not be
    // write-once. Runs on its own test thread, so other tests keep btop.
    use mtop::ui::{active, set_palette};
    set_palette(Palette::amber());
    assert_eq!(active(), Palette::amber());
    set_palette(Palette::mono());
    assert_eq!(active(), Palette::mono());
    set_palette(Palette::btop());
}

fn gpu_view(util: &[f32]) -> mtop::ui::AppView {
    use mtop::model::{GpuSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::AppView;
    use std::time::{Duration, SystemTime};
    let base = SystemSnapshot {
        gpus: vec![GpuSnapshot {
            name: "Apple M5 (10-core)".into(),
            utilization_percent: Some(0.0),
            memory_used_bytes: Some(1 << 30),
            memory_total_bytes: Some(16 << 30),
            temperature_celsius: None,
            ..GpuSnapshot::default()
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(base.clone(), Theme::from_name("neon"));
    for (i, u) in util.iter().enumerate() {
        let mut s = base.clone();
        s.gpus[0].utilization_percent = Some(*u);
        s.captured_at = SystemTime::UNIX_EPOCH + Duration::from_secs(i as u64 + 1);
        view.accept_snapshot(s);
    }
    view
}

fn render_text(view: &mtop::ui::AppView, w: u16, h: u16) -> (String, ratatui::buffer::Buffer) {
    use mtop::ui::draw_dashboard;
    use ratatui::{Terminal, backend::TestBackend};
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| draw_dashboard(f, view)).unwrap();
    let buf = t.backend().buffer().clone();
    let text = buf.content().iter().map(|c| c.symbol()).collect();
    (text, buf)
}

#[test]
fn gpu_utilization_is_recorded_into_history() {
    let view = gpu_view(&[10.0, 55.0, 90.0]);
    assert_eq!(
        view.history.gpu.iter().copied().collect::<Vec<_>>(),
        vec![10, 55, 90]
    );
}

#[test]
fn gpu_box_renders_title_name_meter_and_braille_graph() {
    let view = gpu_view(&[20.0, 40.0, 60.0, 80.0, 70.0]);
    let (text, buf) = render_text(&view, 150, 45);
    assert!(text.contains("gpu"), "GPU box title missing");
    assert!(text.contains("Apple M5"), "GPU name missing");
    assert!(text.contains("70%"), "current utilization missing");
    assert!(text.contains("1.0 GiB"), "GPU memory missing");
    let braille = buf
        .content()
        .iter()
        .filter(|c| {
            c.symbol()
                .chars()
                .next()
                .is_some_and(|ch| ('\u{2801}'..='\u{28FF}').contains(&ch))
        })
        .count();
    assert!(
        braille > 20,
        "expected a braille GPU graph, found {braille} cells"
    );
}

#[test]
fn no_gpu_means_no_gpu_box() {
    use mtop::model::SystemSnapshot;
    use mtop::theme::Theme;
    use mtop::ui::AppView;
    let view = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    let (text, _) = render_text(&view, 150, 45);
    assert!(
        !text.contains("gpu"),
        "GPU box must be hidden when there is no GPU"
    );
}

#[test]
fn key_5_toggles_gpu_box_like_btop() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut view = gpu_view(&[30.0, 30.0]);
    assert!(view.show_gpu);
    view.feed_key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE));
    assert!(!view.show_gpu);
    let (text, _) = render_text(&view, 150, 45);
    assert!(!text.contains("gpu"), "GPU box hidden after pressing 5");
}

fn proc_header_line(width: u16) -> String {
    use mtop::model::{ProcessSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::AppView;
    let snap = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 1,
            name: "launchd".into(),
            command: "/sbin/launchd".into(),
            user: Some("ronnakon".into()),
            ..ProcessSnapshot::default()
        }],
        ..SystemSnapshot::default()
    };
    let view = AppView::new(snap, Theme::from_name("neon"));
    let (_, buf) = render_text(&view, width, 40);
    let w = buf.area.width;
    (0..buf.area.height)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .find(|l| l.contains("Pid:"))
        .expect("proc header row")
}

#[test]
fn proc_command_column_flexes_to_fill_the_box() {
    // btop gives Command all leftover width; no dead gap on the right.
    for width in [120u16, 160, 220] {
        let line = proc_header_line(width);
        assert!(
            line.contains("Command:"),
            "w={width}: Command column missing: {line}"
        );
        let row = proc_row_line(width);
        assert!(
            row.contains("ronnakon"),
            "w={width}: user must not be truncated: {row}"
        );
        let body = row.trim_end_matches('│');
        let trailing = body.len() - body.trim_end().len();
        assert!(
            trailing <= 2,
            "w={width}: {trailing} blank cells at row end: {row:?}"
        );
    }
}

fn proc_row_line(width: u16) -> String {
    use mtop::model::{ProcessSnapshot, SystemSnapshot};
    use mtop::theme::Theme;
    use mtop::ui::AppView;
    let snap = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 1,
            name: "launchd".into(),
            command: "/sbin/launchd".into(),
            user: Some("ronnakon".into()),
            memory_bytes: 12 << 20,
            ..ProcessSnapshot::default()
        }],
        ..SystemSnapshot::default()
    };
    let view = AppView::new(snap, Theme::from_name("neon"));
    let (_, buf) = render_text(&view, width, 40);
    let w = buf.area.width;
    (0..buf.area.height)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .find(|l| l.contains("launchd") && l.contains("ronnakon") || l.contains("      1  launchd"))
        .expect("proc data row")
        .chars()
        .skip_while(|c| *c != '│')
        .collect::<String>()
        .rsplit("││")
        .next()
        .unwrap()
        .to_owned()
}

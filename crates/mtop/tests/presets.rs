//! btop view presets: p cycles forward, P backward. Preset 0 = all boxes;
//! defaults mirror btop's `presets` config: cpu+proc, cpu+mem+net, cpu+net.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::config::Config;
use mtop::model::SystemSnapshot;
use mtop::theme::Theme;
use mtop::ui::{AppView, parse_presets};

fn press(v: &mut AppView, c: char) {
    v.feed_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn shown(v: &AppView) -> Vec<&'static str> {
    [
        ("cpu", v.show_cpu),
        ("mem", v.show_mem),
        ("net", v.show_net),
        ("proc", v.show_proc),
        ("gpu", v.show_gpu),
    ]
    .into_iter()
    .filter(|(_, on)| *on)
    .map(|(n, _)| n)
    .collect()
}

#[test]
fn btop_preset_string_parses_box_names() {
    let p = parse_presets(
        "cpu:1:default,proc:0:default cpu:0:default,mem:0:default,net:0:default cpu:0:block,net:0:tty",
    );
    assert_eq!(p.len(), 4, "preset 0 (all) + 3 configured");
    assert_eq!(p[0], vec!["cpu", "mem", "net", "proc", "gpu"]);
    assert_eq!(p[1], vec!["cpu", "proc"]);
    assert_eq!(p[2], vec!["cpu", "mem", "net"]);
    assert_eq!(p[3], vec!["cpu", "net"]);
}

#[test]
fn invalid_entries_are_skipped_and_max_nine() {
    let many = (0..20)
        .map(|_| "cpu:0:default")
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(parse_presets(&many).len(), 10, "preset 0 + max 9");
    let p = parse_presets("bogus:0:x cpu:0:default,nope:1:y");
    assert_eq!(
        p,
        vec![vec!["cpu", "mem", "net", "proc", "gpu"], vec!["cpu"]]
    );
}

#[test]
fn p_cycles_forward_and_shift_p_backward_with_wrap() {
    let mut v = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    v.apply_config(&Config::default());
    assert_eq!(shown(&v), ["cpu", "mem", "net", "proc", "gpu"]);
    press(&mut v, 'p');
    assert_eq!(shown(&v), ["cpu", "proc"]);
    assert_eq!(v.preset, 1);
    press(&mut v, 'p');
    assert_eq!(shown(&v), ["cpu", "mem", "net"]);
    press(&mut v, 'P');
    press(&mut v, 'P');
    assert_eq!(v.preset, 0);
    press(&mut v, 'P');
    assert_eq!(v.preset, 3, "P wraps to the last preset");
    assert_eq!(shown(&v), ["cpu", "net"]);
    mtop::ui::set_palette(mtop::ui::Palette::btop());
}

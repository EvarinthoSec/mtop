//! btop process-box features: regex filter, follow, %mem, per-core cpu,
//! filter matching command/user, delete clears filter.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::model::{CpuSnapshot, MemorySnapshot, ProcessSnapshot, SystemSnapshot};
use mtop::theme::Theme;
use mtop::ui::{AppView, draw_dashboard};
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, SystemTime};

fn key(v: &mut AppView, code: KeyCode) {
    v.feed_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn typ(v: &mut AppView, s: &str) {
    for c in s.chars() {
        key(v, KeyCode::Char(c));
    }
}

fn p(pid: u32, name: &str, cmd: &str, user: &str, cpu: f32, mem: u64) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        name: name.into(),
        command: cmd.into(),
        user: Some(user.into()),
        cpu_percent: cpu,
        memory_bytes: mem,
        ..ProcessSnapshot::default()
    }
}

fn snap(t: u64, procs: Vec<ProcessSnapshot>) -> SystemSnapshot {
    SystemSnapshot {
        captured_at: SystemTime::UNIX_EPOCH + Duration::from_secs(t),
        cpu: CpuSnapshot {
            available: true,
            per_core_percent: vec![0.0; 4],
            ..CpuSnapshot::default()
        },
        memory: MemorySnapshot {
            total_bytes: 1000,
            ..MemorySnapshot::default()
        },
        processes: procs,
        ..SystemSnapshot::default()
    }
}

fn base() -> Vec<ProcessSnapshot> {
    vec![
        p(10, "nginx", "nginx: worker", "www", 80.0, 100),
        p(20, "node", "node server.js", "ronnakon", 60.0, 250),
        p(30, "postgres", "postgres -D /data", "postgres", 40.0, 500),
        p(40, "zsh", "-zsh", "ronnakon", 1.0, 10),
    ]
}

fn view() -> AppView {
    AppView::new(snap(1, base()), Theme::from_name("neon"))
}

fn names(v: &AppView) -> Vec<String> {
    v.display_indices()
        .into_iter()
        .map(|i| v.snapshot.processes[i].name.clone())
        .collect()
}

fn render(v: &AppView) -> String {
    let mut t = Terminal::new(TestBackend::new(150, 40)).unwrap();
    t.draw(|f| draw_dashboard(f, v)).unwrap();
    t.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn filter_matches_command_and_user_too() {
    let mut v = view();
    key(&mut v, KeyCode::Char('f'));
    typ(&mut v, "server.js");
    assert_eq!(names(&v), vec!["node"]);
    v.filter.clear();
    v.filter.push_str("postgres");
    assert_eq!(
        names(&v),
        vec!["postgres"],
        "user/name/cmd dedupe to one row"
    );
    v.filter.clear();
    v.filter.push_str("RONNAKON");
    assert_eq!(
        names(&v),
        vec!["node", "zsh"],
        "case-insensitive user match"
    );
}

#[test]
fn bang_prefix_is_a_regex_filter() {
    let mut v = view();
    v.filter = "!^n.*x$".into();
    assert_eq!(names(&v), vec!["nginx"]);
    v.filter = "!^(zsh|node)$".into();
    assert_eq!(names(&v), vec!["node", "zsh"]);
}

#[test]
fn invalid_regex_matches_nothing_and_is_flagged() {
    let mut v = view();
    v.filter = "!([".into();
    assert!(names(&v).is_empty());
    assert!(render(&v).contains("bad regex"));
}

#[test]
fn delete_clears_the_filter() {
    let mut v = view();
    v.filter = "zsh".into();
    key(&mut v, KeyCode::Delete);
    assert!(v.filter.is_empty());
    assert_eq!(names(&v).len(), 4);
}

#[test]
fn follow_keeps_the_same_pid_selected_as_order_changes() {
    let mut v = view();
    key(&mut v, KeyCode::Down); // node (2nd by cpu)
    key(&mut v, KeyCode::Char('F'));
    assert_eq!(v.followed_pid, Some(20));
    // node becomes the heaviest process → moves to the top.
    let mut next = base();
    next[1].cpu_percent = 99.0;
    v.accept_snapshot(snap(2, next));
    assert_eq!(names(&v)[v.selected_process], "node");
    assert!(render(&v).contains("follow"), "title shows follow mode");
    // Manual navigation stops following (btop).
    key(&mut v, KeyCode::Down);
    assert_eq!(v.followed_pid, None);
}

#[test]
fn followed_process_exit_ends_follow() {
    let mut v = view();
    key(&mut v, KeyCode::Char('F'));
    let pid = v.followed_pid.unwrap();
    let rest: Vec<_> = base().into_iter().filter(|p| p.pid != pid).collect();
    v.accept_snapshot(snap(2, rest));
    assert_eq!(v.followed_pid, None);
}

#[test]
fn percent_key_switches_mem_column_to_percent() {
    let mut v = view();
    assert!(render(&v).contains("MemB"));
    key(&mut v, KeyCode::Char('%'));
    let t = render(&v);
    assert!(t.contains("Mem%"), "header switches to Mem%");
    assert!(t.contains("50.0"), "postgres uses 500/1000 = 50%");
}

#[test]
fn c_key_toggles_per_core_cpu_scale() {
    let mut v = view();
    // btop default: per-core (a process can exceed 100% on multicore).
    assert!(render(&v).contains("80.0"));
    key(&mut v, KeyCode::Char('c'));
    assert!(!v.proc_per_core);
    // total-cpu mode divides by core count (4): 80 → 20.
    assert!(render(&v).contains("20.0"));
    assert!(render(&v).contains("per-core□"));
}

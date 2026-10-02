use std::time::Duration;

use mtop::config::{Config, ProcessSort};
use mtop::model::{ProcessSnapshot, SystemSnapshot};
use mtop::platform::{SnapshotProvider, SysinfoCollector, rate_per_second, sort_processes};

fn process(pid: u32, name: &str, cpu_percent: f32, memory_bytes: u64) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        name: name.to_owned(),
        cpu_percent,
        memory_bytes,
        status: "running".to_owned(),
        user: None,
        elapsed_secs: None,
        threads: None,
        parent_pid: None,
        command: String::new(),
    }
}

#[test]
fn rate_per_second_rejects_zero_duration() {
    assert_eq!(rate_per_second(100, 200, Duration::ZERO), None);
}

#[test]
fn rate_per_second_rejects_counter_resets() {
    assert_eq!(rate_per_second(200, 100, Duration::from_secs(1)), None);
}

#[test]
fn sort_processes_cpu_is_descending_with_pid_tiebreaker_and_limit() {
    let processes = vec![
        process(30, "same", 1.0, 300),
        process(20, "same", 9.0, 100),
        process(10, "same", 1.0, 200),
        process(40, "same", 9.0, 400),
    ];

    let sorted = sort_processes(processes, ProcessSort::Cpu, 3);

    assert_eq!(
        sorted.iter().map(|process| process.pid).collect::<Vec<_>>(),
        vec![20, 40, 10]
    );
}

#[test]
fn sort_processes_memory_is_descending_with_name_and_pid_tiebreakers_and_limit() {
    let processes = vec![
        process(30, "zeta", 1.0, 500),
        process(20, "alpha", 1.0, 900),
        process(10, "beta", 1.0, 900),
        process(40, "alpha", 1.0, 900),
    ];

    let sorted = sort_processes(processes, ProcessSort::Memory, 3);

    assert_eq!(
        sorted
            .iter()
            .map(|process| (process.name.as_str(), process.pid))
            .collect::<Vec<_>>(),
        vec![("alpha", 20), ("alpha", 40), ("beta", 10)]
    );
}

struct FakeProvider {
    snapshot: SystemSnapshot,
}

impl SnapshotProvider for FakeProvider {
    fn collect(&mut self) -> SystemSnapshot {
        self.snapshot.clone()
    }
}

#[test]
fn fake_provider_satisfies_snapshot_provider_and_returns_snapshot_unchanged() {
    let expected = SystemSnapshot {
        warnings: vec!["fake warning".to_owned()],
        ..SystemSnapshot::default()
    };
    let mut provider = FakeProvider {
        snapshot: expected.clone(),
    };

    assert_eq!(provider.collect(), expected);
}

#[test]
fn real_collector_returns_timestamp_and_portable_cpu_result() {
    let mut collector = SysinfoCollector::new(&Config::default());

    let snapshot = collector.collect();

    assert!(snapshot.captured_at > std::time::SystemTime::UNIX_EPOCH);
    assert!(
        !snapshot.cpu.per_core_percent.is_empty()
            || snapshot
                .warnings
                .iter()
                .any(|warning| { warning.contains("CPU data unavailable") })
    );
}

#[test]
fn first_real_collect_is_explicitly_warming_up() {
    let mut collector = SysinfoCollector::new(&Config::default());

    let snapshot = collector.collect();

    assert!(
        snapshot
            .warnings
            .iter()
            .any(|warning| warning == "CPU sample warming up")
    );
    assert_eq!(snapshot.cpu.overall_percent, 0.0);
    assert!(
        snapshot
            .cpu
            .per_core_percent
            .iter()
            .all(|usage| *usage == 0.0)
    );
}

#[test]
fn collector_returns_more_than_display_limit_for_ui_side_sorting() {
    // The collector must not pre-truncate to the small display limit;
    // the UI owns display sorting/limiting now. Spawn our own children so the
    // host (e.g. a minimal build chroot) need not already run >25 processes.
    struct Children(Vec<std::process::Child>);
    impl Drop for Children {
        fn drop(&mut self) {
            for child in &mut self.0 {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let spawn = || {
        #[cfg(windows)]
        let mut cmd = {
            let mut cmd = std::process::Command::new("ping");
            cmd.args(["-n", "60", "127.0.0.1"]);
            cmd
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut cmd = std::process::Command::new("sleep");
            cmd.arg("60");
            cmd
        };
        cmd.stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn helper process")
    };
    let children = Children((0..30).map(|_| spawn()).collect());

    let mut c = SysinfoCollector::new(&Config::default());
    let _ = c.collect(); // warm up cpu%
    std::thread::sleep(Duration::from_millis(120));
    let snap = c.collect();
    assert!(
        snap.processes.len() > 25,
        "expected full process set, got {}",
        snap.processes.len()
    );
    drop(children);
}

#[test]
fn rate_per_second_saturates_large_deltas() {
    assert_eq!(
        rate_per_second(0, u64::MAX, Duration::from_nanos(1)),
        Some(u64::MAX)
    );
}

#[test]
fn process_user_is_resolved_to_a_name_not_a_numeric_uid() {
    // btop shows usernames. Raw uids ("501") are unreadable in the proc list.
    let mut c = SysinfoCollector::new(&Config::default());
    let snap = c.collect();
    let me = std::process::id();
    let own = snap
        .processes
        .iter()
        .find(|p| p.pid == me)
        .expect("collector should see its own process");
    let user = own.user.as_deref().expect("own process has an owner");
    assert!(
        !user.chars().all(|ch| ch.is_ascii_digit()),
        "user should be a name, got uid {user:?}"
    );
}

#[test]
fn process_command_line_is_collected() {
    // btop's "Command:" column shows the real command line, not the status.
    let mut c = SysinfoCollector::new(&Config::default());
    let snap = c.collect();
    let me = std::process::id();
    let own = snap
        .processes
        .iter()
        .find(|p| p.pid == me)
        .expect("collector should see its own process");
    assert!(
        own.command.contains("collector"),
        "own command line should name the test binary, got {:?}",
        own.command
    );
}

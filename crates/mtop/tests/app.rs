use std::sync::{
    Arc, Barrier,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

use mtop::app::{Action, App, spawn_collector};
use mtop::model::{ProcessSnapshot, SystemSnapshot};
use mtop::platform::SnapshotProvider;

#[derive(Clone)]
struct FakeProvider;

#[derive(Clone)]
struct CountingProvider {
    calls: Arc<AtomicUsize>,
    collected: mpsc::SyncSender<u64>,
}

struct GatedProvider {
    started: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

struct CoalescingProvider {
    calls: Arc<AtomicUsize>,
    collected: mpsc::SyncSender<u64>,
    second_started: mpsc::Sender<()>,
    second_release: mpsc::Receiver<()>,
}

impl SnapshotProvider for FakeProvider {
    fn collect(&mut self) -> SystemSnapshot {
        SystemSnapshot::default()
    }
}

impl SnapshotProvider for CountingProvider {
    fn collect(&mut self) -> SystemSnapshot {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        self.collected.send(call as u64).unwrap();
        SystemSnapshot {
            uptime: Duration::from_secs(call as u64),
            ..SystemSnapshot::default()
        }
    }
}

impl SnapshotProvider for GatedProvider {
    fn collect(&mut self) -> SystemSnapshot {
        self.started.send(()).unwrap();
        self.release.recv().unwrap();
        SystemSnapshot::default()
    }
}

impl SnapshotProvider for CoalescingProvider {
    fn collect(&mut self) -> SystemSnapshot {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        self.collected.send(call as u64).unwrap();
        if call == 2 {
            self.second_started.send(()).unwrap();
            self.second_release.recv().unwrap();
        }
        SystemSnapshot {
            uptime: Duration::from_secs(call as u64),
            ..SystemSnapshot::default()
        }
    }
}

fn process(pid: u32) -> ProcessSnapshot {
    ProcessSnapshot {
        pid,
        name: format!("process-{pid}"),
        cpu_percent: 0.0,
        memory_bytes: 0,
        status: "running".to_owned(),
        user: None,
        elapsed_secs: None,
        threads: None,
        parent_pid: None,
        command: String::new(),
    }
}

fn snapshot_with_processes(processes: Vec<ProcessSnapshot>) -> SystemSnapshot {
    SystemSnapshot {
        processes,
        ..SystemSnapshot::default()
    }
}

#[test]
fn actions_update_lifecycle_and_help_state() {
    let mut app = App::new(FakeProvider, Duration::from_secs(1));

    app.apply(Action::ToggleHelp);
    assert!(app.show_help);
    app.apply(Action::TogglePause);
    assert!(app.paused);
    app.apply(Action::Quit);
    assert!(app.should_quit);
}

#[test]
fn refresh_now_sets_request_without_collecting() {
    let mut app = App::new(FakeProvider, Duration::from_secs(1));

    assert!(!app.refresh_requested);
    app.apply(Action::RefreshNow);
    assert!(app.refresh_requested);
    app.accept_snapshot(SystemSnapshot::default());
    assert!(app.refresh_requested);
    assert!(app.take_refresh_request());
    assert!(!app.refresh_requested);
    assert!(!app.take_refresh_request());
}

#[test]
fn newest_snapshot_replaces_queued_stale_snapshot() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(4);
    let (snapshots, commands, worker) = spawn_collector(
        CountingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
        },
        Duration::from_secs(60),
    );

    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);
    assert_eq!(snapshots.latest().uptime, Duration::from_secs(1));
    commands.try_refresh().unwrap();
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(2);
    commands.try_refresh().unwrap();
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(3);
    assert_eq!(snapshots.latest().uptime, Duration::from_secs(3));

    let shutdown = commands.try_shutdown();
    let joined = worker.join();
    assert!(shutdown.is_ok(), "sending shutdown: {shutdown:?}");
    assert!(joined.is_ok(), "joining collector worker: {joined:?}");
}

#[test]
fn refresh_command_triggers_collection_without_waiting_for_interval() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(4);
    let (snapshots, commands, worker) = spawn_collector(
        CountingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
        },
        Duration::from_secs(60),
    );
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);
    assert_eq!(snapshots.latest().uptime, Duration::from_secs(1));

    commands.try_refresh().unwrap();
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(2);
    let refreshed = snapshots.latest();
    assert_eq!(refreshed.uptime, Duration::from_secs(2));

    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn shutdown_causes_collector_worker_to_exit_and_join() {
    let (snapshots, commands, worker) = spawn_collector(FakeProvider, Duration::from_secs(60));
    assert_eq!(snapshots.latest(), SystemSnapshot::default());
    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn spawn_collector_returns_before_initial_collection_finishes() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (snapshots, commands, worker) = spawn_collector(
        GatedProvider {
            started: started_tx,
            release: release_rx,
        },
        Duration::from_secs(60),
    );

    started_rx.recv().unwrap();
    assert_eq!(snapshots.latest(), SystemSnapshot::default());
    release_tx.send(()).unwrap();
    snapshots.wait_for_publication(1);
    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn repeated_refreshes_are_coalesced_without_blocking() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(4);
    let (second_started_tx, second_started_rx) = mpsc::channel();
    let (second_release_tx, second_release_rx) = mpsc::channel();
    let (snapshots, commands, worker) = spawn_collector(
        CoalescingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
            second_started: second_started_tx,
            second_release: second_release_rx,
        },
        Duration::from_secs(60),
    );
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);

    commands.try_refresh().unwrap();
    second_started_rx.recv().unwrap();
    for _ in 0..64 {
        commands.try_refresh().unwrap();
    }
    second_release_tx.send(()).unwrap();
    snapshots.wait_for_publication(2);
    assert!(calls.load(Ordering::SeqCst) <= 3);

    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn paused_collector_ignores_refresh_until_resumed() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(4);
    let (snapshots, commands, worker) = spawn_collector(
        CountingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
        },
        Duration::from_secs(60),
    );
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);

    commands.try_pause(true).unwrap();
    commands.try_refresh().unwrap();
    assert!(
        collected_rx
            .recv_timeout(Duration::from_millis(50))
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    commands.try_pause(false).unwrap();
    commands.try_refresh().unwrap();
    assert_eq!(collected_rx.recv().unwrap(), 2);
    snapshots.wait_for_publication(2);

    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn interval_update_changes_subsequent_timer_wait() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(4);
    let (snapshots, commands, worker) = spawn_collector(
        CountingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
        },
        Duration::from_secs(60),
    );
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);

    commands
        .try_set_interval(Duration::from_millis(10))
        .unwrap();
    assert_eq!(
        collected_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        2
    );
    snapshots.wait_for_publication(2);

    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn update_ms_key_wires_ui_interval_into_collector() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use mtop::config::Config;
    use mtop::theme::Theme;
    use mtop::ui::{AppView, KeyOutcome};

    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(4);
    let (_snapshots, commands, worker) = spawn_collector(
        CountingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
        },
        Duration::from_secs(60),
    );
    assert_eq!(collected_rx.recv().unwrap(), 1);

    let mut view = AppView::new(SystemSnapshot::default(), Theme::from_name("neon"));
    view.apply_config(&Config {
        interval_ms: 300,
        ..Config::default()
    });
    assert_eq!(
        view.feed_key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE)),
        KeyOutcome::WireRefresh
    );
    assert_eq!(view.refresh_interval, Duration::from_millis(200));

    // Same control path the TUI loop must execute for WireRefresh.
    commands
        .try_sync_interval(view.refresh_interval, view.paused)
        .unwrap();
    assert_eq!(
        collected_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        2
    );
    assert_eq!(
        collected_rx
            .recv_timeout(Duration::from_millis(800))
            .unwrap(),
        3,
        "collector should use the newly selected 200ms interval, not its original 60s wait"
    );

    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn zero_interval_is_clamped_before_periodic_collection() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(8);
    let (snapshots, commands, worker) = spawn_collector(
        CountingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
        },
        Duration::ZERO,
    );
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);

    commands.try_set_interval(Duration::ZERO).unwrap();
    assert!(
        collected_rx
            .recv_timeout(Duration::from_millis(50))
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        collected_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        2
    );

    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn concurrent_refreshes_are_coalesced_without_deadlock() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (collected_tx, collected_rx) = mpsc::sync_channel(8);
    let (second_started_tx, second_started_rx) = mpsc::channel();
    let (second_release_tx, second_release_rx) = mpsc::channel();
    let (snapshots, commands, worker) = spawn_collector(
        CoalescingProvider {
            calls: Arc::clone(&calls),
            collected: collected_tx,
            second_started: second_started_tx,
            second_release: second_release_rx,
        },
        Duration::from_secs(60),
    );
    collected_rx.recv().unwrap();
    snapshots.wait_for_publication(1);

    commands.try_refresh().unwrap();
    second_started_rx.recv().unwrap();

    let barrier = Arc::new(Barrier::new(16));
    let threads = (0..15)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let commands = commands.clone();
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..16 {
                    commands.try_refresh().unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    second_release_tx.send(()).unwrap();
    snapshots.wait_for_publication(2);

    assert!(calls.load(Ordering::SeqCst) <= 3);
    commands.try_shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn interval_changes_clamp_to_supported_bounds() {
    let mut app = App::new(FakeProvider, Duration::from_millis(500));

    for _ in 0..10 {
        app.apply(Action::DecreaseInterval);
    }
    assert_eq!(app.refresh_interval, Duration::from_millis(100));

    for _ in 0..200 {
        app.apply(Action::IncreaseInterval);
    }
    assert_eq!(app.refresh_interval, Duration::from_secs(10));
}

#[test]
fn selection_moves_and_clamps_to_current_process_list() {
    let mut app = App::new(FakeProvider, Duration::from_secs(1));
    app.accept_snapshot(snapshot_with_processes(vec![
        process(1),
        process(2),
        process(3),
    ]));

    app.apply(Action::SelectNext);
    app.apply(Action::SelectNext);
    app.apply(Action::SelectNext);
    assert_eq!(app.selected_process, 2);

    app.apply(Action::SelectPrevious);
    app.apply(Action::SelectPrevious);
    app.apply(Action::SelectPrevious);
    assert_eq!(app.selected_process, 0);
}

#[test]
fn accepting_snapshot_clamps_selection_and_empty_processes_stay_at_zero() {
    let mut app = App::new(FakeProvider, Duration::from_secs(1));
    app.accept_snapshot(snapshot_with_processes(vec![process(1), process(2)]));
    app.apply(Action::SelectNext);
    app.apply(Action::SelectNext);
    assert_eq!(app.selected_process, 1);

    app.accept_snapshot(snapshot_with_processes(vec![process(3)]));
    assert_eq!(app.selected_process, 0);

    app.apply(Action::SelectPrevious);
    app.apply(Action::SelectNext);
    app.accept_snapshot(SystemSnapshot::default());
    assert_eq!(app.selected_process, 0);
    app.apply(Action::SelectNext);
    assert_eq!(app.selected_process, 0);
}

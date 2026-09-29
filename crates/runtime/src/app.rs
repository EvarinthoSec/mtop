//! Application lifecycle and orchestration.

use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;

use tokio::sync::watch;

use crate::SnapshotProvider;
use crate::model::SystemSnapshot;

const MIN_COLLECTOR_INTERVAL: Duration = Duration::from_millis(100);
const MAX_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const REFRESH_INTERVAL_STEP: Duration = Duration::from_millis(100);

/// Actions understood by the application state reducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    ToggleHelp,
    RefreshNow,
    TogglePause,
    IncreaseInterval,
    DecreaseInterval,
    SelectNext,
    SelectPrevious,
    PageDown,
    PageUp,
    SelectFirst,
    SelectLast,
    ToggleDetail,
    ToggleFilter,
    FilterChar(char),
    FilterClear,
    SortNext,
    SortPrev,
    RequestTerminate,
    RequestKill,
    ConfirmSignal,
    CancelSignal,
    ToggleTree,
    SignalPicker,
    ToggleReverse,
    ToggleCores,
    OpenMenu,
    OpenOptions,
    /// btop 1-5: toggle cpu/mem/net/proc/gpu box.
    ToggleBox(u8),
    NetNext,
    NetPrev,
    NetZero,
    NetAuto,
    NetSync,
    ToggleFollow,
    TogglePerCoreProc,
    ToggleMemPercent,
    ToggleDiskIo,
    NicePicker,
    PresetNext,
    PresetPrev,
    ToggleProcPause,
    /// Mouse click on a proc header: sort by it (again = reverse).
    SortBy(crate::config::ProcessSort),
    ToggleDisks,
}

/// UI-independent application state.
pub struct App<P: SnapshotProvider> {
    pub snapshot: SystemSnapshot,
    pub provider: Option<P>,
    pub refresh_interval: Duration,
    pub paused: bool,
    pub show_help: bool,
    pub should_quit: bool,
    pub selected_process: usize,
    pub refresh_requested: bool,
}

impl<P: SnapshotProvider> App<P> {
    pub fn new(provider: P, refresh_interval: Duration) -> Self {
        Self {
            snapshot: SystemSnapshot::default(),
            provider: Some(provider),
            refresh_interval: refresh_interval.clamp(MIN_COLLECTOR_INTERVAL, MAX_REFRESH_INTERVAL),
            paused: false,
            show_help: false,
            should_quit: false,
            selected_process: 0,
            refresh_requested: false,
        }
    }

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::RefreshNow => self.refresh_requested = true,
            Action::TogglePause => self.paused = !self.paused,
            Action::IncreaseInterval => {
                self.refresh_interval = self
                    .refresh_interval
                    .saturating_add(REFRESH_INTERVAL_STEP)
                    .min(MAX_REFRESH_INTERVAL);
            }
            Action::DecreaseInterval => {
                self.refresh_interval = self
                    .refresh_interval
                    .checked_sub(REFRESH_INTERVAL_STEP)
                    .unwrap_or(Duration::ZERO)
                    .max(MIN_COLLECTOR_INTERVAL);
            }
            Action::SelectNext => {
                if let Some(last) = self.snapshot.processes.len().checked_sub(1) {
                    self.selected_process = (self.selected_process + 1).min(last);
                } else {
                    self.selected_process = 0;
                }
            }
            Action::SelectPrevious => {
                self.selected_process = self.selected_process.saturating_sub(1);
                self.clamp_selection();
            }
            // New navigation/ui actions — App treats them as no-ops;
            // AppView in ui.rs handles them fully.
            Action::PageDown
            | Action::PageUp
            | Action::SelectFirst
            | Action::SelectLast
            | Action::ToggleDetail
            | Action::ToggleFilter
            | Action::FilterChar(_)
            | Action::FilterClear
            | Action::SortNext
            | Action::SortPrev
            | Action::RequestTerminate
            | Action::RequestKill
            | Action::ConfirmSignal
            | Action::CancelSignal
            | Action::ToggleTree
            | Action::SignalPicker
            | Action::ToggleReverse
            | Action::ToggleCores
            | Action::OpenMenu
            | Action::OpenOptions
            | Action::ToggleBox(_)
            | Action::NetNext
            | Action::NetPrev
            | Action::NetZero
            | Action::NetAuto
            | Action::NetSync
            | Action::ToggleFollow
            | Action::TogglePerCoreProc
            | Action::ToggleMemPercent
            | Action::ToggleDiskIo
            | Action::NicePicker
            | Action::PresetNext
            | Action::PresetPrev
            | Action::ToggleProcPause
            | Action::SortBy(_)
            | Action::ToggleDisks => {}
        }
    }

    pub fn accept_snapshot(&mut self, snapshot: SystemSnapshot) {
        self.snapshot = snapshot;
        self.clamp_selection();
    }

    /// Take one pending refresh request so the event loop can send it to the collector.
    pub fn take_refresh_request(&mut self) -> bool {
        let requested = self.refresh_requested;
        self.refresh_requested = false;
        requested
    }

    fn clamp_selection(&mut self) {
        self.selected_process = self
            .snapshot
            .processes
            .len()
            .checked_sub(1)
            .map_or(0, |last| self.selected_process.min(last));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CollectorControl {
    refresh_generation: usize,
    paused: bool,
    interval: Duration,
    shutdown: bool,
}

/// Error returned when the collector worker is no longer available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandError;

/// Nonblocking control handle for a collector worker.
#[derive(Clone)]
pub struct CollectorCommandSender {
    tx: watch::Sender<CollectorControl>,
    control: Arc<Mutex<CollectorControl>>,
    shutdown: Arc<AtomicBool>,
}

impl CollectorCommandSender {
    /// Request a refresh. Repeated requests coalesce and never wait for the worker.
    pub fn try_refresh(&self) -> Result<(), CommandError> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(CommandError);
        }
        let mut control = self.control.lock().unwrap();
        control.refresh_generation = control.refresh_generation.wrapping_add(1);
        self.tx.send(*control).map_err(|_| CommandError)
    }

    /// Pause or resume periodic sampling without waiting for the worker.
    pub fn try_pause(&self, paused: bool) -> Result<(), CommandError> {
        self.update_control(|control| control.paused = paused)
    }

    /// Update the interval used by subsequent periodic waits without waiting for the worker.
    ///
    /// Values below 100ms are clamped to the shared collector minimum.
    pub fn try_set_interval(&self, interval: Duration) -> Result<(), CommandError> {
        self.update_control(|control| {
            control.interval = interval.clamp(MIN_COLLECTOR_INTERVAL, MAX_REFRESH_INTERVAL)
        })
    }

    /// Synchronize UI interval/pause state with the worker. When running,
    /// request one immediate sample so the new interval is visible promptly;
    /// while paused, only update the worker controls.
    pub fn try_sync_interval(&self, interval: Duration, paused: bool) -> Result<(), CommandError> {
        self.update_control(|control| {
            control.interval = interval.clamp(MIN_COLLECTOR_INTERVAL, MAX_REFRESH_INTERVAL);
            control.paused = paused;
            if !paused {
                control.refresh_generation = control.refresh_generation.wrapping_add(1);
            }
        })
    }

    fn update_control(
        &self,
        update: impl FnOnce(&mut CollectorControl),
    ) -> Result<(), CommandError> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(CommandError);
        }
        let mut control = self.control.lock().unwrap();
        update(&mut control);
        self.tx.send(*control).map_err(|_| CommandError)
    }

    /// Request shutdown. It replaces any pending refresh and never waits for the worker.
    pub fn try_shutdown(&self) -> Result<(), CommandError> {
        self.shutdown.store(true, Ordering::Release);
        let mut control = self.control.lock().unwrap();
        control.shutdown = true;
        self.tx.send(*control).map_err(|_| CommandError)
    }
}

/// Receiver for the latest available system snapshot.
pub struct SnapshotReceiver {
    inner: Arc<Mutex<watch::Receiver<SystemSnapshot>>>,
    publication_count: Arc<AtomicUsize>,
    publication_wait: Arc<(Mutex<usize>, Condvar)>,
}

impl SnapshotReceiver {
    /// Return the newest snapshot currently published by the collector.
    pub fn latest(&self) -> SystemSnapshot {
        self.inner.lock().unwrap().borrow().clone()
    }

    /// Return the number of snapshots successfully published so far.
    pub fn published_count(&self) -> usize {
        self.publication_count.load(Ordering::Acquire)
    }

    /// Block until at least `target` snapshots have been published.
    pub fn wait_for_publication(&self, target: usize) {
        let (lock, ready) = &*self.publication_wait;
        let mut count = lock.lock().unwrap();
        while *count < target {
            count = ready.wait(count).unwrap();
        }
    }
}

/// Spawn a collector on a dedicated Tokio runtime.
///
/// Intervals below 100ms are clamped to the shared collector minimum.
/// Startup returns immediately; the initial collection happens on the worker. The returned
/// join handle is required for deterministic shutdown. If a provider blocks inside `collect`,
/// joining may wait because `SnapshotProvider` has no cancellation mechanism.
pub fn spawn_collector<P: SnapshotProvider + 'static>(
    mut provider: P,
    interval: Duration,
) -> (
    SnapshotReceiver,
    CollectorCommandSender,
    thread::JoinHandle<()>,
) {
    let (snapshot_tx, snapshot_rx) = watch::channel(SystemSnapshot::default());
    let snapshot_rx = Arc::new(Mutex::new(snapshot_rx));
    let publication_count = Arc::new(AtomicUsize::new(0));
    let publication_wait = Arc::new((Mutex::new(0), Condvar::new()));
    let interval = interval.clamp(MIN_COLLECTOR_INTERVAL, MAX_REFRESH_INTERVAL);
    let initial_control = CollectorControl {
        refresh_generation: 0,
        paused: false,
        interval,
        shutdown: false,
    };
    let (command_tx, mut command_rx) = watch::channel(initial_control);
    let control = Arc::new(Mutex::new(initial_control));
    let shutdown = Arc::new(AtomicBool::new(false));

    let worker_publication_count = Arc::clone(&publication_count);
    let worker_publication_wait = Arc::clone(&publication_wait);
    let worker = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("collector Tokio runtime should build");
        runtime.block_on(async move {
            let publish = |snapshot| {
                if snapshot_tx.send(snapshot).is_ok() {
                    worker_publication_count.fetch_add(1, Ordering::Release);
                    let (lock, ready) = &*worker_publication_wait;
                    *lock.lock().unwrap() += 1;
                    ready.notify_all();
                }
            };

            publish(provider.collect());
            let mut paused = false;
            let mut interval = interval;
            let mut refresh_generation = 0;
            loop {
                if paused {
                    if command_rx.changed().await.is_err() {
                        break;
                    }
                    let control = *command_rx.borrow();
                    if control.shutdown {
                        break;
                    }
                    let refresh_requested = control.refresh_generation != refresh_generation;
                    paused = control.paused;
                    interval = control.interval;
                    refresh_generation = control.refresh_generation;
                    if refresh_requested && !paused {
                        publish(provider.collect());
                    }
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => publish(provider.collect()),
                        changed = command_rx.changed() => {
                            if changed.is_err() {
                                break;
                            }
                            let control = *command_rx.borrow();
                            if control.shutdown {
                                break;
                            }
                            let refresh_requested = control.refresh_generation != refresh_generation;
                            refresh_generation = control.refresh_generation;
                            paused = control.paused;
                            interval = control.interval;
                            if refresh_requested && !paused {
                                publish(provider.collect());
                            }
                        }
                    }
                }
            }
        });
    });

    (
        SnapshotReceiver {
            inner: snapshot_rx,
            publication_count,
            publication_wait,
        },
        CollectorCommandSender {
            tx: command_tx,
            control,
            shutdown,
        },
        worker,
    )
}

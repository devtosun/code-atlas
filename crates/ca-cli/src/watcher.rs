use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ca_core::CancellationContext;
use notify::{
    Config as NotifyConfig, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher, WatcherKind,
};
use thiserror::Error;

const MIN_DEBOUNCE: Duration = Duration::from_millis(10);
const MAX_DEBOUNCE: Duration = Duration::from_secs(30);
const MIN_RECONCILE: Duration = Duration::from_millis(50);
const MAX_RECONCILE: Duration = Duration::from_secs(24 * 60 * 60);
const MIN_QUEUE_CAPACITY: usize = 1;
const MAX_QUEUE_CAPACITY: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WatchConfig {
    pub enabled: bool,
    pub debounce: Duration,
    pub reconcile_interval: Duration,
    pub queue_capacity: usize,
    pub polling_fallback: bool,
    pub force_polling: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            debounce: Duration::from_millis(250),
            reconcile_interval: Duration::from_secs(60),
            queue_capacity: 1_024,
            polling_fallback: true,
            force_polling: false,
        }
    }
}

impl WatchConfig {
    pub(crate) fn validate(self) -> Result<Self, WatchError> {
        if !(MIN_DEBOUNCE..=MAX_DEBOUNCE).contains(&self.debounce) {
            return Err(WatchError::InvalidConfig(
                "debounce must be between 10 milliseconds and 30 seconds".to_owned(),
            ));
        }
        if !(MIN_RECONCILE..=MAX_RECONCILE).contains(&self.reconcile_interval) {
            return Err(WatchError::InvalidConfig(
                "reconciliation interval must be between 50 milliseconds and 24 hours".to_owned(),
            ));
        }
        if !(MIN_QUEUE_CAPACITY..=MAX_QUEUE_CAPACITY).contains(&self.queue_capacity) {
            return Err(WatchError::InvalidConfig(format!(
                "watch queue capacity must be between {MIN_QUEUE_CAPACITY} and {MAX_QUEUE_CAPACITY}"
            )));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReconcileTrigger {
    Event,
    Overflow,
    Periodic,
}

impl ReconcileTrigger {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Overflow => "overflow",
            Self::Periodic => "periodic",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WatchSnapshot {
    pub enabled: bool,
    pub running: bool,
    pub backend: &'static str,
    pub pending_reconciliation: bool,
    pub reconciling: bool,
    pub queue_capacity: usize,
    pub events_seen: u64,
    pub coalesced_events: u64,
    pub overflow_count: u64,
    pub reconciliation_count: u64,
    pub event_reconciliations: u64,
    pub periodic_reconciliations: u64,
    pub last_trigger: Option<&'static str>,
    pub last_reconciled_unix_ms: Option<u128>,
    pub last_reconciliation_duration_ms: Option<u128>,
    pub last_error: Option<String>,
}

impl WatchSnapshot {
    pub(crate) fn configured(config: WatchConfig) -> Self {
        Self {
            enabled: config.enabled,
            running: false,
            backend: "not_started",
            pending_reconciliation: false,
            reconciling: false,
            queue_capacity: config.queue_capacity,
            events_seen: 0,
            coalesced_events: 0,
            overflow_count: 0,
            reconciliation_count: 0,
            event_reconciliations: 0,
            periodic_reconciliations: 0,
            last_trigger: None,
            last_reconciled_unix_ms: None,
            last_reconciliation_duration_ms: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum WatchError {
    #[error("invalid watcher configuration: {0}")]
    InvalidConfig(String),
    #[error("cannot spawn watcher coordinator: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("watcher coordinator panicked")]
    Panicked,
}

enum WatchSignal {
    Hint(Vec<PathBuf>),
    Wake,
}

pub(crate) struct WatchHandle {
    state: Arc<Mutex<WatchSnapshot>>,
    cancellation: CancellationContext,
    signals: SyncSender<WatchSignal>,
    worker: Option<JoinHandle<()>>,
}

impl WatchHandle {
    pub(crate) fn start<F>(
        root: PathBuf,
        config: WatchConfig,
        reconcile: F,
    ) -> Result<Self, WatchError>
    where
        F: FnMut(ReconcileTrigger, Vec<PathBuf>, &CancellationContext) -> Result<(), String>
            + Send
            + 'static,
    {
        let config = config.validate()?;
        let state = Arc::new(Mutex::new(WatchSnapshot::configured(config)));
        let cancellation = CancellationContext::default();
        let (signals, receiver) = mpsc::sync_channel(config.queue_capacity);
        let worker_state = Arc::clone(&state);
        let worker_cancellation = cancellation.clone();
        let event_signals = signals.clone();
        let worker = thread::Builder::new()
            .name("codeatlas-watch".to_owned())
            .spawn(move || {
                run_watch_loop(
                    root,
                    config,
                    receiver,
                    event_signals,
                    worker_state,
                    worker_cancellation,
                    reconcile,
                );
            })
            .map_err(WatchError::Spawn)?;
        Ok(Self {
            state,
            cancellation,
            signals,
            worker: Some(worker),
        })
    }

    pub(crate) fn snapshot(&self) -> WatchSnapshot {
        self.state.lock().map_or_else(
            |_| {
                let mut snapshot = WatchSnapshot::configured(WatchConfig::default());
                snapshot.enabled = true;
                snapshot.backend = "state_unavailable";
                snapshot.last_error = Some("watch status lock is poisoned".to_owned());
                snapshot
            },
            |state| state.clone(),
        )
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), WatchError> {
        self.request_stop();
        self.join()
    }

    pub(crate) fn request_stop(&self) {
        self.cancellation.cancel();
        let _ = self.signals.try_send(WatchSignal::Wake);
    }

    pub(crate) fn join(&mut self) -> Result<(), WatchError> {
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| WatchError::Panicked)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn inject_hint(&self) {
        if matches!(
            self.signals.try_send(WatchSignal::Hint(Vec::new())),
            Err(TrySendError::Full(_))
        ) {
            update_state(&self.state, |snapshot| {
                snapshot.overflow_count = snapshot.overflow_count.saturating_add(1);
                snapshot.coalesced_events = snapshot.coalesced_events.saturating_add(1);
                snapshot.pending_reconciliation = true;
            });
        }
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn run_watch_loop<F>(
    root: PathBuf,
    config: WatchConfig,
    receiver: Receiver<WatchSignal>,
    signals: SyncSender<WatchSignal>,
    state: Arc<Mutex<WatchSnapshot>>,
    cancellation: CancellationContext,
    mut reconcile: F,
) where
    F: FnMut(ReconcileTrigger, Vec<PathBuf>, &CancellationContext) -> Result<(), String>,
{
    let watcher_result = create_watcher(&root, config, signals, Arc::clone(&state));
    let (watcher, backend) = match watcher_result {
        Ok(value) => value,
        Err(error) => {
            update_state(&state, |snapshot| {
                snapshot.running = false;
                snapshot.backend = "unavailable";
                snapshot.last_error = Some(error);
            });
            return;
        }
    };
    update_state(&state, |snapshot| {
        snapshot.running = true;
        snapshot.backend = backend;
    });

    let mut pending_since: Option<Instant> = None;
    let mut pending_paths = BTreeSet::new();
    let mut next_periodic = Instant::now() + config.reconcile_interval;
    let mut reconciled_overflow_count = 0;
    while !cancellation.is_cancelled() {
        let now = Instant::now();
        let debounce_deadline = pending_since.map(|start| start + config.debounce);
        let deadline = debounce_deadline.map_or(next_periodic, |value| value.min(next_periodic));
        let timeout = deadline.saturating_duration_since(now);
        match receiver.recv_timeout(timeout) {
            Ok(WatchSignal::Hint(paths)) => {
                if pending_since.is_none() {
                    pending_since = Some(Instant::now());
                }
                pending_paths.extend(paths);
                update_state(&state, |snapshot| {
                    snapshot.pending_reconciliation = true;
                });
            }
            Ok(WatchSignal::Wake) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                let event_due = pending_since
                    .is_some_and(|started| now.duration_since(started) >= config.debounce);
                let periodic_due = now >= next_periodic;
                if !event_due && !periodic_due {
                    continue;
                }
                if cancellation.is_cancelled() {
                    break;
                }
                let overflow_count = state.lock().map_or(reconciled_overflow_count, |snapshot| {
                    snapshot.overflow_count
                });
                let trigger = if overflow_count > reconciled_overflow_count && event_due {
                    ReconcileTrigger::Overflow
                } else if event_due {
                    ReconcileTrigger::Event
                } else {
                    ReconcileTrigger::Periodic
                };
                update_state(&state, |snapshot| {
                    snapshot.reconciling = true;
                });
                let paths = std::mem::take(&mut pending_paths)
                    .into_iter()
                    .collect::<Vec<_>>();
                let reconciliation_started = Instant::now();
                let result = reconcile(trigger, paths, &cancellation);
                let reconciliation_duration_ms = reconciliation_started.elapsed().as_millis();
                let reconciled_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|value| value.as_millis());
                update_state(&state, |snapshot| {
                    snapshot.reconciliation_count = snapshot.reconciliation_count.saturating_add(1);
                    match trigger {
                        ReconcileTrigger::Event | ReconcileTrigger::Overflow => {
                            snapshot.event_reconciliations =
                                snapshot.event_reconciliations.saturating_add(1);
                        }
                        ReconcileTrigger::Periodic => {
                            snapshot.periodic_reconciliations =
                                snapshot.periodic_reconciliations.saturating_add(1);
                        }
                    }
                    snapshot.last_trigger = Some(trigger.as_str());
                    snapshot.last_reconciled_unix_ms = reconciled_at;
                    snapshot.last_reconciliation_duration_ms = Some(reconciliation_duration_ms);
                    snapshot.last_error = result.err();
                    snapshot.pending_reconciliation = false;
                    snapshot.reconciling = false;
                });
                pending_since = None;
                reconciled_overflow_count = overflow_count;
                next_periodic = Instant::now() + config.reconcile_interval;
            }
        }
    }
    drop(watcher);
    update_state(&state, |snapshot| {
        snapshot.running = false;
        snapshot.pending_reconciliation = false;
        snapshot.reconciling = false;
    });
}

fn create_watcher(
    root: &std::path::Path,
    config: WatchConfig,
    signals: SyncSender<WatchSignal>,
    state: Arc<Mutex<WatchSnapshot>>,
) -> Result<(Box<dyn Watcher>, &'static str), String> {
    if config.force_polling {
        return create_poll_watcher(root, config, signals, state);
    }
    let handler = event_handler(signals.clone(), Arc::clone(&state));
    let native_config = NotifyConfig::default().with_follow_symlinks(false);
    let native = RecommendedWatcher::new(handler, native_config).and_then(|mut watcher| {
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(watcher)
    });
    match native {
        Ok(watcher) => Ok((
            Box::new(watcher),
            watcher_kind_name(RecommendedWatcher::kind()),
        )),
        Err(native_error) if config.polling_fallback => {
            update_state(&state, |snapshot| {
                snapshot.last_error = Some(format!(
                    "native watcher unavailable; using polling fallback: {native_error}"
                ));
            });
            create_poll_watcher(root, config, signals, state)
        }
        Err(error) => Err(format!("native watcher unavailable: {error}")),
    }
}

fn watcher_kind_name(kind: WatcherKind) -> &'static str {
    match kind {
        WatcherKind::Inotify => "inotify",
        WatcherKind::Fsevent => "fsevent",
        WatcherKind::Kqueue => "kqueue",
        WatcherKind::PollWatcher => "polling",
        WatcherKind::ReadDirectoryChangesWatcher => "read_directory_changes",
        WatcherKind::NullWatcher => "null",
        _ => "native_unknown",
    }
}

fn create_poll_watcher(
    root: &std::path::Path,
    config: WatchConfig,
    signals: SyncSender<WatchSignal>,
    state: Arc<Mutex<WatchSnapshot>>,
) -> Result<(Box<dyn Watcher>, &'static str), String> {
    let handler = event_handler(signals, state);
    let poll_interval = config.reconcile_interval.min(Duration::from_secs(2));
    let poll_config = NotifyConfig::default()
        .with_poll_interval(poll_interval)
        .with_compare_contents(true)
        .with_follow_symlinks(false);
    let mut watcher = PollWatcher::new(handler, poll_config)
        .map_err(|error| format!("polling watcher unavailable: {error}"))?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|error| format!("cannot watch authorized root with polling: {error}"))?;
    Ok((Box::new(watcher), "polling"))
}

fn event_handler(
    signals: SyncSender<WatchSignal>,
    state: Arc<Mutex<WatchSnapshot>>,
) -> impl FnMut(notify::Result<notify::Event>) + Send + 'static {
    move |event| {
        update_state(&state, |snapshot| {
            snapshot.events_seen = snapshot.events_seen.saturating_add(1);
            if let Err(error) = &event {
                snapshot.last_error = Some(format!("watch backend event error: {error}"));
            }
        });
        let paths = event.map_or_else(|_| Vec::new(), |event| event.paths);
        match signals.try_send(WatchSignal::Hint(paths)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => update_state(&state, |snapshot| {
                snapshot.overflow_count = snapshot.overflow_count.saturating_add(1);
                snapshot.coalesced_events = snapshot.coalesced_events.saturating_add(1);
                snapshot.pending_reconciliation = true;
            }),
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

fn update_state(state: &Mutex<WatchSnapshot>, update: impl FnOnce(&mut WatchSnapshot)) {
    if let Ok(mut snapshot) = state.lock() {
        update(&mut snapshot);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Arc, Mutex},
        time::Instant,
    };

    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "codeatlas-watch-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temporary watch root");
        root
    }

    #[test]
    fn native_atomic_save_and_burst_are_coalesced_and_shutdown_is_joined() {
        let root = temp_root("native");
        let triggers = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&triggers);
        let mut handle = WatchHandle::start(
            root.clone(),
            WatchConfig {
                enabled: true,
                debounce: Duration::from_millis(40),
                reconcile_interval: Duration::from_millis(500),
                queue_capacity: 2,
                polling_fallback: true,
                force_polling: false,
            },
            move |trigger, _paths, _| {
                captured.lock().expect("triggers").push(trigger);
                Ok(())
            },
        )
        .expect("start watcher");
        let deadline = Instant::now() + Duration::from_secs(3);
        while !handle.snapshot().running && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(handle.snapshot().running);
        thread::sleep(Duration::from_millis(500));

        for index in 0..32 {
            let temporary = root.join(format!(".save-{index}"));
            fs::write(&temporary, format!("pub fn value_{index}() {{}}\n")).expect("write");
            fs::rename(&temporary, root.join("atomic.rs")).expect("atomic save");
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while triggers.lock().expect("triggers").is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let snapshot = handle.snapshot();
        assert!(!matches!(snapshot.backend, "polling" | "not_started"));
        assert!(snapshot.reconciliation_count > 0);
        assert!(snapshot.events_seen >= snapshot.event_reconciliations);
        handle.shutdown().expect("joined shutdown");
        assert!(!handle.snapshot().running);
        fs::remove_dir_all(root).expect("remove temporary watch root");
    }

    #[test]
    fn simulated_overflow_and_lost_events_are_reconciled() {
        let root = temp_root("overflow");
        let triggers = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&triggers);
        let mut handle = WatchHandle::start(
            root.clone(),
            WatchConfig {
                enabled: true,
                debounce: Duration::from_millis(20),
                reconcile_interval: Duration::from_millis(80),
                queue_capacity: 1,
                polling_fallback: true,
                force_polling: true,
            },
            move |trigger, _paths, _| {
                captured.lock().expect("triggers").push(trigger);
                thread::sleep(Duration::from_millis(30));
                Ok(())
            },
        )
        .expect("start polling watcher");
        for _ in 0..128 {
            handle.inject_hint();
        }
        let pending_deadline = Instant::now() + Duration::from_secs(1);
        while !handle.snapshot().pending_reconciliation && Instant::now() < pending_deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(handle.snapshot().pending_reconciliation);
        let deadline = Instant::now() + Duration::from_secs(3);
        while handle.snapshot().periodic_reconciliations == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.backend, "polling");
        assert!(snapshot.overflow_count > 0);
        assert!(snapshot.reconciliation_count > 0);
        assert!(snapshot.periodic_reconciliations > 0);
        assert!(
            triggers
                .lock()
                .expect("triggers")
                .contains(&ReconcileTrigger::Overflow)
        );
        handle.shutdown().expect("joined shutdown");
        fs::remove_dir_all(root).expect("remove temporary watch root");
    }
}

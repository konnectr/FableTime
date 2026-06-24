//! Shared application state: owns the DB connection, the running-entry
//! snapshot, and the 1-second timer task that drives the live elapsed display.
//!
//! `AppState` is a gpui entity. Views hold an `Entity<AppState>`, observe it
//! for re-render, and mutate it through its methods (which call `cx.notify()`).

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use gpui::{App, Context, Entity, Task};

use crate::db::Db;
use crate::models::Id;

/// Snapshot of the currently-running entry, kept in memory so the Tracker view
/// can render project/task/elapsed without a query per frame.
pub struct RunningInfo {
    pub entry_id: Id,
    pub task_id: Id,
    pub project: String,
    pub task: String,
    pub start: DateTime<Utc>,
}

pub struct AppState {
    pub db: Db,
    pub running: Option<RunningInfo>,
    /// 1s ticker; dropping it cancels the timer (used on Stop).
    tick: Option<Task<()>>,
}

impl AppState {
    /// Open the DB (running migrations), resume any entry left running at the
    /// previous exit, and wrap it all in a gpui entity.
    pub fn load(cx: &mut App) -> Entity<Self> {
        let db = Db::open(&default_db_path()).expect("open database");
        cx.new(|cx| {
            let mut state = Self {
                db,
                running: None,
                tick: None,
            };
            if let Ok(Some(entry)) = state.db.running_entry() {
                let (project, task) = state.db.task_names(entry.task_id).unwrap_or_default();
                state.running = Some(RunningInfo {
                    entry_id: entry.id,
                    task_id: entry.task_id,
                    project,
                    task,
                    start: entry.start(),
                });
                state.start_tick(cx);
            }
            state
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Start tracking `task_id`. The DB layer stops any open entry first, so
    /// only one entry is ever running.
    pub fn start(&mut self, task_id: Id, cx: &mut Context<Self>) {
        match self.db.start_entry(task_id) {
            Ok(entry_id) => {
                let (project, task) = self.db.task_names(task_id).unwrap_or_default();
                self.running = Some(RunningInfo {
                    entry_id,
                    task_id,
                    project,
                    task,
                    start: Utc::now(),
                });
                self.start_tick(cx);
                cx.notify();
            }
            Err(e) => eprintln!("start_entry failed: {e:#}"),
        }
    }

    /// Stop the running entry (if any) and cancel the ticker.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.db.stop_running() {
            eprintln!("stop_running failed: {e:#}");
        }
        self.running = None;
        self.tick = None; // drop cancels the spawned timer
        cx.notify();
    }

    /// Spawn a foreground task that notifies once per second so observing views
    /// recompute elapsed time from `start`. Runs on the main thread, so the
    /// non-Send `Connection` is never moved across threads.
    fn start_tick(&mut self, cx: &mut Context<Self>) {
        self.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                gpui::Timer::after(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break; // entity dropped — stop ticking
                }
            }
        }));
    }
}

/// Per-OS application data path for the SQLite file, created if missing.
pub fn default_db_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("dev", "timetracker", "TimeTracker") {
        let dir = dirs.data_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("timetracker.db")
    } else {
        PathBuf::from("timetracker.db")
    }
}

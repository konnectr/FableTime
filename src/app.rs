//! Shared application state: owns the DB connection, the running-entry
//! snapshot, and the 1-second timer task that drives the live elapsed display.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use gpui::{App, AppContext, Context, Entity, Task};

use crate::db::Db;
use crate::models::Id;

/// In-memory snapshot of the running entry so the Tracker renders without a
/// query per frame.
pub struct RunningInfo {
    pub entry_id: Id,
    pub project_id: Id,
    pub project: String,
    pub color: String, // "#rrggbb"
    pub description: String,
    pub start: DateTime<Utc>,
}

pub struct AppState {
    pub db: Db,
    pub running: Option<RunningInfo>,
    tick: Option<Task<()>>,
}

impl AppState {
    pub fn load(cx: &mut App) -> Entity<Self> {
        let db = Db::open(&default_db_path()).expect("open database");
        cx.new(|cx| {
            let mut state = Self {
                db,
                running: None,
                tick: None,
            };
            if let Ok(Some(entry)) = state.db.running_entry() {
                let (project, color) = state.db.project_meta(entry.project_id).unwrap_or_default();
                state.running = Some(RunningInfo {
                    entry_id: entry.id,
                    project_id: entry.project_id,
                    project,
                    color,
                    description: entry.description.clone().unwrap_or_default(),
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

    /// Start tracking `project_id` with a description (stops any open entry).
    pub fn start(&mut self, project_id: Id, description: &str, cx: &mut Context<Self>) {
        match self.db.start_entry(project_id, description) {
            Ok(entry_id) => {
                let (project, color) = self.db.project_meta(project_id).unwrap_or_default();
                self.running = Some(RunningInfo {
                    entry_id,
                    project_id,
                    project,
                    color,
                    description: description.trim().to_string(),
                    start: Utc::now(),
                });
                self.start_tick(cx);
                cx.notify();
            }
            Err(e) => eprintln!("start_entry failed: {e:#}"),
        }
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.db.stop_running() {
            eprintln!("stop_running failed: {e:#}");
        }
        self.running = None;
        self.tick = None;
        cx.notify();
    }

    /// Live-edit the running entry's description (kept in DB + snapshot).
    pub fn set_running_desc(&mut self, desc: &str, cx: &mut Context<Self>) {
        if let Some(r) = self.running.as_mut() {
            let d = desc.trim();
            r.description = d.to_string();
            let _ = self.db.update_entry(
                r.entry_id,
                r.project_id,
                r.start,
                None,
                (!d.is_empty()).then_some(d),
            );
            cx.notify();
        }
    }

    fn start_tick(&mut self, cx: &mut Context<Self>) {
        self.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        }));
    }
}

pub fn default_db_path() -> PathBuf {
    // Override (tests / demo) — keeps the real DB untouched.
    if let Ok(p) = std::env::var("TIMETRACKER_DB") {
        return PathBuf::from(p);
    }
    if let Some(dirs) = directories::ProjectDirs::from("dev", "timetracker", "TimeTracker") {
        let dir = dirs.data_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("timetracker.db")
    } else {
        PathBuf::from("timetracker.db")
    }
}

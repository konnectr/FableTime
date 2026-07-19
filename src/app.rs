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
    pub note: String,
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
        let state = cx.new(|cx| {
            let mut state = Self {
                db,
                running: None,
                tick: None,
            };
            // Close a timer left running by a previous crash / hard shutdown,
            // trimming it back to its last heartbeat before we read the snapshot.
            if let Err(e) = state.db.reconcile_running() {
                eprintln!("reconcile_running failed: {e:#}");
            }
            if let Ok(Some(entry)) = state.db.running_entry() {
                let (project, color) = state.db.project_meta(entry.project_id).unwrap_or_default();
                state.running = Some(RunningInfo {
                    entry_id: entry.id,
                    project_id: entry.project_id,
                    project,
                    color,
                    description: entry.description.clone().unwrap_or_default(),
                    note: entry.note.clone().unwrap_or_default(),
                    start: entry.start(),
                });
                state.start_tick(cx);
            }
            state
        });

        // Stop the running timer when the app terminates (Cmd+Q, or the OS asking
        // us to quit on logout / shutdown) so it doesn't keep counting across the
        // closed period. A hard power-off can't be caught — nothing runs then.
        let handle = state.clone();
        cx.on_app_quit(move |cx| {
            handle.update(cx, |s, _| {
                if s.running.is_some() {
                    if let Err(e) = s.db.stop_running() {
                        eprintln!("stop_running on quit failed: {e:#}");
                    }
                    let _ = s.db.clear_heartbeat();
                    s.running = None;
                }
            });
            async {}
        })
        .detach();

        state
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Start tracking `project_id` with a description (stops any open entry).
    pub fn start(&mut self, project_id: Id, description: &str, cx: &mut Context<Self>) {
        match self.db.start_entry(project_id, description) {
            Ok(entry_id) => {
                let _ = self.db.heartbeat();
                let (project, color) = self.db.project_meta(project_id).unwrap_or_default();
                self.running = Some(RunningInfo {
                    entry_id,
                    project_id,
                    project,
                    color,
                    description: description.trim().to_string(),
                    note: String::new(),
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
        let _ = self.db.clear_heartbeat();
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

    /// Live-edit the running entry's detail note (kept in DB + snapshot).
    pub fn set_running_note(&mut self, note: &str, cx: &mut Context<Self>) {
        if let Some(r) = self.running.as_mut() {
            r.note = note.to_string();
            let _ = self.db.set_entry_note(r.entry_id, Some(note));
            cx.notify();
        }
    }

    fn start_tick(&mut self, cx: &mut Context<Self>) {
        self.tick = Some(cx.spawn(async move |this, cx| {
            let mut ticks: u32 = 0;
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                ticks += 1;
                let beat = ticks % 15 == 0; // persist a heartbeat every ~15s
                if this
                    .update(cx, |s, cx| {
                        if beat {
                            let _ = s.db.heartbeat();
                        }
                        cx.notify();
                    })
                    .is_err()
                {
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

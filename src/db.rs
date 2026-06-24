//! All SQLite access: connection, migrations, CRUD, day totals, range export.
//!
//! The `Connection` is single-threaded and lives on the main thread inside
//! `AppState`; queries are tiny/local so they run synchronously. Never share
//! the `Connection` across threads.

use anyhow::{Context as _, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

use crate::models::*;

/// Schema migrations applied in order against `PRAGMA user_version`.
/// Append-only: never edit an existing entry, add a new one.
const MIGRATIONS: &[&str] = &[
    // v1 — initial schema
    r#"
    CREATE TABLE projects (
      id         INTEGER PRIMARY KEY,
      name       TEXT NOT NULL,
      color      TEXT,
      archived   INTEGER NOT NULL DEFAULT 0,
      created_at TEXT NOT NULL
    );
    CREATE TABLE tasks (
      id         INTEGER PRIMARY KEY,
      project_id INTEGER NOT NULL REFERENCES projects(id),
      name       TEXT NOT NULL,
      archived   INTEGER NOT NULL DEFAULT 0,
      created_at TEXT NOT NULL
    );
    CREATE TABLE time_entries (
      id         INTEGER PRIMARY KEY,
      task_id    INTEGER NOT NULL REFERENCES tasks(id),
      start_ts   TEXT NOT NULL,
      end_ts     TEXT,
      note       TEXT,
      created_at TEXT NOT NULL
    );
    CREATE INDEX idx_entries_start ON time_entries(start_ts);
    CREATE INDEX idx_entries_task  ON time_entries(task_id);
    "#,
];

/// A time entry joined with its project/task names — for list/detail views.
#[derive(Debug, Clone)]
pub struct EntryDetail {
    pub entry: TimeEntry,
    pub project_id: Id,
    pub project: String,
    pub task: String,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("open db at {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA journal_mode = WAL;",
        )?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    /// In-memory DB for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    // --- projects -----------------------------------------------------------

    pub fn create_project(&self, name: &str, color: Option<&str>) -> Result<Id> {
        self.conn.execute(
            "INSERT INTO projects (name, color, archived, created_at) VALUES (?1, ?2, 0, ?3)",
            params![name, color, now_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color, archived, created_at
             FROM projects WHERE archived = 0 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_project)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn archive_project(&self, id: Id) -> Result<()> {
        self.conn
            .execute("UPDATE projects SET archived = 1 WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- tasks --------------------------------------------------------------

    pub fn create_task(&self, project_id: Id, name: &str) -> Result<Id> {
        self.conn.execute(
            "INSERT INTO tasks (project_id, name, archived, created_at) VALUES (?1, ?2, 0, ?3)",
            params![project_id, name, now_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_tasks(&self, project_id: Id) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, name, archived, created_at
             FROM tasks WHERE project_id = ?1 AND archived = 0 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![project_id], row_to_task)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Project name + task name for a task id (for the running-entry banner).
    pub fn task_names(&self, task_id: Id) -> Result<(String, String)> {
        self.conn
            .query_row(
                "SELECT p.name, t.name FROM tasks t
                 JOIN projects p ON p.id = t.project_id WHERE t.id = ?1",
                params![task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(Into::into)
    }

    // --- time entries -------------------------------------------------------

    /// Start a live timer on `task_id`. Enforces the single-running-entry rule
    /// by stopping any currently open entry first. Returns the new row id.
    pub fn start_entry(&self, task_id: Id) -> Result<Id> {
        self.stop_running()?;
        let now = now_rfc3339();
        self.conn.execute(
            "INSERT INTO time_entries (task_id, start_ts, end_ts, note, created_at)
             VALUES (?1, ?2, NULL, NULL, ?3)",
            params![task_id, now, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Close any open entry (end_ts == NULL) at the current instant.
    pub fn stop_running(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE time_entries SET end_ts = ?1 WHERE end_ts IS NULL",
            params![now_rfc3339()],
        )?;
        Ok(())
    }

    /// The currently-running entry, if any.
    pub fn running_entry(&self) -> Result<Option<TimeEntry>> {
        self.conn
            .query_row(
                "SELECT id, task_id, start_ts, end_ts, note, created_at
                 FROM time_entries WHERE end_ts IS NULL ORDER BY start_ts DESC LIMIT 1",
                [],
                row_to_entry,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Insert a fully-specified (manual) entry. Same row shape as a live timer.
    pub fn add_manual_entry(
        &self,
        task_id: Id,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        note: Option<&str>,
    ) -> Result<Id> {
        self.conn.execute(
            "INSERT INTO time_entries (task_id, start_ts, end_ts, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, to_rfc3339(start), to_rfc3339(end), note, now_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_entry(
        &self,
        id: Id,
        task_id: Id,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        note: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE time_entries SET task_id = ?2, start_ts = ?3, end_ts = ?4, note = ?5
             WHERE id = ?1",
            params![id, task_id, to_rfc3339(start), end.map(to_rfc3339), note],
        )?;
        Ok(())
    }

    pub fn delete_entry(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM time_entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// All entries that *started* on the given local calendar day, with names,
    /// ordered by start time.
    pub fn entries_for_day(&self, date: NaiveDate) -> Result<Vec<EntryDetail>> {
        let (lo, hi) = local_day_bounds_utc(date);
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.task_id, e.start_ts, e.end_ts, e.note, e.created_at,
                    t.project_id AS project_id, p.name AS project, t.name AS task
             FROM time_entries e
             JOIN tasks t    ON t.id = e.task_id
             JOIN projects p ON p.id = t.project_id
             WHERE e.start_ts >= ?1 AND e.start_ts < ?2
             ORDER BY e.start_ts ASC",
        )?;
        let rows = stmt.query_map(params![lo, hi], |r| {
            Ok(EntryDetail {
                entry: row_to_entry(r)?,
                project_id: r.get("project_id")?,
                project: r.get("project")?,
                task: r.get("task")?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Total tracked seconds for a local day. A running entry counts up to now.
    pub fn day_total_secs(&self, date: NaiveDate) -> Result<i64> {
        let now = Utc::now();
        Ok(self
            .entries_for_day(date)?
            .iter()
            .map(|d| d.entry.duration_secs(now))
            .sum())
    }

    /// Flattened export rows for an inclusive local date range `[from, to]`.
    pub fn entries_in_range(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<ExportRow>> {
        let (lo, _) = local_day_bounds_utc(from);
        let (_, hi) = local_day_bounds_utc(to);
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.task_id, e.start_ts, e.end_ts, e.note, e.created_at,
                    p.name AS project, t.name AS task
             FROM time_entries e
             JOIN tasks t    ON t.id = e.task_id
             JOIN projects p ON p.id = t.project_id
             WHERE e.start_ts >= ?1 AND e.start_ts < ?2
             ORDER BY e.start_ts ASC",
        )?;
        let now = Utc::now();
        let rows = stmt.query_map(params![lo, hi], |r| {
            let entry = row_to_entry(r)?;
            let project: String = r.get("project")?;
            let task: String = r.get("task")?;
            Ok((entry, project, task))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (entry, project, task) = row?;
            let secs = entry.duration_secs(now);
            out.push(ExportRow {
                date: local_ymd(entry.start()),
                project,
                task,
                start: local_hm(entry.start()),
                end: entry.end().map(local_hm).unwrap_or_default(),
                duration_secs: secs,
                duration_hms: format_hms(secs),
                note: entry.note.clone().unwrap_or_default(),
            });
        }
        Ok(out)
    }
}

// --- migration runner -------------------------------------------------------

fn migrate(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let mut v = current as usize;
    while v < MIGRATIONS.len() {
        conn.execute_batch(MIGRATIONS[v])
            .with_context(|| format!("apply migration v{}", v + 1))?;
        v += 1;
    }
    if v as i64 != current {
        // PRAGMA cannot be parameterized; v is a trusted array length.
        conn.execute_batch(&format!("PRAGMA user_version = {v};"))?;
    }
    Ok(())
}

// --- row mappers ------------------------------------------------------------

fn row_to_project(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get("id")?,
        name: r.get("name")?,
        color: r.get("color")?,
        archived: r.get::<_, i64>("archived")? != 0,
        created_at: r.get("created_at")?,
    })
}

fn row_to_task(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get("id")?,
        project_id: r.get("project_id")?,
        name: r.get("name")?,
        archived: r.get::<_, i64>("archived")? != 0,
        created_at: r.get("created_at")?,
    })
}

fn row_to_entry(r: &Row) -> rusqlite::Result<TimeEntry> {
    Ok(TimeEntry {
        id: r.get("id")?,
        task_id: r.get("task_id")?,
        start_ts: r.get("start_ts")?,
        end_ts: r.get("end_ts")?,
        note: r.get("note")?,
        created_at: r.get("created_at")?,
    })
}

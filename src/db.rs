//! All SQLite access: connection, migrations, CRUD, day/week totals, export.
//!
//! Entries belong directly to a project and carry a free-text description.
//! The `Connection` is single-threaded and lives on the main thread inside
//! `AppState`; never share it across threads.

use anyhow::{Context as _, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::models::*;
use crate::palette;
use std::path::Path;

/// Schema migrations applied in order against `PRAGMA user_version`. Append-only.
const MIGRATIONS: &[&str] = &[
    // v1 — original project→task→entry schema.
    r#"
    CREATE TABLE projects (
      id INTEGER PRIMARY KEY, name TEXT NOT NULL, color TEXT,
      archived INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
    );
    CREATE TABLE tasks (
      id INTEGER PRIMARY KEY, project_id INTEGER NOT NULL REFERENCES projects(id),
      name TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
    );
    CREATE TABLE time_entries (
      id INTEGER PRIMARY KEY, task_id INTEGER NOT NULL REFERENCES tasks(id),
      start_ts TEXT NOT NULL, end_ts TEXT, note TEXT, created_at TEXT NOT NULL
    );
    CREATE INDEX idx_entries_start ON time_entries(start_ts);
    CREATE INDEX idx_entries_task  ON time_entries(task_id);
    "#,
    // v2 — entries belong to a project + carry a description; drop the task layer.
    r#"
    ALTER TABLE projects ADD COLUMN client TEXT;
    UPDATE projects SET color = '#4f46e5' WHERE color IS NULL OR color = '';

    CREATE TABLE time_entries_v2 (
      id INTEGER PRIMARY KEY,
      project_id INTEGER NOT NULL REFERENCES projects(id),
      description TEXT,
      start_ts TEXT NOT NULL,
      end_ts TEXT,
      created_at TEXT NOT NULL
    );
    INSERT INTO time_entries_v2 (id, project_id, description, start_ts, end_ts, created_at)
      SELECT e.id, t.project_id,
             COALESCE(NULLIF(TRIM(e.note), ''), t.name),
             e.start_ts, e.end_ts, e.created_at
      FROM time_entries e JOIN tasks t ON t.id = e.task_id;
    DROP TABLE time_entries;
    ALTER TABLE time_entries_v2 RENAME TO time_entries;
    DROP TABLE tasks;
    CREATE INDEX idx_entries_start   ON time_entries(start_ts);
    CREATE INDEX idx_entries_project ON time_entries(project_id);
    "#,
];

/// A time entry joined with its project name + color — for list/detail views.
#[derive(Debug, Clone)]
pub struct EntryDetail {
    pub entry: TimeEntry,
    pub project_id: Id,
    pub project: String,
    pub color: String,
}

/// Per-project weekly rollup for the Projects tab.
#[derive(Debug, Clone)]
pub struct ProjectStat {
    pub project: Project,
    pub week_secs: i64,
    pub entry_count: i64,
    pub per_day_secs: [i64; 7], // Mon..Sun
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn =
            Connection::open(path).with_context(|| format!("open db at {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL;",
        )?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    // --- projects -----------------------------------------------------------

    fn count_projects(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM projects WHERE archived = 0", [], |r| r.get(0))?)
    }

    /// Create a project, auto-assigning the next palette color.
    pub fn create_project(&self, name: &str, client: Option<&str>) -> Result<Id> {
        let count = self.count_projects()? as usize;
        let color = palette::u32_to_hex(palette::nth_palette(count).main);
        self.conn.execute(
            "INSERT INTO projects (name, client, color, archived, created_at)
             VALUES (?1, ?2, ?3, 0, ?4)",
            params![name, client, color, now_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, client, color, archived, created_at
             FROM projects WHERE archived = 0 ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], row_to_project)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn archive_project(&self, id: Id) -> Result<()> {
        self.conn
            .execute("UPDATE projects SET archived = 1 WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// (name, color hex) for a project — for the running-entry banner.
    pub fn project_meta(&self, id: Id) -> Result<(String, String)> {
        self.conn
            .query_row(
                "SELECT name, color FROM projects WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(Into::into)
    }

    // --- time entries -------------------------------------------------------

    /// Start a live timer on `project_id` with a description. Stops any open
    /// entry first (single running entry).
    pub fn start_entry(&self, project_id: Id, description: &str) -> Result<Id> {
        self.stop_running()?;
        let now = now_rfc3339();
        let desc = description.trim();
        let desc_opt = (!desc.is_empty()).then_some(desc);
        self.conn.execute(
            "INSERT INTO time_entries (project_id, description, start_ts, end_ts, created_at)
             VALUES (?1, ?2, ?3, NULL, ?4)",
            params![project_id, desc_opt, now, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn stop_running(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE time_entries SET end_ts = ?1 WHERE end_ts IS NULL",
            params![now_rfc3339()],
        )?;
        Ok(())
    }

    pub fn running_entry(&self) -> Result<Option<TimeEntry>> {
        self.conn
            .query_row(
                "SELECT id, project_id, description, start_ts, end_ts, created_at
                 FROM time_entries WHERE end_ts IS NULL ORDER BY start_ts DESC LIMIT 1",
                [],
                row_to_entry,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn add_manual_entry(
        &self,
        project_id: Id,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        description: Option<&str>,
    ) -> Result<Id> {
        self.conn.execute(
            "INSERT INTO time_entries (project_id, description, start_ts, end_ts, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, description, to_rfc3339(start), to_rfc3339(end), now_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_entry(
        &self,
        id: Id,
        project_id: Id,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        description: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE time_entries SET project_id = ?2, start_ts = ?3, end_ts = ?4, description = ?5
             WHERE id = ?1",
            params![id, project_id, to_rfc3339(start), end.map(to_rfc3339), description],
        )?;
        Ok(())
    }

    pub fn delete_entry(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM time_entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn entries_between(&self, lo: &str, hi: &str) -> Result<Vec<EntryDetail>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.project_id, e.description, e.start_ts, e.end_ts, e.created_at,
                    p.name AS project, p.color AS color
             FROM time_entries e JOIN projects p ON p.id = e.project_id
             WHERE e.start_ts >= ?1 AND e.start_ts < ?2
             ORDER BY e.start_ts ASC",
        )?;
        let rows = stmt.query_map(params![lo, hi], |r| {
            Ok(EntryDetail {
                entry: row_to_entry(r)?,
                project_id: r.get("project_id")?,
                project: r.get("project")?,
                color: r.get("color")?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn entries_for_day(&self, date: NaiveDate) -> Result<Vec<EntryDetail>> {
        let (lo, hi) = local_day_bounds_utc(date);
        self.entries_between(&lo, &hi)
    }

    pub fn entries_for_week(&self, monday: NaiveDate) -> Result<Vec<EntryDetail>> {
        let (lo, _) = local_day_bounds_utc(monday);
        let (hi, _) = local_day_bounds_utc(monday + Duration::days(7));
        self.entries_between(&lo, &hi)
    }

    pub fn day_total_secs(&self, date: NaiveDate) -> Result<i64> {
        let now = Utc::now();
        Ok(self
            .entries_for_day(date)?
            .iter()
            .map(|d| d.entry.duration_secs(now))
            .sum())
    }

    /// Per-project weekly rollup (totals, counts, per-weekday seconds).
    pub fn project_stats(&self, monday: NaiveDate) -> Result<Vec<ProjectStat>> {
        let projects = self.list_projects()?;
        let week = self.entries_for_week(monday)?;
        let days = week_days(monday);
        let now = Utc::now();
        let stats = projects
            .into_iter()
            .map(|p| {
                let mut week_secs = 0;
                let mut entry_count = 0;
                let mut per_day_secs = [0i64; 7];
                for e in week.iter().filter(|e| e.project_id == p.id) {
                    let secs = e.entry.duration_secs(now);
                    week_secs += secs;
                    entry_count += 1;
                    if let Some(i) = days.iter().position(|d| *d == e.entry.local_date()) {
                        per_day_secs[i] += secs;
                    }
                }
                ProjectStat { project: p, week_secs, entry_count, per_day_secs }
            })
            .collect();
        Ok(stats)
    }

    /// Flattened export rows for an inclusive local date range `[from, to]`.
    pub fn entries_in_range(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<ExportRow>> {
        let (lo, _) = local_day_bounds_utc(from);
        let (_, hi) = local_day_bounds_utc(to);
        let now = Utc::now();
        let rows = self
            .entries_between(&lo, &hi)?
            .into_iter()
            .map(|d| {
                let secs = d.entry.duration_secs(now);
                ExportRow {
                    date: local_ymd(d.entry.start()),
                    project: d.project,
                    description: d.entry.desc_or(""),
                    start: local_hm(d.entry.start()),
                    end: d.entry.end().map(local_hm).unwrap_or_default(),
                    duration_secs: secs,
                    duration_hms: format_hms(secs),
                }
            })
            .collect();
        Ok(rows)
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
        conn.execute_batch(&format!("PRAGMA user_version = {v};"))?;
    }
    Ok(())
}

// --- row mappers ------------------------------------------------------------

fn row_to_project(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get("id")?,
        name: r.get("name")?,
        client: r.get("client")?,
        color: r.get::<_, Option<String>>("color")?.unwrap_or_else(|| "#4f46e5".into()),
        archived: r.get::<_, i64>("archived")? != 0,
        created_at: r.get("created_at")?,
    })
}

fn row_to_entry(r: &Row) -> rusqlite::Result<TimeEntry> {
    Ok(TimeEntry {
        id: r.get("id")?,
        project_id: r.get("project_id")?,
        description: r.get("description")?,
        start_ts: r.get("start_ts")?,
        end_ts: r.get("end_ts")?,
        created_at: r.get("created_at")?,
    })
}

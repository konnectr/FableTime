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
    // v3 — a free-text detail note on entries, and a note on projects.
    r#"
    ALTER TABLE time_entries ADD COLUMN note TEXT;
    ALTER TABLE projects     ADD COLUMN note TEXT;
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

/// Per-project rollup for the Projects tab (this-week + all-time figures).
#[derive(Debug, Clone)]
pub struct ProjectStat {
    pub project: Project,
    pub week_secs: i64,
    pub entry_count: i64,       // entries this week
    pub total_secs: i64,        // all-time tracked
    pub total_entries: i64,     // all-time entry count
    pub per_day_secs: [i64; 7], // Mon..Sun
}

/// One entry line inside a project's history detail.
#[derive(Debug, Clone)]
pub struct HistItem {
    pub id: Id,
    pub desc: String,
    pub range: String,        // "HH:MM – HH:MM" (local)
    pub dur_label: String,    // "1ч 45м"
    pub color: String,        // project color hex
    pub note: Option<String>, // free-text detail note (the "comment")
}

/// A day-group in a project's history: date header + day total + its entries.
#[derive(Debug, Clone)]
pub struct DayGroup {
    pub date_label: String,  // "Пн, 7 июля"
    pub total_label: String, // day total, "3ч 10м"
    pub items: Vec<HistItem>,
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
            "SELECT id, name, client, color, note, archived, created_at
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

    /// Set (or clear) a project's note. Blank/whitespace stores NULL.
    pub fn set_project_note(&self, id: Id, note: Option<&str>) -> Result<()> {
        let n = note.map(str::trim).filter(|s| !s.is_empty());
        self.conn
            .execute("UPDATE projects SET note = ?2 WHERE id = ?1", params![id, n])?;
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
                "SELECT id, project_id, description, note, start_ts, end_ts, created_at
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

    /// Set (or clear) an entry's detail note. Blank/whitespace stores NULL so
    /// "has a note" is a plain `note IS NOT NULL` check.
    pub fn set_entry_note(&self, id: Id, note: Option<&str>) -> Result<()> {
        let n = note.map(str::trim).filter(|s| !s.is_empty());
        self.conn
            .execute("UPDATE time_entries SET note = ?2 WHERE id = ?1", params![id, n])?;
        Ok(())
    }

    fn entries_between(&self, lo: &str, hi: &str) -> Result<Vec<EntryDetail>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.project_id, e.description, e.note, e.start_ts, e.end_ts, e.created_at,
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

    /// Per-project rollup: this-week totals/counts/per-weekday seconds, plus
    /// all-time tracked seconds and entry count.
    pub fn project_stats(&self, monday: NaiveDate) -> Result<Vec<ProjectStat>> {
        let projects = self.list_projects()?;
        let week = self.entries_for_week(monday)?;
        let totals = self.project_totals()?;
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
                let (total_secs, total_entries) = totals.get(&p.id).copied().unwrap_or((0, 0));
                ProjectStat { project: p, week_secs, entry_count, total_secs, total_entries, per_day_secs }
            })
            .collect();
        Ok(stats)
    }

    /// All-time (seconds, entry count) per project id, across every entry. A
    /// running entry counts up to now.
    fn project_totals(&self) -> Result<std::collections::HashMap<Id, (i64, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT project_id, start_ts, end_ts FROM time_entries")?;
        let now = Utc::now();
        let mut map: std::collections::HashMap<Id, (i64, i64)> = std::collections::HashMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Id>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (pid, start, end) = row?;
            let s = parse_ts(&start);
            let e = end.as_deref().map(parse_ts).unwrap_or(now);
            let secs = (e - s).num_seconds().max(0);
            let ent = map.entry(pid).or_insert((0, 0));
            ent.0 += secs;
            ent.1 += 1;
        }
        Ok(map)
    }

    /// Full entry history for a project, grouped by local day, most recent first.
    pub fn project_history(&self, project_id: Id) -> Result<Vec<DayGroup>> {
        let color = self.project_meta(project_id).map(|(_, c)| c).unwrap_or_default();
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, description, note, start_ts, end_ts, created_at
             FROM time_entries WHERE project_id = ?1 ORDER BY start_ts DESC",
        )?;
        let now = Utc::now();
        let entries = stmt
            .query_map(params![project_id], row_to_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        // Rows are sorted by start desc, so equal local days are already adjacent.
        let mut acc: Vec<(NaiveDate, i64, Vec<HistItem>)> = Vec::new();
        for e in &entries {
            let d = e.local_date();
            let secs = e.duration_secs(now);
            if acc.last().map(|g| g.0) != Some(d) {
                acc.push((d, 0, Vec::new()));
            }
            let range = format!(
                "{} – {}",
                local_hm(e.start()),
                e.end().map(local_hm).unwrap_or_else(|| "…".into())
            );
            let g = acc.last_mut().unwrap();
            g.1 += secs;
            g.2.push(HistItem {
                id: e.id,
                desc: e.desc_or("Без названия"),
                range,
                dur_label: format_dur_ru(secs),
                color: color.clone(),
                note: e.note.clone(),
            });
        }
        Ok(acc
            .into_iter()
            .map(|(d, secs, items)| DayGroup {
                date_label: ru_date_label(d),
                total_label: format_dur_ru(secs),
                items,
            })
            .collect())
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
        note: r.get("note")?,
        archived: r.get::<_, i64>("archived")? != 0,
        created_at: r.get("created_at")?,
    })
}

fn row_to_entry(r: &Row) -> rusqlite::Result<TimeEntry> {
    Ok(TimeEntry {
        id: r.get("id")?,
        project_id: r.get("project_id")?,
        description: r.get("description")?,
        note: r.get("note")?,
        start_ts: r.get("start_ts")?,
        end_ts: r.get("end_ts")?,
        created_at: r.get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn migration_reaches_v3() {
        let db = Db::open_in_memory().unwrap();
        let v: i64 = db.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        assert_eq!(v, 3);
    }

    #[test]
    fn project_note_roundtrips_and_blanks_to_null() {
        let db = Db::open_in_memory().unwrap();
        let pid = db.create_project("Proj", Some("Acme")).unwrap();
        let get = |db: &Db| db.list_projects().unwrap().into_iter().find(|p| p.id == pid).unwrap().note;

        assert_eq!(get(&db), None);
        db.set_project_note(pid, Some("  https://x.io  ")).unwrap();
        assert_eq!(get(&db).as_deref(), Some("https://x.io")); // trimmed
        db.set_project_note(pid, Some("   ")).unwrap();
        assert_eq!(get(&db), None); // blank -> NULL
    }

    #[test]
    fn entry_note_survives_running_stop_and_time_edit() {
        let db = Db::open_in_memory().unwrap();
        let pid = db.create_project("Proj", None).unwrap();
        let eid = db.start_entry(pid, "desc").unwrap();

        db.set_entry_note(eid, Some("what I did")).unwrap();
        assert_eq!(db.running_entry().unwrap().unwrap().note.as_deref(), Some("what I did"));

        db.stop_running().unwrap();
        // Editing time/description via update_entry must NOT clobber the note.
        let start = Utc::now();
        db.update_entry(eid, pid, start, Some(start + Duration::minutes(5)), Some("newdesc"))
            .unwrap();

        let list = db.entries_between("1970-01-01T00:00:00Z", "2999-01-01T00:00:00Z").unwrap();
        let e = list.iter().find(|e| e.entry.id == eid).unwrap();
        assert_eq!(e.entry.note.as_deref(), Some("what I did"));
        assert_eq!(e.entry.description.as_deref(), Some("newdesc"));
    }
}

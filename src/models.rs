//! Plain data types mirroring the SQLite rows, plus pure time helpers.
//!
//! Timestamps are stored as **UTC RFC3339** (e.g. `2026-06-24T10:05:30Z`).
//! UTC + a fixed `Z` suffix means lexicographic string order == chronological
//! order, so range queries on `start_ts` are correct with plain `<`/`>=`.
//! Conversion to the user's local zone happens only for display / day grouping.

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc};
use serde::Serialize;

pub type Id = i64;

#[derive(Debug, Clone)]
pub struct Project {
    pub id: Id,
    pub name: String,
    pub color: Option<String>,
    pub archived: bool,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: Id,
    pub project_id: Id,
    pub name: String,
    pub archived: bool,
    pub created_at: String,
}

/// A unit of tracked time. A live timer and a manual calendar entry are the
/// same row — manual entries simply set both `start_ts` and `end_ts` up front.
/// `end_ts == None` means the entry is currently running.
#[derive(Debug, Clone)]
pub struct TimeEntry {
    pub id: Id,
    pub task_id: Id,
    pub start_ts: String,
    pub end_ts: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

impl TimeEntry {
    pub fn start(&self) -> DateTime<Utc> {
        parse_ts(&self.start_ts)
    }

    pub fn end(&self) -> Option<DateTime<Utc>> {
        self.end_ts.as_deref().map(parse_ts)
    }

    pub fn is_running(&self) -> bool {
        self.end_ts.is_none()
    }

    /// Duration in seconds. For a running entry, measured up to `now`.
    pub fn duration_secs(&self, now: DateTime<Utc>) -> i64 {
        let end = self.end().unwrap_or(now);
        (end - self.start()).num_seconds().max(0)
    }

    /// Local calendar date the entry started on (used for day grouping).
    pub fn local_date(&self) -> NaiveDate {
        self.start().with_timezone(&Local).date_naive()
    }
}

/// One flattened, display-ready row for CSV / JSON / Markdown export.
#[derive(Debug, Clone, Serialize)]
pub struct ExportRow {
    pub date: String,     // local YYYY-MM-DD
    pub project: String,
    pub task: String,
    pub start: String,    // local HH:MM
    pub end: String,      // local HH:MM, or "" if still running
    pub duration_secs: i64,
    pub duration_hms: String,
    pub note: String,
}

// --- time helpers -----------------------------------------------------------

/// Parse a stored UTC RFC3339 timestamp. Falls back to the Unix epoch on
/// corrupt data so the UI degrades visibly (1970) rather than panicking.
pub fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

/// Current instant as the canonical stored string (UTC, second precision, `Z`).
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Serialize a UTC instant to the canonical stored string.
pub fn to_rfc3339(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Build a stored UTC string from a local wall-clock date + (hour, minute).
/// Used by manual calendar entries where the user picks a day and a time.
pub fn local_hm_to_utc(date: NaiveDate, hour: u32, minute: u32) -> Option<DateTime<Utc>> {
    let naive = date.and_hms_opt(hour, minute, 0)?;
    match Local.from_local_datetime(&naive).single() {
        Some(local) => Some(local.with_timezone(&Utc)),
        None => None, // ambiguous/nonexistent (DST gap) — caller picks another time
    }
}

/// Half-open UTC bounds `[start, end)` of a local calendar day, as stored
/// strings — drop-in for `WHERE start_ts >= ?1 AND start_ts < ?2`.
pub fn local_day_bounds_utc(date: NaiveDate) -> (String, String) {
    let start = local_hm_to_utc(date, 0, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let next = date.succ_opt().unwrap_or(date);
    let end = local_hm_to_utc(next, 0, 0).unwrap_or(start);
    (to_rfc3339(start), to_rfc3339(end))
}

/// `HH:MM:SS`, clamped at zero.
pub fn format_hms(secs: i64) -> String {
    let s = secs.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Decimal hours (e.g. 1.5) for export footers.
pub fn hours_decimal(secs: i64) -> f64 {
    secs as f64 / 3600.0
}

/// Local `YYYY-MM-DD` for a stored instant.
pub fn local_ymd(ts: DateTime<Utc>) -> String {
    let d = ts.with_timezone(&Local);
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

/// Local `HH:MM` for a stored instant.
pub fn local_hm(ts: DateTime<Utc>) -> String {
    let d = ts.with_timezone(&Local);
    use chrono::Timelike;
    format!("{:02}:{:02}", d.hour(), d.minute())
}

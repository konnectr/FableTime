//! Plain data types mirroring the SQLite rows, plus pure time helpers.
//!
//! Data model (matches the approved mockup): a time entry belongs directly to a
//! **project** and carries a free-text **description** — there is no separate
//! "task" layer. Timestamps are stored as **UTC RFC3339** (`...Z`) so
//! lexicographic order == chronological order; convert to local only for
//! display / day grouping.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::Serialize;

pub type Id = i64;

#[derive(Debug, Clone)]
pub struct Project {
    pub id: Id,
    pub name: String,
    pub client: Option<String>,
    pub color: String, // "#rrggbb"
    pub archived: bool,
    pub created_at: String,
}

/// A unit of tracked time: a project + description, with a start and an
/// optional end (`None` = running). A live timer and a manual calendar entry
/// are the same row.
#[derive(Debug, Clone)]
pub struct TimeEntry {
    pub id: Id,
    pub project_id: Id,
    pub description: Option<String>,
    pub start_ts: String,
    pub end_ts: Option<String>,
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
    /// Duration in seconds; running entries measure to `now`.
    pub fn duration_secs(&self, now: DateTime<Utc>) -> i64 {
        let end = self.end().unwrap_or(now);
        (end - self.start()).num_seconds().max(0)
    }
    pub fn local_date(&self) -> NaiveDate {
        self.start().with_timezone(&Local).date_naive()
    }
    pub fn desc_or(&self, fallback: &str) -> String {
        match self.description.as_deref().map(str::trim) {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => fallback.to_string(),
        }
    }
}

/// One flattened, display-ready row for CSV / JSON / Markdown export.
#[derive(Debug, Clone, Serialize)]
pub struct ExportRow {
    pub date: String,        // local YYYY-MM-DD
    pub project: String,
    pub description: String,
    pub start: String,       // local HH:MM
    pub end: String,         // local HH:MM, or "" if running
    pub duration_secs: i64,
    pub duration_hms: String,
}

// --- time helpers -----------------------------------------------------------

pub fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn to_rfc3339(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Parse an `HH:MM` string into (hour, minute), validating ranges.
pub fn parse_hm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
}

/// Build a stored UTC string from a local wall-clock date + (hour, minute).
pub fn local_hm_to_utc(date: NaiveDate, hour: u32, minute: u32) -> Option<DateTime<Utc>> {
    let naive = date.and_hms_opt(hour, minute, 0)?;
    Local.from_local_datetime(&naive).single().map(|l| l.with_timezone(&Utc))
}

/// Half-open UTC bounds `[start, end)` of a local calendar day, as stored strings.
pub fn local_day_bounds_utc(date: NaiveDate) -> (String, String) {
    let start = local_hm_to_utc(date, 0, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let next = date.succ_opt().unwrap_or(date);
    let end = local_hm_to_utc(next, 0, 0).unwrap_or(start);
    (to_rfc3339(start), to_rfc3339(end))
}

/// Monday of the week containing `d`.
pub fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// The seven local dates Mon..Sun of the week containing `anchor`.
pub fn week_days(anchor: NaiveDate) -> [NaiveDate; 7] {
    let mon = monday_of(anchor);
    std::array::from_fn(|i| mon + Duration::days(i as i64))
}

/// `HH:MM:SS`, clamped at zero — for the live clock.
pub fn format_hms(secs: i64) -> String {
    let s = secs.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Russian short duration: `1ч 45м` or `45м` — matches the mockup.
pub fn format_dur_ru(secs: i64) -> String {
    let m = secs.max(0) / 60;
    let (h, mm) = (m / 60, m % 60);
    if h > 0 {
        format!("{h}ч {mm:02}м")
    } else {
        format!("{mm}м")
    }
}

pub fn hours_decimal(secs: i64) -> f64 {
    secs as f64 / 3600.0
}

pub fn local_ymd(ts: DateTime<Utc>) -> String {
    let d = ts.with_timezone(&Local);
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

pub fn local_hm(ts: DateTime<Utc>) -> String {
    use chrono::Timelike;
    let d = ts.with_timezone(&Local);
    format!("{:02}:{:02}", d.hour(), d.minute())
}

/// Local minutes-since-midnight of a stored instant (for calendar block placement).
pub fn local_minutes(ts: DateTime<Utc>) -> i64 {
    use chrono::Timelike;
    let d = ts.with_timezone(&Local);
    d.hour() as i64 * 60 + d.minute() as i64
}

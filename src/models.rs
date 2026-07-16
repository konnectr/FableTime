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
    pub note: Option<String>, // free-text project note (links, rate, access…)
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
    pub note: Option<String>, // free-text detail note, separate from the short description
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
    /// The note, trimmed, only if non-empty.
    pub fn note_text(&self) -> Option<&str> {
        self.note.as_deref().map(str::trim).filter(|s| !s.is_empty())
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

// --- note link segmentation -------------------------------------------------

/// A piece of a note: either plain text or a clickable URL.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text(String),
    Link { href: String, text: String },
}

/// Split free text into plain/link segments on `http(s)://…` runs, stripping any
/// trailing `),.;` punctuation off a URL back into the following text. Link
/// display text drops the scheme.
pub fn link_segments(text: &str) -> Vec<Segment> {
    let mut segs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        if rest.starts_with("http://") || rest.starts_with("https://") {
            if i > text_start {
                segs.push(Segment::Text(text[text_start..i].to_string()));
            }
            // Extend to the next whitespace.
            let mut end = i;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            // Peel trailing punctuation back into the following text.
            let mut url_end = end;
            while url_end > i && matches!(bytes[url_end - 1], b')' | b',' | b'.' | b';') {
                url_end -= 1;
            }
            let url = &text[i..url_end];
            let display = url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .unwrap_or(url);
            segs.push(Segment::Link { href: url.to_string(), text: display.to_string() });
            i = url_end;
            text_start = url_end;
        } else {
            // Advance one full char (stay on UTF-8 boundaries).
            i += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
    }
    if text_start < text.len() {
        segs.push(Segment::Text(text[text_start..].to_string()));
    }
    segs
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

/// Russian short weekday, Пн..Вс.
pub fn ru_weekday_short(d: NaiveDate) -> &'static str {
    ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"]
        .get(d.weekday().num_days_from_monday() as usize)
        .copied()
        .unwrap_or("")
}

/// Russian month name in the genitive (as used in dates: "20 июня"), 1-based.
pub fn ru_month_gen(m: u32) -> &'static str {
    [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
        "октября", "ноября", "декабря",
    ]
    .get((m as usize).saturating_sub(1))
    .copied()
    .unwrap_or("")
}

/// A day-group header label like "Пн, 7 июля".
pub fn ru_date_label(d: NaiveDate) -> String {
    format!("{}, {} {}", ru_weekday_short(d), d.day(), ru_month_gen(d.month()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn link(href: &str, text: &str) -> Segment {
        Segment::Link { href: href.into(), text: text.into() }
    }

    #[test]
    fn link_segments_splits_urls() {
        assert_eq!(
            link_segments("see https://github.com/acme/x next"),
            vec![
                Segment::Text("see ".into()),
                link("https://github.com/acme/x", "github.com/acme/x"),
                Segment::Text(" next".into()),
            ]
        );
    }

    #[test]
    fn link_segments_peels_trailing_punctuation() {
        assert_eq!(
            link_segments("repo (https://a.io/p), done."),
            vec![
                Segment::Text("repo (".into()),
                link("https://a.io/p", "a.io/p"),
                Segment::Text("), done.".into()),
            ]
        );
    }

    #[test]
    fn link_segments_plain_text_is_one_segment() {
        assert_eq!(link_segments("just text"), vec![Segment::Text("just text".into())]);
        assert_eq!(link_segments(""), vec![]);
    }
}

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

/// A project's billing currency. Stored as its ISO 4217 code; `Currency::Rub`
/// is the fallback for projects created before this field existed (and for
/// any unrecognized stored code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Rub,
    Byn,
    Usd,
    Eur,
}

impl Currency {
    pub const ALL: [Currency; 4] = [Currency::Rub, Currency::Byn, Currency::Usd, Currency::Eur];

    pub fn code(self) -> &'static str {
        match self {
            Currency::Rub => "RUB",
            Currency::Byn => "BYN",
            Currency::Usd => "USD",
            Currency::Eur => "EUR",
        }
    }

    /// Short label for currency pickers — RUB/BYN share the ₽/Br convention
    /// but need distinct text since both are "рубли" to a Russian speaker.
    pub fn label(self) -> &'static str {
        match self {
            Currency::Rub => "₽ RUB",
            Currency::Byn => "Br BYN",
            Currency::Usd => "$ USD",
            Currency::Eur => "€ EUR",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Currency::Rub => "₽",
            Currency::Byn => "Br",
            Currency::Usd => "$",
            Currency::Eur => "€",
        }
    }

    pub fn from_code(code: &str) -> Currency {
        Currency::ALL.into_iter().find(|c| c.code() == code).unwrap_or(Currency::Rub)
    }
}

impl Default for Currency {
    fn default() -> Self {
        Currency::Rub
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: Id,
    pub name: String,
    pub client: Option<String>,
    pub color: String, // "#rrggbb"
    pub note: Option<String>, // free-text project note (links, access…)
    pub archived: bool,
    pub created_at: String,
    pub hourly_rate: Option<f64>, // per-hour rate in `currency`; None or <=0 = not billable
    pub currency: Currency,
}

impl Project {
    pub fn is_billable(&self) -> bool {
        self.hourly_rate.is_some_and(|r| r > 0.0)
    }
}

/// A recorded payment against a project: `minutes` paid for, at `rate` per
/// hour (in the project's `currency` at the time — see `Db::add_payment`), on
/// `paid_date` (local calendar date, "YYYY-MM-DD").
#[derive(Debug, Clone)]
pub struct Payment {
    pub id: Id,
    pub project_id: Id,
    pub minutes: i64,
    pub rate: f64,
    pub paid_date: String,
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

/// Parse a `ДД.ММ.ГГГГ` string into a calendar date (permissive on leading
/// zeros), validating via `NaiveDate`.
pub fn parse_date_ru(s: &str) -> Option<NaiveDate> {
    let mut parts = s.trim().splitn(3, '.');
    let d: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let y: i32 = parts.next()?.trim().parse().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

/// Format a date as `ДД.ММ.ГГГГ` for the payment-form input's round-trip.
pub fn format_date_ru(d: NaiveDate) -> String {
    format!("{:02}.{:02}.{:04}", d.day(), d.month(), d.year())
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

/// Decimal hours with a Russian comma, e.g. `1,75 ч` — the invoice table's
/// "Часы" column format (distinct from `format_dur_ru`'s "1ч 45м" style).
pub fn format_hours_ru(minutes: i64) -> String {
    format!("{:.2} ч", minutes as f64 / 60.0).replace('.', ",")
}

/// Money in the given currency, thin-space-grouped thousands, rounded to the
/// whole unit — matches the mockup's `fmtMoney`, which always `Math.round`s
/// and never shows minor units (kopecks/cents).
pub fn format_money(amount: f64, currency: Currency) -> String {
    let n = amount.round().max(0.0) as i64;
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        let from_end = bytes.len() - i;
        if i > 0 && from_end % 3 == 0 {
            grouped.push('\u{2009}'); // thin space
        }
        grouped.push(*b as char);
    }
    grouped.push(' ');
    grouped.push_str(currency.symbol());
    grouped
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

    #[test]
    fn format_money_groups_and_rounds() {
        assert_eq!(format_money(12500.0, Currency::Rub), "12\u{2009}500 ₽");
        assert_eq!(format_money(1234.5, Currency::Rub), "1\u{2009}235 ₽"); // rounds, no kopecks
        assert_eq!(format_money(900.0, Currency::Usd), "900 $");
        assert_eq!(format_money(-5.0, Currency::Rub), "0 ₽"); // never negative
    }

    #[test]
    fn currency_code_roundtrips_and_unknown_falls_back_to_rub() {
        assert_eq!(Currency::from_code("USD"), Currency::Usd);
        assert_eq!(Currency::from_code("eur"), Currency::Rub); // case-sensitive; unknown -> Rub
        assert_eq!(Currency::from_code(""), Currency::Rub);
        for c in Currency::ALL {
            assert_eq!(Currency::from_code(c.code()), c);
        }
    }

    #[test]
    fn format_hours_ru_uses_comma() {
        assert_eq!(format_hours_ru(105), "1,75 ч");
        assert_eq!(format_hours_ru(30), "0,50 ч");
    }

    #[test]
    fn date_ru_roundtrips() {
        let d = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        assert_eq!(format_date_ru(d), "24.06.2026");
        assert_eq!(parse_date_ru("24.06.2026"), Some(d));
        assert_eq!(parse_date_ru(" 3.7.2026 "), NaiveDate::from_ymd_opt(2026, 7, 3));
        assert_eq!(parse_date_ru("not a date"), None);
    }
}

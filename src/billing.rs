//! Pure (no-UI) paid/unpaid time allocation + invoice-row building. Kept
//! gpui-free so it is unit-testable, like `exporter.rs`.
//!
//! Works in whole minutes (matching the `payments.minutes` column); callers
//! convert seconds <-> minutes at the boundary. Payments are always applied
//! to a project's entries oldest-first (the oldest tracked time is the oldest
//! debt), via the single `allocate_fifo` primitive shared by both the
//! per-entry paid/partial/unpaid status and invoice-row building below.

use chrono::{Datelike, NaiveDateTime};

use crate::models::{format_dur_ru, format_hours_ru, format_money, ru_month_gen, Currency, Id};

/// Sum of paid minutes across a project's payments, capped at its total
/// tracked minutes.
pub fn paid_minutes_for_project(
    payment_minutes: impl IntoIterator<Item = i64>,
    project_minutes: i64,
) -> i64 {
    payment_minutes.into_iter().sum::<i64>().min(project_minutes).max(0)
}

/// Consume `paid_minutes` off the front of `entry_minutes` (already sorted
/// oldest-first by the caller). Returns, per entry, how many of its minutes
/// are paid.
fn allocate_fifo(entry_minutes: &[i64], paid_minutes: i64) -> Vec<i64> {
    let mut left = paid_minutes.max(0);
    entry_minutes
        .iter()
        .map(|&m| {
            let paid = left.min(m);
            left -= paid;
            paid
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaidState {
    Paid,
    Partial { paid_minutes: i64 },
    Unpaid,
}

/// One project entry as billing needs it. Callers supply entries in
/// chronological ASCENDING order (oldest first) to both functions below.
#[derive(Debug, Clone)]
pub struct BillableEntry {
    pub id: Id,
    pub minutes: i64,
    pub date_label: String,  // e.g. "20 июня"
    pub range_label: String, // "09:00 – 10:30"
    pub desc: String,
}

/// Per-entry paid/partial/unpaid status, oldest debt paid off first.
pub fn paid_states(entries: &[BillableEntry], paid_minutes: i64) -> Vec<(Id, PaidState)> {
    let mins: Vec<i64> = entries.iter().map(|e| e.minutes).collect();
    let paid = allocate_fifo(&mins, paid_minutes);
    entries
        .iter()
        .zip(paid)
        .map(|(e, p)| {
            let state = if e.minutes > 0 && p >= e.minutes {
                PaidState::Paid
            } else if p > 0 {
                PaidState::Partial { paid_minutes: p }
            } else {
                PaidState::Unpaid
            };
            (e.id, state)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct InvoiceRow {
    pub date_label: String,
    pub desc: String,
    pub range_label: String,
    pub hours_label: String,
    pub amount: f64,
    pub amount_label: String,
}

#[derive(Debug, Clone)]
pub struct Invoice {
    pub number: String,
    pub date_label: String,
    pub rows: Vec<InvoiceRow>,
    pub total_minutes: i64,
    pub total_hours_label: String,
    pub total_amount: f64,
    pub total_amount_label: String,
    pub rate_label: String,
    pub paid_before_label: String,
    /// `Some(...)` only when `limit_minutes` billed less than the full
    /// unpaid total — mirrors the mockup's footnote.
    pub rest_after_label: Option<String>,
}

/// Build invoice rows for up to `limit_minutes` of unpaid time, oldest entry
/// first: entries already fully paid are skipped; a partially-paid entry's
/// unpaid remainder is taken first, then whole unpaid entries, until
/// `limit_minutes` runs out. A row cut short by the limit is marked
/// "(частично)" in its description; the tail of an already-partial entry is
/// marked "(остаток)" (a row that is both gets "(частично, остаток)").
pub fn build_invoice(
    entries_oldest_first: &[BillableEntry],
    paid_minutes_before: i64,
    limit_minutes: i64,
    rate: f64,
    currency: Currency,
    project_id: Id,
    now: NaiveDateTime,
) -> Invoice {
    let mins: Vec<i64> = entries_oldest_first.iter().map(|e| e.minutes).collect();
    let already_paid = allocate_fifo(&mins, paid_minutes_before);
    let total_minutes: i64 = mins.iter().sum();
    let unpaid_total = (total_minutes - paid_minutes_before.min(total_minutes)).max(0);
    let mut left = limit_minutes.max(0).min(unpaid_total);

    let mut rows = Vec::new();
    let mut billed_minutes = 0i64;
    for (e, paid_before) in entries_oldest_first.iter().zip(&already_paid) {
        if left <= 0 {
            break;
        }
        let remainder = (e.minutes - paid_before).max(0);
        if remainder <= 0 {
            continue;
        }
        let take = remainder.min(left);
        left -= take;
        billed_minutes += take;

        let mut desc = e.desc.clone();
        match (*paid_before > 0, take < remainder) {
            (true, true) => desc.push_str(" (частично, остаток)"),
            (true, false) => desc.push_str(" (остаток)"),
            (false, true) => desc.push_str(" (частично)"),
            (false, false) => {}
        }
        let amount = take as f64 / 60.0 * rate;
        rows.push(InvoiceRow {
            date_label: e.date_label.clone(),
            desc,
            range_label: e.range_label.clone(),
            hours_label: format_hours_ru(take),
            amount,
            amount_label: format_money(amount, currency),
        });
    }

    let total_amount = billed_minutes as f64 / 60.0 * rate;
    let rest = (unpaid_total - billed_minutes).max(0);

    Invoice {
        number: format!("СЧ-{project_id}-{}", now.format("%Y%m%d-%H%M")),
        date_label: format!("{} {} {}", now.day(), ru_month_gen(now.month()), now.year()),
        rows,
        total_minutes: billed_minutes,
        total_hours_label: format_hours_ru(billed_minutes),
        total_amount,
        total_amount_label: format_money(total_amount, currency),
        rate_label: format!("{}/ч", format_money(rate, currency)),
        paid_before_label: format_dur_ru(paid_minutes_before.min(total_minutes) * 60),
        rest_after_label: (rest > 0).then(|| format_dur_ru(rest * 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: Id, minutes: i64, desc: &str) -> BillableEntry {
        BillableEntry {
            id,
            minutes,
            date_label: "20 июня".into(),
            range_label: "09:00 – 10:30".into(),
            desc: desc.into(),
        }
    }

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 6, 24)
            .unwrap()
            .and_hms_opt(15, 4, 0)
            .unwrap()
    }

    #[test]
    fn paid_minutes_caps_at_project_total() {
        assert_eq!(paid_minutes_for_project([720, 480], 1000), 1000);
        assert_eq!(paid_minutes_for_project([100], 1000), 100);
        assert_eq!(paid_minutes_for_project(std::iter::empty(), 1000), 0);
    }

    #[test]
    fn paid_states_splits_a_partial_entry() {
        // Oldest first: 60m (paid), 60m (partial: 30 of 60 paid), 60m (unpaid).
        let entries = vec![entry(1, 60, "a"), entry(2, 60, "b"), entry(3, 60, "c")];
        let states = paid_states(&entries, 90);
        assert_eq!(states[0], (1, PaidState::Paid));
        assert_eq!(states[1], (2, PaidState::Partial { paid_minutes: 30 }));
        assert_eq!(states[2], (3, PaidState::Unpaid));
    }

    #[test]
    fn build_invoice_skips_paid_takes_partial_remainder_then_stops_at_limit() {
        // Oldest first: 60m paid, 60m partial (30 paid, 30 owed), 120m unpaid.
        let entries = vec![entry(1, 60, "paid"), entry(2, 60, "partial"), entry(3, 120, "fresh")];
        let paid_before = 90; // covers all of #1, 30 of #2

        // Limit only covers the partial's 30m remainder + 40 more of #3.
        let invoice = build_invoice(&entries, paid_before, 70, 100.0, Currency::Rub, 42, now());

        assert_eq!(invoice.rows.len(), 2);
        assert_eq!(invoice.rows[0].desc, "partial (остаток)");
        assert_eq!(invoice.rows[1].desc, "fresh (частично)");
        assert_eq!(invoice.total_minutes, 70);
        assert_eq!(invoice.total_amount, 70.0 / 60.0 * 100.0);
        assert!(invoice.rest_after_label.is_some());
    }

    #[test]
    fn build_invoice_full_unpaid_amount_has_no_rest() {
        let entries = vec![entry(1, 60, "a"), entry(2, 60, "b")];
        let invoice = build_invoice(&entries, 0, 120, 100.0, Currency::Rub, 1, now());
        assert_eq!(invoice.rows.len(), 2);
        assert_eq!(invoice.total_minutes, 120);
        assert!(invoice.rest_after_label.is_none());
    }
}

//! Pure (no-UI) paid/unpaid time allocation + invoice-row building. Kept
//! gpui-free so it is unit-testable, like `exporter.rs`.
//!
//! Works in whole minutes (matching the `payments.minutes` column); callers
//! convert seconds <-> minutes at the boundary. Payments are always applied
//! to a project's entries oldest-first (the oldest tracked time is the oldest
//! debt), via the `allocate_fifo` primitive — used for the per-entry
//! paid/partial/unpaid *display* status only. Invoicing is a separate,
//! independent ledger (`Db::invoiced_minutes_by_entry` / `create_invoice`):
//! an invoice bills an explicit, user-checked set of entries
//! (`build_invoice_from_selection`), and once saved those minutes are
//! excluded from future invoices regardless of payment status — see
//! `ui/projects.rs`'s invoice screen.

use chrono::{Datelike, NaiveDateTime};

use crate::models::{format_hours_ru, format_money, ru_month_gen, Currency, Id};

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
}

/// An entry checked for a new invoice, already resolved to how many of its
/// minutes to bill (normally its full still-uninvoiced remainder — see
/// `Db::invoiced_minutes_by_entry` — computed by the caller before this is
/// built, since availability is a DB concern, not a billing-math one).
#[derive(Debug, Clone)]
pub struct InvoiceSelection {
    pub entry: BillableEntry,
    pub minutes: i64,
}

/// Build an invoice from an explicit, user-checked set of entries. Unlike the
/// old FIFO/hour-limit model, a checkbox is either on or off — no partial-
/// entry slicing — so each row simply bills the minutes it was given.
pub fn build_invoice_from_selection(
    selection: &[InvoiceSelection],
    rate: f64,
    currency: Currency,
    project_id: Id,
    now: NaiveDateTime,
) -> Invoice {
    let mut rows = Vec::new();
    let mut total_minutes = 0i64;
    for sel in selection {
        if sel.minutes <= 0 {
            continue;
        }
        total_minutes += sel.minutes;
        let amount = sel.minutes as f64 / 60.0 * rate;
        rows.push(InvoiceRow {
            date_label: sel.entry.date_label.clone(),
            desc: sel.entry.desc.clone(),
            range_label: sel.entry.range_label.clone(),
            hours_label: format_hours_ru(sel.minutes),
            amount,
            amount_label: format_money(amount, currency),
        });
    }
    let total_amount = total_minutes as f64 / 60.0 * rate;

    Invoice {
        number: format!("СЧ-{project_id}-{}", now.format("%Y%m%d-%H%M")),
        date_label: format!("{} {} {}", now.day(), ru_month_gen(now.month()), now.year()),
        rows,
        total_minutes,
        total_hours_label: format_hours_ru(total_minutes),
        total_amount,
        total_amount_label: format_money(total_amount, currency),
        rate_label: format!("{}/ч", format_money(rate, currency)),
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
    fn build_invoice_from_selection_bills_exactly_the_checked_minutes() {
        // A checked entry with only 30 of its 60 minutes still uninvoiced
        // (the caller resolves that before building the selection) bills
        // just those 30 — no partial-slicing/labeling inside billing itself.
        let selection = vec![
            InvoiceSelection { entry: entry(1, 60, "partial"), minutes: 30 },
            InvoiceSelection { entry: entry(2, 120, "fresh"), minutes: 120 },
        ];
        let invoice = build_invoice_from_selection(&selection, 100.0, Currency::Rub, 42, now());

        assert_eq!(invoice.rows.len(), 2);
        assert_eq!(invoice.rows[0].desc, "partial"); // description untouched
        assert_eq!(invoice.total_minutes, 150);
        assert_eq!(invoice.total_amount, 150.0 / 60.0 * 100.0);
    }

    #[test]
    fn build_invoice_from_selection_skips_zero_minute_entries() {
        let selection = vec![
            InvoiceSelection { entry: entry(1, 60, "a"), minutes: 0 },
            InvoiceSelection { entry: entry(2, 60, "b"), minutes: 60 },
        ];
        let invoice = build_invoice_from_selection(&selection, 100.0, Currency::Rub, 1, now());
        assert_eq!(invoice.rows.len(), 1);
        assert_eq!(invoice.total_minutes, 60);
    }
}

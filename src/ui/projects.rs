//! Projects tab: a grid of project cards (color, client, weekly total, entry
//! count, a Mon–Sun sparkline, an editable per-project note) plus a "new
//! project" row. Billable projects also show a paid/unpaid split, a payment
//! form, and a full-page "Счёт на оплату" (PDF invoice) screen.

use chrono::{Local, Utc};
use gpui::{div, prelude::*, px, relative, rgb, AnyElement, Context, Div, Entity, SharedString, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use std::collections::HashMap;
use std::rc::Rc;

use crate::app::AppState;
use crate::billing::{self, PaidState};
use crate::db::{DayGroup, ProjectStat};
use crate::icons::Lucide;
use crate::invoice_pdf;
use crate::models::{
    format_date_ru, format_dur_ru, format_money, link_segments, local_hm, monday_of,
    parse_date_ru, Currency, Id, Segment,
};
use crate::palette;
use crate::ui::common::{entry_row, paid_status_pill, EntryRow};

pub struct ProjectsView {
    app: Entity<AppState>,
    new_name: Entity<InputState>,
    // Inline note editor: one shared multi-line input, open for one card at a time.
    note_open_id: Option<Id>,
    note_input: Entity<InputState>,
    // When set, the tab shows that project's history detail instead of the grid.
    open_id: Option<Id>,
    // Inline editor for a history entry's note (comment) in the detail view.
    entry_note_open_id: Option<Id>,
    // Inline rename of a history entry's description ("title") — replaces
    // that row with a small text field + save/cancel, like the note editor
    // but swapping the whole row (mirrors Tracker's edit_row pattern).
    entry_rename_id: Option<Id>,
    entry_rename_input: Entity<InputState>,

    // Rate edit (pencil toggle) — only meaningful while a detail view is open.
    rate_edit_open: bool,
    rate_input: Entity<InputState>,
    rate_edit_currency: Currency,

    // Payment-recording form (toggle, but with an explicit submit — structured
    // numeric fields need validation before insert, unlike the note's live save).
    pay_form_open: bool,
    pay_hours_input: Entity<InputState>,
    pay_minutes_input: Entity<InputState>,
    pay_date_input: Entity<InputState>,
    pay_rate_input: Entity<InputState>,

    // Invoice ("Счёт на оплату") full-page screen: Some(id) shows it instead
    // of that project's detail view.
    invoice_project_id: Option<Id>,
    invoice_hours_input: Entity<InputState>,
    invoice_minutes_input: Entity<InputState>,

    status: SharedString,
}

impl ProjectsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("Название нового проекта"));
        let note_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .auto_grow(3, 12)
                .placeholder("Ссылки, репозиторий, доступы, ставка, заметки по проекту…")
        });
        // Editing live-saves to the currently open project's note.
        cx.subscribe(&note_input, |this, inp, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let text = inp.read(cx).value().to_string();
                // Route to a history entry's note when that editor is open,
                // otherwise to the project note.
                if let Some(id) = this.entry_note_open_id {
                    this.app.update(cx, |s, _| {
                        if let Err(e) = s.db.set_entry_note(id, Some(&text)) {
                            eprintln!("set_entry_note: {e:#}");
                        }
                    });
                    cx.notify();
                } else if let Some(id) = this.note_open_id {
                    this.app.update(cx, |s, _| {
                        if let Err(e) = s.db.set_project_note(id, Some(&text)) {
                            eprintln!("set_project_note: {e:#}");
                        }
                    });
                    cx.notify();
                }
            }
        })
        .detach();
        cx.observe(&app, |_, _, cx| cx.notify()).detach();

        let rate_input = cx.new(|cx| InputState::new(window, cx).placeholder("3500"));
        let pay_hours_input = cx.new(|cx| InputState::new(window, cx).placeholder("8"));
        let pay_minutes_input = cx.new(|cx| InputState::new(window, cx).placeholder("30"));
        let pay_date_input = cx.new(|cx| InputState::new(window, cx).placeholder("24.06.2026"));
        let pay_rate_input = cx.new(|cx| InputState::new(window, cx).placeholder("3500"));
        let invoice_hours_input = cx.new(|cx| InputState::new(window, cx).placeholder("0"));
        let invoice_minutes_input = cx.new(|cx| InputState::new(window, cx).placeholder("0"));
        let entry_rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("Название задачи"));

        Self {
            app,
            new_name,
            note_open_id: None,
            note_input,
            open_id: None,
            entry_note_open_id: None,
            entry_rename_id: None,
            entry_rename_input,
            rate_edit_open: false,
            rate_input,
            rate_edit_currency: Currency::Rub,
            pay_form_open: false,
            pay_hours_input,
            pay_minutes_input,
            pay_date_input,
            pay_rate_input,
            invoice_project_id: None,
            invoice_hours_input,
            invoice_minutes_input,
            status: SharedString::default(),
        }
    }

    fn add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        self.app.update(cx, |s, _| {
            if let Err(e) = s.db.create_project(&name, None) {
                eprintln!("create_project: {e:#}");
            }
        });
        self.new_name.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Open the note editor for a project (populating the shared input), or close
    /// it if it's already open for that project.
    fn toggle_note(&mut self, id: Id, note: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.note_open_id == Some(id) {
            self.note_open_id = None;
        } else {
            self.note_open_id = Some(id);
            self.entry_note_open_id = None;
            self.note_input.update(cx, |s, cx| s.set_value(note, window, cx));
        }
        cx.notify();
    }

    /// Open (or close) the note editor for a history entry in the detail view.
    fn toggle_entry_note(&mut self, id: Id, note: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.entry_note_open_id == Some(id) {
            self.entry_note_open_id = None;
        } else {
            self.entry_note_open_id = Some(id);
            self.note_open_id = None;
            self.note_input.update(cx, |s, cx| s.set_value(note, window, cx));
        }
        cx.notify();
    }

    /// Open the inline rename field for a history entry, replacing its row.
    fn begin_entry_rename(&mut self, id: Id, desc: String, window: &mut Window, cx: &mut Context<Self>) {
        self.entry_rename_id = Some(id);
        self.entry_note_open_id = None;
        self.entry_rename_input.update(cx, |s, cx| s.set_value(desc, window, cx));
        cx.notify();
    }

    fn cancel_entry_rename(&mut self, cx: &mut Context<Self>) {
        self.entry_rename_id = None;
        cx.notify();
    }

    fn save_entry_rename(&mut self, id: Id, cx: &mut Context<Self>) {
        let desc = self.entry_rename_input.read(cx).value().to_string();
        self.app.update(cx, |s, _| {
            if let Err(e) = s.db.set_entry_description(id, Some(&desc)) {
                eprintln!("set_entry_description: {e:#}");
            }
        });
        self.entry_rename_id = None;
        cx.notify();
    }

    /// Reset every billing-related toggle/input — called whenever the open
    /// project changes (or closes), so stale state from one project's forms
    /// never leaks into another's.
    fn reset_billing_forms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rate_edit_open = false;
        self.pay_form_open = false;
        self.invoice_project_id = None;
        self.status = SharedString::default();
        self.rate_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.pay_hours_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.pay_minutes_input.update(cx, |s, cx| s.set_value("", window, cx));
    }

    fn billable_entries(&self, project_id: Id, cx: &Context<Self>) -> Vec<billing::BillableEntry> {
        let now = Utc::now();
        self.app
            .read(cx)
            .db
            .entries_for_project_asc(project_id)
            .unwrap_or_default()
            .iter()
            .map(|e| billing::BillableEntry {
                id: e.id,
                minutes: e.duration_secs(now) / 60,
                date_label: format_date_ru(e.local_date()),
                range_label: format!(
                    "{} – {}",
                    local_hm(e.start()),
                    e.end().map(local_hm).unwrap_or_else(|| "…".into())
                ),
                desc: e.desc_or("Без названия"),
            })
            .collect()
    }

    fn toggle_rate_edit(&mut self, current: Option<f64>, currency: Currency, window: &mut Window, cx: &mut Context<Self>) {
        if self.rate_edit_open {
            self.rate_edit_open = false;
        } else {
            self.rate_edit_open = true;
            self.pay_form_open = false;
            self.rate_edit_currency = currency;
            let val = current.map(format_rate_plain).unwrap_or_default();
            self.rate_input.update(cx, |s, cx| s.set_value(val, window, cx));
        }
        cx.notify();
    }

    fn set_rate_edit_currency(&mut self, currency: Currency, cx: &mut Context<Self>) {
        self.rate_edit_currency = currency;
        cx.notify();
    }

    fn submit_rate(&mut self, pid: Id, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.rate_input.read(cx).value().trim().replace(',', ".");
        let rate = raw.parse::<f64>().ok().filter(|r| *r > 0.0);
        let currency = self.rate_edit_currency;
        self.app.update(cx, |s, _| {
            if let Err(e) = s.db.set_project_billing(pid, rate, currency) {
                eprintln!("set_project_billing: {e:#}");
            }
        });
        self.rate_edit_open = false;
        self.rate_input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    fn toggle_pay_form(&mut self, rate: Option<f64>, window: &mut Window, cx: &mut Context<Self>) {
        if self.pay_form_open {
            self.pay_form_open = false;
        } else {
            self.pay_form_open = true;
            self.rate_edit_open = false;
            self.pay_hours_input.update(cx, |s, cx| s.set_value("", window, cx));
            self.pay_minutes_input.update(cx, |s, cx| s.set_value("", window, cx));
            let today = format_date_ru(Local::now().date_naive());
            self.pay_date_input.update(cx, |s, cx| s.set_value(today, window, cx));
            let rate_val = rate.map(format_rate_plain).unwrap_or_default();
            self.pay_rate_input.update(cx, |s, cx| s.set_value(rate_val, window, cx));
        }
        cx.notify();
    }

    fn submit_payment(&mut self, pid: Id, window: &mut Window, cx: &mut Context<Self>) {
        let h: i64 = self.pay_hours_input.read(cx).value().trim().parse().unwrap_or(0);
        let m: i64 = self.pay_minutes_input.read(cx).value().trim().parse().unwrap_or(0);
        let minutes = h.max(0) * 60 + m.max(0);
        let Some(date) = parse_date_ru(&self.pay_date_input.read(cx).value()) else { return };
        let raw_rate = self.pay_rate_input.read(cx).value().trim().replace(',', ".");
        let Ok(rate) = raw_rate.parse::<f64>() else { return };
        if minutes <= 0 || rate <= 0.0 {
            return;
        }
        let iso_date = date.format("%Y-%m-%d").to_string();
        self.app.update(cx, |s, _| {
            if let Err(e) = s.db.add_payment(pid, minutes, rate, &iso_date) {
                eprintln!("add_payment: {e:#}");
            }
        });
        self.pay_form_open = false;
        self.pay_hours_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.pay_minutes_input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Open the invoice screen for a project, defaulting the hour/minute
    /// selectors to the full unpaid amount.
    fn open_invoice(&mut self, pid: Id, unpaid_minutes: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.invoice_project_id = Some(pid);
        self.rate_edit_open = false;
        self.pay_form_open = false;
        self.set_invoice_minutes(unpaid_minutes, window, cx);
    }

    fn close_invoice(&mut self, cx: &mut Context<Self>) {
        self.invoice_project_id = None;
        cx.notify();
    }

    /// The "Всё · Xч Yм" quick-reset link, and the initial prefill in `open_invoice`.
    fn set_invoice_minutes(&mut self, minutes: i64, window: &mut Window, cx: &mut Context<Self>) {
        let minutes = minutes.max(0);
        self.invoice_hours_input
            .update(cx, |s, cx| s.set_value((minutes / 60).to_string(), window, cx));
        self.invoice_minutes_input
            .update(cx, |s, cx| s.set_value((minutes % 60).to_string(), window, cx));
        cx.notify();
    }

    fn invoice_limit_minutes(&self, cx: &Context<Self>) -> i64 {
        let h: i64 = self.invoice_hours_input.read(cx).value().trim().parse().unwrap_or(0);
        let m: i64 = self.invoice_minutes_input.read(cx).value().trim().parse().unwrap_or(0);
        (h.max(0) * 60 + m.max(0)).max(0)
    }

    /// Read the invoice's data on the main thread (the `rusqlite::Connection`
    /// never leaves it), render it to HTML, then hand the pure PDF-bytes
    /// generation and file write to a background task via a native save dialog.
    fn do_generate_invoice(&mut self, project_name: String, client: Option<String>, invoice: billing::Invoice, cx: &mut Context<Self>) {
        let html = invoice_pdf::build_invoice_html(&invoice, &project_name, client.as_deref());
        let file_name = format!("{}.pdf", project_name.replace(['/', '\\'], "-"));
        self.status = "Сохранение…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_title("Сохранить счёт")
                .set_file_name(&file_name)
                .save_file()
                .await
            else {
                let _ = this.update(cx, |this, cx| {
                    this.status = "Сохранение отменено".into();
                    cx.notify();
                });
                return;
            };
            let path = file.path().to_path_buf();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let bytes = invoice_pdf::render_pdf(&html)?;
                    std::fs::write(&path, bytes)?;
                    Ok::<_, anyhow::Error>(path)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.status = match &result {
                    Ok(path) => format!("Счёт сохранён → {}", path.display()),
                    Err(e) => format!("Ошибка сохранения счёта: {e:#}"),
                }
                .into();
                // Open the saved PDF in the OS default viewer right away, so
                // the user sees it land and doesn't have to hunt for it.
                if let Ok(path) = &result {
                    cx.open_with_system(path);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// Plain numeric string for an editable per-hour rate field (no grouping, no
/// currency sign) — distinct from `format_money`, which is display-only.
fn format_rate_plain(r: f64) -> String {
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r}")
    }
}

/// A thin horizontal progress bar: `fill_color` fills `pct` of the track.
fn paid_bar(track_color: u32, fill_color: u32, pct: f64, height: f32) -> Div {
    let pct = (pct as f32).clamp(0.0, 1.0);
    div()
        .w_full()
        .h(px(height))
        .rounded(px(20.))
        .bg(rgb(track_color))
        .overflow_hidden()
        .child(div().h_full().rounded(px(20.)).bg(rgb(fill_color)).w(relative(pct)))
}

/// A small labeled input column for the payment/rate forms.
fn labeled_field(label: &'static str, width: f32, input: &Entity<InputState>) -> Div {
    v_flex()
        .child(div().text_size(px(11.5)).text_color(rgb(palette::TEXT_3)).mb(px(5.)).child(label))
        .child(div().w(px(width)).child(Input::new(input)))
}

impl Render for ProjectsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let today = Local::now().date_naive();
        let stats = self
            .app
            .read(cx)
            .db
            .project_stats(monday_of(today))
            .unwrap_or_default();
        let week_total: i64 = stats.iter().map(|s| s.week_secs).sum();
        let grand_total: i64 = stats.iter().map(|s| s.total_secs).sum();

        // Detail view for an opened project (falls through to the grid if stale).
        if let Some(id) = self.open_id {
            if let Some(stat) = stats.iter().find(|s| s.project.id == id) {
                if self.invoice_project_id == Some(id) {
                    let entries = self.billable_entries(id, cx);
                    let payments = self.app.read(cx).db.list_payments(id).unwrap_or_default();
                    let paid_before = billing::paid_minutes_for_project(
                        payments.iter().map(|p| p.minutes),
                        stat.total_secs / 60,
                    );
                    let limit = self.invoice_limit_minutes(cx);
                    let rate = stat.project.hourly_rate.unwrap_or(0.0);
                    let invoice = billing::build_invoice(
                        &entries,
                        paid_before,
                        limit,
                        rate,
                        stat.project.currency,
                        id,
                        Local::now().naive_local(),
                    );
                    return self.invoice_view(stat, &invoice, cx).into_any_element();
                }
                let history = self.app.read(cx).db.project_history(id).unwrap_or_default();
                return self.detail_view(stat, history, cx).into_any_element();
            }
            self.open_id = None;
            self.invoice_project_id = None;
        }

        let rows: Vec<Div> = stats
            .chunks(2)
            .map(|pair| {
                let mut row = h_flex().gap(px(14.)).items_stretch();
                for s in pair {
                    row = row.child(self.project_card(s, cx));
                }
                if pair.len() == 1 {
                    row = row.child(div().flex_1());
                }
                row
            })
            .collect();

        div()
            .max_w(px(880.))
            .mx_auto()
            .px(px(40.))
            .pt(px(34.))
            .pb(px(60.))
            .child(
                h_flex().items_end().justify_between().mb(px(22.))
                    .child(
                        v_flex()
                            .child(div().text_size(px(25.)).font_semibold().child("Проекты"))
                            .child(
                                div()
                                    .mt(px(5.))
                                    .text_size(px(13.5))
                                    .text_color(rgb(palette::TEXT_2))
                                    .child(format!("За эту неделю — {}", format_dur_ru(week_total))),
                            ),
                    )
                    .child(
                        v_flex().items_end()
                            .child(div().text_size(px(26.)).font_semibold().text_color(rgb(palette::TEXT)).child(format_dur_ru(grand_total)))
                            .child(div().mt(px(2.)).text_size(px(12.)).text_color(rgb(palette::MUTED)).child("всего отслежено")),
                    ),
            )
            .child(
                h_flex().gap(px(10.)).mb(px(22.))
                    .child(div().flex_1().child(Input::new(&self.new_name)))
                    .child(
                        div()
                            .id("add-project")
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .px(px(18.))
                            .h(px(42.))
                            .rounded(px(10.))
                            .cursor_pointer()
                            .bg(rgb(palette::ACCENT))
                            .text_color(rgb(0xffffff))
                            .text_size(px(13.5))
                            .font_semibold()
                            .child(Icon::new(IconName::Plus).xsmall().text_color(rgb(0xffffff)))
                            .child(div().child("Добавить"))
                            .on_click(cx.listener(|this, _, window, cx| this.add_project(window, cx))),
                    ),
            )
            .child(v_flex().gap(px(14.)).children(rows))
            .into_any_element()
    }
}

impl ProjectsView {
    fn project_card(&self, stat: &ProjectStat, cx: &mut Context<Self>) -> Div {
        let color = palette::hex_to_u32(&stat.project.color);
        let mx = stat.per_day_secs.iter().copied().max().unwrap_or(0).max(1);
        let client = stat
            .project
            .client
            .clone()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| "Без клиента".into());

        let bars = stat.per_day_secs.iter().map(|&v| {
            let h = ((v as f64 / mx as f64) * 38.0).round().max(3.0) as f32;
            div()
                .flex_1()
                .h(px(h))
                .rounded(px(3.))
                .bg(rgb(if v > 0 { color } else { palette::BORDER }))
        });

        let day_labels = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"].map(|d| {
            div().flex_1().text_size(px(10.)).text_color(rgb(0xc4c4cb)).child(d)
        });

        let pid = stat.project.id;
        // The upper portion (header + figures + sparkline) opens the project's
        // history detail; the note footer below stays outside the click target.
        let body = div()
            .id(("proj-open", pid as usize))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_id = Some(pid);
                this.note_open_id = None;
                this.entry_note_open_id = None;
                this.entry_rename_id = None;
                this.reset_billing_forms(window, cx);
                cx.notify();
            }))
            .child(
                h_flex().items_center().gap(px(10.))
                    .child(div().w(px(11.)).h(px(11.)).flex_shrink_0().rounded(px(4.)).bg(rgb(color)))
                    .child(
                        v_flex().flex_1().min_w(px(0.))
                            .child(div().text_size(px(15.)).font_semibold().truncate().child(stat.project.name.clone()))
                            .child(div().text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child(client)),
                    )
                    .child(Icon::new(IconName::ChevronRight).small().flex_shrink_0().text_color(rgb(0xc4c4cb))),
            )
            .child(
                h_flex().items_end().gap(px(18.)).mt(px(16.))
                    .child(
                        v_flex()
                            .child(div().text_size(px(22.)).font_semibold().child(format_dur_ru(stat.total_secs)))
                            .child(div().text_size(px(11.5)).text_color(rgb(palette::MUTED)).child("всего")),
                    )
                    .child(
                        v_flex()
                            .child(div().text_size(px(15.)).font_semibold().text_color(rgb(0x3f3f46)).child(format_dur_ru(stat.week_secs)))
                            .child(div().text_size(px(11.5)).text_color(rgb(palette::MUTED)).child("за неделю")),
                    )
                    .child(div().flex_1())
                    .child(
                        v_flex().items_end()
                            .child(div().text_size(px(15.)).font_semibold().text_color(rgb(0x3f3f46)).child(stat.total_entries.to_string()))
                            .child(div().text_size(px(11.5)).text_color(rgb(palette::MUTED)).child("записей")),
                    ),
            )
            .child(h_flex().items_end().gap(px(5.)).h(px(38.)).mt(px(16.)).children(bars))
            .child(h_flex().justify_between().mt(px(7.)).children(day_labels))
            .child(self.card_billing_footer(stat));

        div()
            .flex_1()
            .min_w(px(0.))
            .border_1()
            .border_color(rgb(palette::BORDER))
            .rounded(px(14.))
            .bg(rgb(palette::CARD))
            .p(px(18.))
            .shadow_sm()
            .child(body)
            .child(self.note_footer(stat, cx))
    }

    /// Compact paid/unpaid line under a billable card's sparkline; "Без
    /// биллинга" for a project with no rate set.
    fn card_billing_footer(&self, stat: &ProjectStat) -> Div {
        let footer = div().mt(px(14.)).pt(px(13.)).border_t_1().border_color(rgb(palette::HAIRLINE));
        if !stat.project.is_billable() {
            return footer.text_size(px(12.)).text_color(rgb(palette::MUTED)).child("Без биллинга");
        }
        let color = palette::hex_to_u32(&stat.project.color);
        let unpaid_secs = (stat.total_secs - stat.paid_secs).max(0);
        let rate = stat.project.hourly_rate.unwrap_or(0.0);
        let unpaid_amount = unpaid_secs as f64 / 3600.0 * rate;
        let pct = if stat.total_secs > 0 { stat.paid_secs as f64 / stat.total_secs as f64 } else { 0.0 };
        footer
            .child(paid_bar(palette::BORDER_2, color, pct, 5.))
            .child(
                h_flex().items_baseline().justify_between().mt(px(9.)).text_size(px(12.))
                    .child(
                        h_flex().gap(px(4.))
                            .child(div().text_color(rgb(palette::TEXT_2)).child("Оплачено"))
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(rgb(0x3f3f46))
                                    .child(format_dur_ru(stat.paid_secs)),
                            ),
                    )
                    .child(
                        div()
                            .font_semibold()
                            .text_color(rgb(palette::UNPAID_TEXT))
                            .child(format!(
                                "{} · {}",
                                format_dur_ru(unpaid_secs),
                                format_money(unpaid_amount, stat.project.currency)
                            )),
                    ),
            )
    }

    /// Full-tab history detail for one project: back link, header, a totals
    /// strip, the note, and the entry history grouped by day (most recent first).
    fn detail_view(&self, stat: &ProjectStat, history: Vec<DayGroup>, cx: &mut Context<Self>) -> Div {
        let color = palette::hex_to_u32(&stat.project.color);
        let client = stat
            .project
            .client
            .clone()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or_else(|| "Без клиента".into());

        let back = h_flex()
            .id("proj-back")
            .items_center().gap(px(6.)).mb(px(18.))
            .cursor_pointer()
            .text_size(px(13.)).font_medium().text_color(rgb(palette::TEXT_2))
            .hover(|s| s.text_color(rgb(palette::ACCENT)))
            .child(Icon::new(IconName::ChevronLeft).small())
            .child(div().child("Все проекты"))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_id = None;
                this.entry_note_open_id = None;
                this.entry_rename_id = None;
                this.reset_billing_forms(window, cx);
                cx.notify();
            }));

        let header = h_flex().items_center().gap(px(12.)).mb(px(22.))
            .child(div().w(px(13.)).h(px(13.)).flex_shrink_0().rounded(px(4.)).bg(rgb(color)))
            .child(
                v_flex()
                    .child(div().text_size(px(25.)).font_semibold().child(stat.project.name.clone()))
                    .child(div().mt(px(4.)).text_size(px(13.5)).text_color(rgb(palette::TEXT_2)).child(client)),
            );

        let cell = |big: String, label: &'static str, first: bool| {
            let mut c = v_flex().flex_1().px(px(20.)).py(px(18.));
            if !first {
                c = c.border_l_1().border_color(rgb(palette::HAIRLINE));
            }
            c.child(div().text_size(px(27.)).font_semibold().child(big))
                .child(div().mt(px(3.)).text_size(px(12.)).text_color(rgb(palette::MUTED)).child(label))
        };
        let strip = h_flex().mb(px(22.))
            .border_1().border_color(rgb(palette::BORDER)).rounded(px(14.)).bg(rgb(palette::CARD)).overflow_hidden()
            .child(cell(format_dur_ru(stat.total_secs), "всего по проекту", true))
            .child(cell(format_dur_ru(stat.week_secs), "за эту неделю", false))
            .child(cell(stat.total_entries.to_string(), "записей", false));

        // Per-entry paid/partial/unpaid badges, only computed when billable.
        let paid_states: HashMap<Id, PaidState> = if stat.project.is_billable() {
            let entries = self.billable_entries(stat.project.id, cx);
            let payments = self.app.read(cx).db.list_payments(stat.project.id).unwrap_or_default();
            let paid_before = billing::paid_minutes_for_project(
                payments.iter().map(|p| p.minutes),
                stat.total_secs / 60,
            );
            billing::paid_states(&entries, paid_before).into_iter().collect()
        } else {
            HashMap::new()
        };

        // History: a bordered card of day-groups.
        let mut groups = v_flex()
            .border_1().border_color(rgb(palette::BORDER)).rounded(px(14.)).bg(rgb(palette::CARD)).overflow_hidden();
        for g in &history {
            groups = groups.child(
                h_flex().items_center().justify_between().px(px(18.)).py(px(10.))
                    .bg(rgb(palette::SURFACE)).border_b_1().border_color(rgb(palette::HAIRLINE))
                    .child(div().text_size(px(12.)).font_semibold().text_color(rgb(palette::TEXT_2)).child(g.date_label.clone()))
                    .child(div().text_size(px(12.)).font_semibold().text_color(rgb(palette::MUTED)).child(g.total_label.clone())),
            );
            for it in &g.items {
                let eid = it.id;
                if self.entry_rename_id == Some(eid) {
                    groups = groups.child(self.entry_rename_row(eid, cx));
                    continue;
                }
                let note = it.note.clone().unwrap_or_default();
                let toggle = {
                    let n = note.clone();
                    Rc::new(move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.toggle_entry_note(eid, n.clone(), window, cx);
                    })
                };
                // Clicking the desc text opens the inline rename field. A
                // HistItem's desc is already desc_or's placeholder-filled
                // string, so prefill empty when it's just that placeholder —
                // otherwise saving unchanged would literally store the
                // Russian placeholder text instead of leaving it NULL.
                let on_text_click = {
                    let prefill = if it.desc == "Без названия" { String::new() } else { it.desc.clone() };
                    Rc::new(move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.begin_entry_rename(eid, prefill.clone(), window, cx);
                    }) as Rc<dyn Fn(&mut Self, &mut Window, &mut Context<Self>)>
                };
                let row = EntryRow {
                    id: eid,
                    color: palette::hex_to_u32(&it.color),
                    desc: it.desc.clone(),
                    secondary: it.range.clone(),
                    dur: it.dur_label.clone(),
                    note,
                    note_open: self.entry_note_open_id == Some(eid),
                };
                let trailing: Option<AnyElement> =
                    paid_states.get(&eid).map(|s| paid_status_pill(*s).into_any_element());
                groups = groups.child(entry_row(row, &self.note_input, toggle, Some(on_text_click), trailing, cx));
            }
        }
        if history.is_empty() {
            groups = groups.child(
                div().px(px(18.)).py(px(22.)).text_size(px(13.)).text_color(rgb(palette::MUTED))
                    .child("Пока нет записей по этому проекту."),
            );
        }

        let mut page = div()
            .max_w(px(880.)).mx_auto().px(px(40.)).pt(px(34.)).pb(px(60.))
            .child(back)
            .child(header)
            .child(strip)
            .child(self.payment_block(stat, cx));
        if let Some(note) = stat.project.note.clone().filter(|n| !n.trim().is_empty()) {
            page = page.child(self.detail_note(&note, cx));
        }
        page.child(groups)
    }

    /// The inline rename field rendered in place of a history row being renamed.
    fn entry_rename_row(&self, id: Id, cx: &mut Context<Self>) -> Div {
        h_flex()
            .items_center().gap(px(8.))
            .px(px(18.)).py(px(11.))
            .bg(rgb(0xfafaff))
            .border_b_1().border_color(rgb(palette::HAIRLINE_2))
            .child(div().flex_1().min_w(px(0.)).child(Input::new(&self.entry_rename_input)))
            .child(
                div()
                    .id(("erename-save", id as usize))
                    .flex().items_center().justify_center().w(px(30.)).h(px(30.)).rounded(px(8.))
                    .cursor_pointer()
                    .bg(rgb(palette::ACCENT)).text_color(rgb(0xffffff))
                    .child(Icon::new(IconName::Check).xsmall().text_color(rgb(0xffffff)))
                    .on_click(cx.listener(move |this, _, _, cx| this.save_entry_rename(id, cx))),
            )
            .child(
                div()
                    .id(("erename-cancel", id as usize))
                    .flex().items_center().justify_center().w(px(30.)).h(px(30.)).rounded(px(8.))
                    .cursor_pointer()
                    .text_color(rgb(palette::LABEL))
                    .hover(|s| s.bg(rgb(palette::HOVER_2)))
                    .child(Icon::new(IconName::Close).xsmall())
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_entry_rename(cx))),
            )
    }

    /// Read-only note block for the detail view (icon + text with clickable links).
    fn detail_note(&self, note: &str, cx: &mut Context<Self>) -> Div {
        let mut body = div()
            .flex().flex_wrap().items_center().flex_1().min_w(px(0.))
            .text_size(px(13.)).text_color(rgb(palette::LABEL));
        for (i, seg) in link_segments(note).into_iter().enumerate() {
            body = match seg {
                Segment::Text(t) => body.child(div().child(t)),
                Segment::Link { href, text } => {
                    let icon = if href.contains("github.com") { IconName::Github } else { IconName::ExternalLink };
                    body.child(
                        h_flex()
                            .id(("dnote-link", i))
                            .items_center().gap(px(4.)).cursor_pointer().font_medium().text_color(rgb(palette::ACCENT))
                            .child(Icon::new(icon).xsmall())
                            .child(div().child(text))
                            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&href))),
                    )
                }
            };
        }
        h_flex().items_start().gap(px(9.)).mb(px(22.))
            .p(px(13.))
            .border_1().border_color(rgb(palette::BORDER)).rounded(px(12.)).bg(rgb(palette::SURFACE))
            .child(div().flex_shrink_0().mt(px(2.)).child(Icon::new(Lucide::FileText).small().text_color(rgb(palette::FAINT))))
            .child(body)
    }

    /// The per-project note area beneath the sparkline: view / add / edit.
    fn note_footer(&self, stat: &ProjectStat, cx: &mut Context<Self>) -> Div {
        let pid = stat.project.id;
        let note = stat.project.note.clone().unwrap_or_default();
        let has_note = !note.trim().is_empty();
        let open = self.note_open_id == Some(pid);

        let footer = div()
            .mt(px(16.))
            .pt(px(14.))
            .border_t_1()
            .border_color(rgb(palette::HAIRLINE));

        if open {
            let done = h_flex().justify_end().mt(px(8.)).child(
                div()
                    .id(("pnote-done", pid as usize))
                    .px(px(14.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(rgb(palette::ACCENT))
                    .text_color(rgb(0xffffff))
                    .text_size(px(12.5))
                    .font_semibold()
                    .child("Готово")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.note_open_id = None;
                        cx.notify();
                    })),
            );
            return footer
                .child(div().child(Input::new(&self.note_input)))
                .child(done);
        }

        if has_note {
            // Note body with clickable links (github / external-link prefixed);
            // a pencil button opens the editor.
            let mut body = div()
                .flex()
                .flex_wrap()
                .items_center()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(12.5))
                .text_color(rgb(palette::LABEL));
            for (i, seg) in link_segments(&note).into_iter().enumerate() {
                body = match seg {
                    Segment::Text(t) => body.child(div().child(t)),
                    Segment::Link { href, text } => {
                        let link_icon = if href.contains("github.com") {
                            IconName::Github
                        } else {
                            IconName::ExternalLink
                        };
                        body.child(
                            h_flex()
                                .id(("pnote-link", ((pid as usize) << 8) | (i & 0xff)))
                                .items_center()
                                .gap(px(4.))
                                .cursor_pointer()
                                .font_medium()
                                .text_color(rgb(palette::ACCENT))
                                .child(Icon::new(link_icon).xsmall())
                                .child(div().child(text))
                                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&href))),
                        )
                    }
                };
            }

            let note_for_edit = note.clone();
            return footer.child(
                h_flex()
                    .items_start()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .mt(px(2.))
                            .child(Icon::new(Lucide::FileText).small().text_color(rgb(palette::FAINT))),
                    )
                    .child(body)
                    .child(
                        div()
                            .id(("pnote-edit", pid as usize))
                            .flex_shrink_0()
                            .w(px(26.))
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .cursor_pointer()
                            .text_color(rgb(palette::MUTED))
                            .hover(|s| s.bg(rgb(palette::HOVER_2)))
                            .child(Icon::new(Lucide::Pencil).xsmall())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_note(pid, note_for_edit.clone(), window, cx);
                            })),
                    ),
            );
        }

        // No note yet — an "add" affordance.
        footer.child(
            h_flex()
                .id(("pnote-add", pid as usize))
                .items_center()
                .gap(px(7.))
                .cursor_pointer()
                .text_size(px(12.5))
                .font_medium()
                .text_color(rgb(palette::MUTED))
                .hover(|s| s.text_color(rgb(palette::ACCENT)))
                .child(Icon::new(IconName::Plus).xsmall())
                .child(div().child("Добавить заметку по проекту"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toggle_note(pid, String::new(), window, cx);
                })),
        )
    }

    /// The "Оплата" block in a project's detail view: rate (+ edit), a
    /// paid/unpaid progress bar and figures, the "Записать оплату" /
    /// "Счёт на оплату" actions, and the recorded-payments list.
    fn payment_block(&self, stat: &ProjectStat, cx: &mut Context<Self>) -> Div {
        let pid = stat.project.id;
        let rate = stat.project.hourly_rate;
        let currency = stat.project.currency;
        let billable = stat.project.is_billable();
        let unpaid_secs = (stat.total_secs - stat.paid_secs).max(0);

        let rate_pencil = div()
            .id(("rate-edit", pid as usize))
            .flex_shrink_0()
            .w(px(22.)).h(px(22.))
            .flex().items_center().justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(rgb(palette::MUTED))
            .hover(|s| s.bg(rgb(palette::HOVER_2)))
            .child(Icon::new(Lucide::Pencil).xsmall())
            .on_click(cx.listener(move |this, _, window, cx| this.toggle_rate_edit(rate, currency, window, cx)));

        let mut header = h_flex().items_center().gap(px(10.)).mb(px(16.))
            .child(div().text_size(px(14.)).font_semibold().child("Оплата"));
        header = header.child(
            div().text_size(px(12.)).text_color(rgb(palette::MUTED)).child(if billable {
                format!("ставка {}/ч", format_money(rate.unwrap_or(0.0), currency))
            } else {
                "Без биллинга".to_string()
            }),
        );
        header = header.child(rate_pencil).child(div().flex_1());

        if billable {
            let mut actions = h_flex().gap(px(9.));
            if unpaid_secs > 0 {
                let unpaid_minutes = unpaid_secs / 60;
                actions = actions.child(
                    div()
                        .id(("open-invoice", pid as usize))
                        .flex().items_center().gap(px(7.)).px(px(14.)).h(px(34.)).rounded(px(9.))
                        .cursor_pointer()
                        .border_1().border_color(rgb(palette::BORDER)).bg(rgb(palette::CARD))
                        .text_size(px(13.)).font_medium().text_color(rgb(0x3f3f46))
                        .hover(|s| s.border_color(rgb(0xc4c4cb)))
                        .child(Icon::new(Lucide::FileText).xsmall())
                        .child(div().child("Счёт на оплату"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_invoice(pid, unpaid_minutes, window, cx);
                        })),
                );
            }
            actions = actions.child(
                div()
                    .id(("toggle-pay", pid as usize))
                    .flex().items_center().gap(px(7.)).px(px(14.)).h(px(34.)).rounded(px(9.))
                    .cursor_pointer()
                    .bg(rgb(palette::ACCENT))
                    .text_color(rgb(0xffffff))
                    .text_size(px(13.)).font_semibold()
                    .child(Icon::new(IconName::Plus).xsmall().text_color(rgb(0xffffff)))
                    .child(div().child("Записать оплату"))
                    .on_click(cx.listener(move |this, _, window, cx| this.toggle_pay_form(rate, window, cx))),
            );
            header = header.child(actions);
        }

        let mut block = v_flex()
            .mb(px(22.)).p(px(20.))
            .border_1().border_color(rgb(palette::BORDER)).rounded(px(14.)).bg(rgb(palette::CARD))
            .child(header);

        if billable {
            let color = palette::hex_to_u32(&stat.project.color);
            let pct = if stat.total_secs > 0 { stat.paid_secs as f64 / stat.total_secs as f64 } else { 0.0 };
            let unpaid_amount = unpaid_secs as f64 / 3600.0 * rate.unwrap_or(0.0);
            let paid_amount = stat.paid_secs as f64 / 3600.0 * rate.unwrap_or(0.0);
            block = block
                .child(paid_bar(palette::BORDER_2, color, pct, 7.))
                .child(
                    h_flex().gap(px(32.)).mt(px(14.))
                        .child(
                            v_flex()
                                .child(
                                    div()
                                        .text_size(px(18.))
                                        .font_semibold()
                                        .text_color(rgb(palette::PAID_TEXT))
                                        .child(format_dur_ru(stat.paid_secs)),
                                )
                                .child(
                                    div()
                                        .mt(px(2.))
                                        .text_size(px(12.))
                                        .text_color(rgb(palette::MUTED))
                                        .child(format!("оплачено · {}", format_money(paid_amount, currency))),
                                ),
                        )
                        .child(
                            v_flex()
                                .child(
                                    div()
                                        .text_size(px(18.))
                                        .font_semibold()
                                        .text_color(rgb(palette::UNPAID_TEXT))
                                        .child(format_dur_ru(unpaid_secs)),
                                )
                                .child(
                                    div()
                                        .mt(px(2.))
                                        .text_size(px(12.))
                                        .text_color(rgb(palette::MUTED))
                                        .child(format!("к оплате · {}", format_money(unpaid_amount, currency))),
                                ),
                        ),
                );
        }

        if self.rate_edit_open {
            block = block.child(self.rate_edit_form(pid, cx));
        }
        if billable && self.pay_form_open {
            block = block.child(self.pay_form(pid, currency, cx));
        }
        if billable {
            block = block.child(self.payments_list(pid, currency, cx));
        }
        block
    }

    fn rate_edit_form(&self, pid: Id, cx: &mut Context<Self>) -> Div {
        let mut currency_row = h_flex().gap(px(6.));
        for c in Currency::ALL {
            let selected = self.rate_edit_currency == c;
            currency_row = currency_row.child(
                div()
                    .id(SharedString::from(format!("rate-cur-{}", c.code())))
                    .px(px(10.)).h(px(38.)).flex().items_center()
                    .rounded(px(9.))
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(if selected { palette::ACCENT } else { palette::BORDER }))
                    .when(selected, |d| d.bg(rgb(palette::ACCENT_SOFT)))
                    .text_size(px(13.))
                    .font_medium()
                    .text_color(rgb(if selected { palette::ACCENT_DK } else { palette::LABEL }))
                    .child(c.label())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_rate_edit_currency(c, cx))),
            );
        }

        h_flex().items_end().gap(px(10.)).flex_wrap().mt(px(16.)).pt(px(16.)).border_t_1().border_color(rgb(palette::HAIRLINE))
            .child(labeled_field("Ставка за час", 110., &self.rate_input))
            .child(
                v_flex()
                    .child(div().text_size(px(11.5)).text_color(rgb(palette::TEXT_3)).mb(px(5.)).child("Валюта"))
                    .child(currency_row),
            )
            .child(
                div()
                    .id(("rate-save", pid as usize))
                    .px(px(16.)).h(px(38.)).flex().items_center().rounded(px(9.))
                    .cursor_pointer()
                    .bg(rgb(0x18181b)).text_color(rgb(0xffffff)).text_size(px(13.5)).font_semibold()
                    .child("Готово")
                    .on_click(cx.listener(move |this, _, window, cx| this.submit_rate(pid, window, cx))),
            )
    }

    fn pay_form(&self, pid: Id, currency: Currency, cx: &mut Context<Self>) -> Div {
        h_flex().items_end().gap(px(10.)).flex_wrap().mt(px(16.)).pt(px(16.)).border_t_1().border_color(rgb(palette::HAIRLINE))
            .child(labeled_field("Часы", 74., &self.pay_hours_input))
            .child(labeled_field("Минуты", 82., &self.pay_minutes_input))
            .child(labeled_field("Дата оплаты", 130., &self.pay_date_input))
            .child(labeled_field(
                if currency == Currency::Rub { "Ставка, ₽/ч" } else { "Ставка/ч" },
                106.,
                &self.pay_rate_input,
            ))
            .child(
                div()
                    .id(("pay-submit", pid as usize))
                    .px(px(20.)).h(px(38.)).flex().items_center().rounded(px(9.))
                    .cursor_pointer()
                    .bg(rgb(0x18181b)).text_color(rgb(0xffffff)).text_size(px(13.5)).font_semibold()
                    .child("Подтвердить оплату")
                    .on_click(cx.listener(move |this, _, window, cx| this.submit_payment(pid, window, cx))),
            )
    }

    fn payments_list(&self, pid: Id, currency: Currency, cx: &mut Context<Self>) -> Div {
        let payments = self.app.read(cx).db.list_payments(pid).unwrap_or_default();
        if payments.is_empty() {
            return div()
                .mt(px(16.)).pt(px(14.)).border_t_1().border_color(rgb(palette::HAIRLINE))
                .text_size(px(12.5)).text_color(rgb(palette::MUTED))
                .child("Оплат пока не было — всё время в счёте к оплате.");
        }
        let mut list = v_flex().mt(px(16.)).pt(px(6.)).border_t_1().border_color(rgb(palette::HAIRLINE));
        for p in payments.iter().rev() {
            let amount = p.minutes as f64 / 60.0 * p.rate;
            let pay_id = p.id;
            list = list.child(
                h_flex().items_center().gap(px(12.)).py(px(11.)).border_b_1().border_color(rgb(palette::HAIRLINE_2))
                    .child(Icon::new(IconName::Check).small().text_color(rgb(palette::PAID_TEXT)))
                    .child(
                        v_flex().flex_1().min_w(px(0.))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_medium()
                                    .child(format!("Оплачено {}", format_dur_ru(p.minutes * 60))),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .text_size(px(11.5))
                                    .text_color(rgb(palette::MUTED))
                                    .child(format!("{} · {}/ч", display_date(&p.paid_date), format_money(p.rate, currency))),
                            ),
                    )
                    .child(div().text_size(px(13.5)).font_semibold().child(format_money(amount, currency)))
                    .child(
                        div()
                            .id(("pay-del", pay_id as usize))
                            .cursor_pointer().p(px(4.))
                            .text_color(rgb(palette::NOTE_IDLE))
                            .hover(|s| s.text_color(rgb(palette::DANGER)))
                            .child(Icon::new(IconName::Delete).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.app.update(cx, |s, _| {
                                    let _ = s.db.delete_payment(pay_id);
                                });
                                cx.notify();
                            })),
                    ),
            );
        }
        list
    }

    /// Full-page "Счёт на оплату": a toolbar (back link, hours/minutes to
    /// bill, a "Всё" quick-reset, "Скачать PDF") above a document-styled card
    /// built from the already-computed `billing::Invoice`.
    fn invoice_view(&self, stat: &ProjectStat, invoice: &billing::Invoice, cx: &mut Context<Self>) -> Div {
        let unpaid_secs = (stat.total_secs - stat.paid_secs).max(0);
        let unpaid_minutes = unpaid_secs / 60;
        let project_name = stat.project.name.clone();
        let client = stat.project.client.clone().filter(|c| !c.trim().is_empty());
        let dl_project_name = project_name.clone();
        let dl_client = client.clone();
        let dl_invoice = invoice.clone();

        let back = h_flex()
            .id("inv-back")
            .items_center().gap(px(6.)).cursor_pointer()
            .text_size(px(13.)).font_medium().text_color(rgb(palette::TEXT_2))
            .hover(|s| s.text_color(rgb(palette::ACCENT)))
            .child(Icon::new(IconName::ChevronLeft).small())
            .child(div().child("Назад к проекту"))
            .on_click(cx.listener(|this, _, _, cx| this.close_invoice(cx)));

        let toolbar = h_flex().items_center().gap(px(14.)).flex_wrap().mb(px(22.))
            .child(back)
            .child(
                h_flex().items_center().gap(px(7.)).ml(px(18.)).pl(px(18.)).border_l_1().border_color(rgb(palette::BORDER))
                    .child(div().text_size(px(12.5)).text_color(rgb(palette::TEXT_2)).child("Выставить за"))
                    .child(div().w(px(54.)).child(Input::new(&self.invoice_hours_input)))
                    .child(div().text_size(px(12.5)).text_color(rgb(palette::MUTED)).child("ч"))
                    .child(div().w(px(54.)).child(Input::new(&self.invoice_minutes_input)))
                    .child(div().text_size(px(12.5)).text_color(rgb(palette::MUTED)).child("м"))
                    .child(
                        div()
                            .id("inv-all")
                            .cursor_pointer().ml(px(6.))
                            .text_size(px(12.5)).font_medium().text_color(rgb(palette::ACCENT))
                            .child(format!("Всё · {}", format_dur_ru(unpaid_secs)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_invoice_minutes(unpaid_minutes, window, cx);
                            })),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("inv-download")
                    .flex().items_center().gap(px(8.)).px(px(18.)).h(px(36.)).rounded(px(10.))
                    .cursor_pointer()
                    .bg(rgb(palette::ACCENT)).text_color(rgb(0xffffff)).text_size(px(13.5)).font_semibold()
                    .child(Icon::new(Lucide::Download).xsmall().text_color(rgb(0xffffff)))
                    .child(div().child("Скачать PDF"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.do_generate_invoice(dl_project_name.clone(), dl_client.clone(), dl_invoice.clone(), cx);
                    })),
            );

        let mut rows_col = v_flex();
        for r in &invoice.rows {
            rows_col = rows_col.child(
                h_flex().items_center().gap(px(10.)).py(px(10.)).border_b_1().border_color(rgb(palette::HAIRLINE))
                    .child(div().w(px(88.)).flex_shrink_0().text_size(px(12.5)).text_color(rgb(palette::LABEL)).child(r.date_label.clone()))
                    .child(div().flex_1().min_w(px(0.)).text_size(px(13.)).font_medium().child(r.desc.clone()))
                    .child(div().w(px(120.)).flex_shrink_0().text_size(px(12.5)).text_color(rgb(palette::LABEL)).child(r.range_label.clone()))
                    .child(div().w(px(64.)).flex_shrink_0().text_size(px(12.5)).text_color(rgb(palette::LABEL)).child(r.hours_label.clone()))
                    .child(div().w(px(90.)).flex_shrink_0().text_size(px(13.)).font_semibold().child(r.amount_label.clone())),
            );
        }
        let body: AnyElement = if invoice.rows.is_empty() {
            div()
                .py(px(26.)).text_size(px(13.)).text_color(rgb(palette::MUTED))
                .child("Нет задач для выставления: укажите количество часов выше или запишите новое отработанное время.")
                .into_any_element()
        } else {
            rows_col.into_any_element()
        };

        let mut footnote = format!(
            "В счёт включены задачи, по которым оплата ещё не поступала. Ранее оплачено по проекту: {}.",
            invoice.paid_before_label
        );
        if let Some(rest) = &invoice.rest_after_label {
            footnote.push_str(&format!(" Остаток к оплате после этого счёта: {rest}."));
        }

        let client_label = client.clone().unwrap_or_else(|| "Без клиента".into());
        let doc = v_flex()
            .p(px(36.))
            .border_1().border_color(rgb(palette::BORDER)).rounded(px(14.)).bg(rgb(palette::CARD)).shadow_sm()
            .child(
                h_flex().items_start().justify_between().pb(px(18.)).border_b_2().border_color(rgb(0x18181b))
                    .child(
                        v_flex()
                            .child(div().text_size(px(22.)).font_semibold().child("Счёт на оплату"))
                            .child(
                                div()
                                    .mt(px(5.))
                                    .text_size(px(12.5))
                                    .text_color(rgb(palette::TEXT_2))
                                    .child(format!("{} от {}", invoice.number, invoice.date_label)),
                            ),
                    )
                    .child(
                        v_flex().items_end()
                            .child(div().text_size(px(10.5)).text_color(rgb(palette::MUTED)).child("К ОПЛАТЕ"))
                            .child(div().mt(px(4.)).text_size(px(22.)).font_semibold().child(invoice.total_amount_label.clone())),
                    ),
            )
            .child(
                h_flex().gap(px(32.)).mt(px(18.))
                    .child(info_cell("ПРОЕКТ", stat.project.name.clone()))
                    .child(info_cell("ЗАКАЗЧИК", client_label))
                    .child(info_cell("СТАВКА", invoice.rate_label.clone())),
            )
            .child(
                h_flex().gap(px(10.)).mt(px(22.)).pb(px(8.)).border_b_1().border_color(rgb(palette::BORDER))
                    .child(div().w(px(88.)).flex_shrink_0().text_size(px(10.5)).text_color(rgb(palette::TEXT_2)).child("ДАТА"))
                    .child(div().flex_1().text_size(px(10.5)).text_color(rgb(palette::TEXT_2)).child("ЗАДАЧА"))
                    .child(div().w(px(120.)).flex_shrink_0().text_size(px(10.5)).text_color(rgb(palette::TEXT_2)).child("ВРЕМЯ ВЫПОЛНЕНИЯ"))
                    .child(div().w(px(64.)).flex_shrink_0().text_size(px(10.5)).text_color(rgb(palette::TEXT_2)).child("ЧАСЫ"))
                    .child(div().w(px(90.)).flex_shrink_0().text_size(px(10.5)).text_color(rgb(palette::TEXT_2)).child("СУММА")),
            )
            .child(body)
            .child(
                v_flex().items_end().mt(px(18.))
                    .child(
                        h_flex().gap(px(24.)).text_size(px(12.5)).text_color(rgb(palette::TEXT_2))
                            .child("Всего часов").child(invoice.total_hours_label.clone()),
                    )
                    .child(
                        h_flex().gap(px(24.)).mt(px(4.)).text_size(px(12.5)).text_color(rgb(palette::TEXT_2))
                            .child("Ставка").child(invoice.rate_label.clone()),
                    )
                    .child(
                        h_flex().gap(px(24.)).mt(px(10.)).pt(px(10.)).border_t_1().border_color(rgb(palette::BORDER))
                            .text_size(px(15.)).font_semibold()
                            .child("Итого к оплате").child(invoice.total_amount_label.clone()),
                    ),
            )
            .child(
                div()
                    .mt(px(20.)).pt(px(14.)).border_t_1().border_color(rgb(palette::HAIRLINE))
                    .text_size(px(11.)).text_color(rgb(palette::MUTED))
                    .child(footnote),
            );

        div().max_w(px(880.)).mx_auto().px(px(40.)).pt(px(34.)).pb(px(60.))
            .child(toolbar)
            .child(doc)
            .when(!self.status.is_empty(), |d| {
                d.child(
                    div()
                        .mt(px(14.))
                        .text_size(px(12.5))
                        .text_color(rgb(palette::TEXT_2))
                        .child(self.status.clone()),
                )
            })
    }
}

/// An invoice info-row cell ("ПРОЕКТ" / "ЗАКАЗЧИК" / "СТАВКА").
fn info_cell(label: &'static str, value: String) -> Div {
    v_flex().flex_1().min_w(px(0.))
        .child(div().text_size(px(10.5)).text_color(rgb(palette::MUTED)).child(label))
        .child(div().mt(px(4.)).text_size(px(13.5)).font_semibold().truncate().child(value))
}

/// Stored ISO `paid_date` ("YYYY-MM-DD") rendered as `ДД.ММ.ГГГГ`.
fn display_date(iso: &str) -> String {
    chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .map(format_date_ru)
        .unwrap_or_else(|_| iso.to_string())
}

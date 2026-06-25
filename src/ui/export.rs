//! Export tab: pick a period + project filter + formats, see a live summary,
//! then write the chosen formats into a folder.

use chrono::{Datelike, Duration, Local, NaiveDate};
use gpui::{div, prelude::*, px, rgb, Context, Entity, SharedString, Window};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::app::AppState;
use crate::exporter::write_exports;
use crate::models::{format_dur_ru, Id};
use crate::palette;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Period {
    ThisWeek,
    LastWeek,
    ThisMonth,
}

pub struct ExportView {
    app: Entity<AppState>,
    period: Period,
    proj_filter: Option<Id>,
    fmt_csv: bool,
    fmt_json: bool,
    fmt_md: bool,
    status: SharedString,
}

fn period_range(p: Period, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let monday = crate::models::monday_of(today);
    match p {
        Period::ThisWeek => (monday, monday + Duration::days(6)),
        Period::LastWeek => (monday - Duration::days(7), monday - Duration::days(1)),
        Period::ThisMonth => {
            let first = today.with_day(1).unwrap_or(today);
            let next = if today.month() == 12 {
                NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
            }
            .unwrap_or(today);
            (first, next.pred_opt().unwrap_or(today))
        }
    }
}

fn zap(n: i64) -> &'static str {
    let (a, b) = (n % 10, n % 100);
    if a == 1 && b != 11 {
        "запись"
    } else if (2..=4).contains(&a) && !(12..=14).contains(&b) {
        "записи"
    } else {
        "записей"
    }
}

impl ExportView {
    pub fn new(app: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            period: Period::ThisWeek,
            proj_filter: None,
            fmt_csv: true,
            fmt_json: true,
            fmt_md: true,
            status: SharedString::default(),
        }
    }

    fn rows(&self, cx: &Context<Self>) -> Vec<crate::models::ExportRow> {
        let (from, to) = period_range(self.period, Local::now().date_naive());
        let app = self.app.read(cx);
        let mut rows = app.db.entries_in_range(from, to).unwrap_or_default();
        if let Some(pid) = self.proj_filter {
            if let Ok((name, _)) = app.db.project_meta(pid) {
                rows.retain(|r| r.project == name);
            }
        }
        rows
    }

    fn do_export(&mut self, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        if rows.is_empty() {
            self.status = "Нет записей за выбранный период".into();
            cx.notify();
            return;
        }
        if !(self.fmt_csv || self.fmt_json || self.fmt_md) {
            self.status = "Выберите хотя бы один формат".into();
            cx.notify();
            return;
        }
        let (csv, json, md) = (self.fmt_csv, self.fmt_json, self.fmt_md);
        self.status = "Выберите папку…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let Some(folder) = rfd::AsyncFileDialog::new()
                .set_title("Папка для экспорта")
                .pick_folder()
                .await
            else {
                let _ = this.update(cx, |this, cx| {
                    this.status = "Экспорт отменён".into();
                    cx.notify();
                });
                return;
            };
            let dir = folder.path().to_path_buf();
            let dir2 = dir.clone();
            let written = cx
                .background_executor()
                .spawn(async move { write_exports(&dir2, &rows, csv, json, md) })
                .await;
            let msg = match written {
                Ok(n) => format!("Экспортировано файлов: {n} → {}", dir.display()),
                Err(e) => format!("Ошибка экспорта: {e:#}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.status = msg.into();
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ExportView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let projects = self.app.read(cx).db.list_projects().unwrap_or_default();
        let rows = self.rows(cx);
        let total: i64 = rows.iter().map(|r| r.duration_secs).sum();
        let count = rows.len() as i64;
        let summary = format!("{count} {} · {}", zap(count), format_dur_ru(total));

        let section = |label: &str| {
            div()
                .text_size(px(13.))
                .font_semibold()
                .text_color(rgb(palette::LABEL))
                .mb(px(11.))
                .child(label.to_string())
        };

        // period chips
        let periods = [
            (Period::ThisWeek, "Эта неделя"),
            (Period::LastWeek, "Прошлая неделя"),
            (Period::ThisMonth, "Этот месяц"),
        ];
        let period_chips = h_flex().gap(px(8.)).children(periods.map(|(p, label)| {
            let on = self.period == p;
            div()
                .id(label)
                .px(px(16.))
                .py(px(9.))
                .rounded(px(9.))
                .text_size(px(13.))
                .font_medium()
                .cursor_pointer()
                .border_1()
                .border_color(rgb(if on { palette::ACCENT } else { palette::BORDER }))
                .text_color(rgb(if on { palette::ACCENT_DK } else { palette::LABEL }))
                .when(on, |d| d.bg(rgb(palette::ACCENT_SOFT)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.period = p;
                    this.status = SharedString::default();
                    cx.notify();
                }))
        }));

        // project filter chips
        let mut proj_chips = h_flex().gap(px(8.)).flex_wrap();
        {
            let on = self.proj_filter.is_none();
            proj_chips = proj_chips.child(
                div()
                    .id("proj-all")
                    .px(px(14.))
                    .py(px(9.))
                    .rounded(px(9.))
                    .text_size(px(13.))
                    .font_medium()
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(if on { palette::ACCENT } else { palette::BORDER }))
                    .text_color(rgb(if on { palette::ACCENT_DK } else { palette::LABEL }))
                    .when(on, |d| d.bg(rgb(palette::ACCENT_SOFT)))
                    .child("Все проекты")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.proj_filter = None;
                        cx.notify();
                    })),
            );
        }
        for p in &projects {
            let pid = p.id;
            let on = self.proj_filter == Some(pid);
            let color = palette::hex_to_u32(&p.color);
            proj_chips = proj_chips.child(
                h_flex()
                    .id(("pf", pid as usize))
                    .items_center()
                    .gap(px(7.))
                    .px(px(14.))
                    .py(px(9.))
                    .rounded(px(9.))
                    .text_size(px(13.))
                    .font_medium()
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(if on { palette::ACCENT } else { palette::BORDER }))
                    .text_color(rgb(if on { palette::ACCENT_DK } else { palette::LABEL }))
                    .when(on, |d| d.bg(rgb(palette::ACCENT_SOFT)))
                    .child(crate::ui::common::dot(color, 8.))
                    .child(p.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.proj_filter = Some(pid);
                        cx.notify();
                    })),
            );
        }

        // format cards (multi-select)
        let formats = [
            ("fmt-csv", "CSV", "Таблица для Excel / Sheets", self.fmt_csv, 0u8),
            ("fmt-json", "JSON", "Структурированные данные", self.fmt_json, 1u8),
            ("fmt-md", "Markdown", "Готовый текстовый отчёт", self.fmt_md, 2u8),
        ];
        let format_cards = h_flex().gap(px(10.)).children(formats.map(|(id, label, desc, on, which)| {
            div()
                .id(id)
                .flex_1()
                .border(px(1.5))
                .border_color(rgb(if on { palette::ACCENT } else { palette::BORDER }))
                .rounded(px(12.))
                .p(px(16.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(0xfafaff)))
                .child(
                    h_flex().items_center().justify_between()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_semibold()
                                .text_color(rgb(if on { palette::ACCENT_DK } else { 0x27272a }))
                                .child(label),
                        )
                        .child(
                            div()
                                .w(px(16.)).h(px(16.))
                                .rounded_full()
                                .border(px(1.5))
                                .border_color(rgb(if on { palette::ACCENT } else { 0xd4d4d8 }))
                                .when(on, |d| d.bg(rgb(palette::ACCENT)))
                                .flex().items_center().justify_center()
                                .when(on, |d| d.child(div().w(px(6.)).h(px(6.)).rounded_full().bg(rgb(0xffffff)))),
                        ),
                )
                .child(div().mt(px(5.)).text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child(desc))
                .on_click(cx.listener(move |this, _, _, cx| {
                    match which {
                        0 => this.fmt_csv = !this.fmt_csv,
                        1 => this.fmt_json = !this.fmt_json,
                        _ => this.fmt_md = !this.fmt_md,
                    }
                    this.status = SharedString::default();
                    cx.notify();
                }))
        }));

        let footer = h_flex()
            .items_center()
            .gap(px(16.))
            .border_1()
            .border_color(rgb(palette::BORDER))
            .rounded(px(14.))
            .bg(rgb(palette::SURFACE))
            .px(px(20.))
            .py(px(18.))
            .child(
                v_flex().flex_1()
                    .child(div().text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child("Будет экспортировано"))
                    .child(div().mt(px(3.)).text_size(px(17.)).font_semibold().child(summary)),
            )
            .child(
                div()
                    .id("do-export")
                    .flex().items_center().gap(px(8.))
                    .h(px(44.)).px(px(22.))
                    .rounded(px(11.))
                    .cursor_pointer()
                    .bg(rgb(palette::ACCENT))
                    .text_color(rgb(0xffffff))
                    .text_size(px(14.))
                    .font_semibold()
                    .child(Icon::new(IconName::ArrowDown).xsmall().text_color(rgb(0xffffff)))
                    .child(div().child("Экспортировать"))
                    .on_click(cx.listener(|this, _, _, cx| this.do_export(cx))),
            );

        div()
            .max_w(px(720.))
            .mx_auto()
            .px(px(40.))
            .pt(px(34.))
            .pb(px(60.))
            .child(
                v_flex().mb(px(26.))
                    .child(div().text_size(px(25.)).font_semibold().child("Экспорт"))
                    .child(div().mt(px(5.)).text_size(px(13.5)).text_color(rgb(palette::TEXT_2)).child("Выгрузите записи времени в файл")),
            )
            .child(section("Период"))
            .child(div().mb(px(26.)).child(period_chips))
            .child(section("Проекты"))
            .child(div().mb(px(26.)).child(proj_chips))
            .child(section("Формат"))
            .child(div().mb(px(28.)).child(format_cards))
            .child(footer)
            .when(!self.status.is_empty(), |d| {
                d.child(div().mt(px(12.)).text_size(px(13.)).text_color(rgb(palette::TEXT_2)).child(self.status.clone()))
            })
    }
}

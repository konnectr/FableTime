//! Calendar tab: a Mon–Sun week timeline with project-colored time blocks, plus
//! a compact form below to add / edit / delete entries.

use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
use gpui::{div, prelude::*, px, rgb, Context, Entity, Window};
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::app::AppState;
use crate::models::{
    format_dur_ru, local_hm, local_hm_to_utc, local_minutes, monday_of, parse_hm, week_days, Id,
};
use crate::palette;

const START_H: i64 = 8;
const HOURS: i64 = 11; // 08:00 .. 18:00
const HOUR_H: f32 = 64.0;
const TOTAL_H: f32 = HOURS as f32 * HOUR_H;

const WD: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];

fn ru_month(m: u32) -> &'static str {
    [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
        "октября", "ноября", "декабря",
    ]
    .get((m as usize).saturating_sub(1))
    .copied()
    .unwrap_or("")
}

#[derive(Clone)]
struct Block {
    id: Id,
    project_id: Id,
    day: usize,
    start_hm: String,
    end_hm: String,
    desc: String,
    top: f32,
    height: f32,
    main: u32,
    soft: u32,
    text: u32,
}

pub struct CalendarView {
    app: Entity<AppState>,
    anchor: NaiveDate, // any date within the displayed week
    desc: Entity<InputState>,
    start_in: Entity<InputState>,
    end_in: Entity<InputState>,
    form_project: Option<Id>,
    form_picker_open: bool,
    form_day: usize,
    editing_id: Option<Id>,
}

impl CalendarView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let desc = cx.new(|cx| InputState::new(window, cx).placeholder("Описание"));
        let start_in = cx.new(|cx| InputState::new(window, cx).placeholder("09:00"));
        let end_in = cx.new(|cx| InputState::new(window, cx).placeholder("10:30"));
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            anchor: Local::now().date_naive(),
            desc,
            start_in,
            end_in,
            form_project: None,
            form_picker_open: false,
            form_day: Local::now().date_naive().weekday().num_days_from_monday() as usize,
            editing_id: None,
        }
    }

    fn reset_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_id = None;
        self.form_picker_open = false;
        self.desc.update(cx, |s, cx| s.set_value("", window, cx));
        self.start_in.update(cx, |s, cx| s.set_value("", window, cx));
        self.end_in.update(cx, |s, cx| s.set_value("", window, cx));
    }

    fn save_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pid = self
            .form_project
            .or_else(|| self.app.read(cx).db.list_projects().ok().and_then(|p| p.first().map(|x| x.id)));
        let Some(pid) = pid else { return };
        let (Some((sh, sm)), Some((eh, em))) = (
            parse_hm(&self.start_in.read(cx).value()),
            parse_hm(&self.end_in.read(cx).value()),
        ) else {
            return;
        };
        let day = week_days(monday_of(self.anchor))[self.form_day];
        let (Some(start), Some(end)) = (local_hm_to_utc(day, sh, sm), local_hm_to_utc(day, eh, em)) else {
            return;
        };
        let desc = self.desc.read(cx).value().to_string();
        let desc = desc.trim();
        let desc_opt = (!desc.is_empty()).then_some(desc);
        let editing = self.editing_id;
        self.app.update(cx, |s, _| {
            let r = match editing {
                Some(id) => s.db.update_entry(id, pid, start, Some(end), desc_opt),
                None => s.db.add_manual_entry(pid, start, end, desc_opt).map(|_| ()),
            };
            if let Err(e) = r {
                eprintln!("save entry: {e:#}");
            }
        });
        self.reset_form(window, cx);
        cx.notify();
    }
}

impl Render for CalendarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let monday = monday_of(self.anchor);
        let week = week_days(monday);
        let today = Local::now().date_naive();

        let app = self.app.read(cx);
        let entries = app.db.entries_for_week(monday).unwrap_or_default();
        let projects = app.db.list_projects().unwrap_or_default();
        let _ = app;

        // group entries into per-day blocks
        let now = Utc::now();
        let mut by_day: Vec<Vec<Block>> = (0..7).map(|_| Vec::new()).collect();
        for e in &entries {
            let d = e.entry.local_date();
            let Some(i) = week.iter().position(|x| *x == d) else { continue };
            let s = local_minutes(e.entry.start());
            let end_min = e.entry.end().map(local_minutes).unwrap_or_else(|| local_minutes(now));
            let top = (((s - START_H * 60) as f32) / 60.0 * HOUR_H).clamp(0.0, TOTAL_H - 12.0);
            let height = (((end_min - s) as f32) / 60.0 * HOUR_H - 3.0).clamp(20.0, TOTAL_H);
            let pal = palette::pal_for_hex(&e.color);
            by_day[i].push(Block {
                id: e.entry.id,
                project_id: e.project_id,
                day: i,
                start_hm: local_hm(e.entry.start()),
                end_hm: e.entry.end().map(local_hm).unwrap_or_default(),
                desc: e.entry.desc_or("Без названия"),
                top,
                height,
                main: pal.main,
                soft: pal.soft,
                text: pal.text,
            });
        }

        // header + nav
        let week_label = format!(
            "{} – {} {} {}",
            week[0].day(),
            week[6].day(),
            ru_month(week[6].month()),
            week[6].year()
        );
        let nav = h_flex()
            .gap(px(8.))
            .items_center()
            .child(
                h_flex()
                    .border_1()
                    .border_color(rgb(palette::BORDER))
                    .rounded(px(9.))
                    .overflow_hidden()
                    .child(
                        div()
                            .id("cal-prev")
                            .px(px(11.)).py(px(8.))
                            .cursor_pointer()
                            .text_color(rgb(palette::LABEL))
                            .border_r_1().border_color(rgb(palette::BORDER))
                            .child(Icon::new(IconName::ChevronLeft).xsmall())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.anchor -= Duration::days(7);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("cal-next")
                            .px(px(11.)).py(px(8.))
                            .cursor_pointer()
                            .text_color(rgb(palette::LABEL))
                            .child(Icon::new(IconName::ChevronRight).xsmall())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.anchor += Duration::days(7);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("cal-today")
                    .px(px(14.)).py(px(8.))
                    .border_1().border_color(rgb(palette::BORDER))
                    .rounded(px(9.))
                    .text_size(px(13.)).font_medium().text_color(rgb(0x3f3f46))
                    .cursor_pointer()
                    .child("Сегодня")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.anchor = Local::now().date_naive();
                        cx.notify();
                    })),
            );

        let header = h_flex().items_end().justify_between().mb(px(20.))
            .child(
                v_flex()
                    .child(div().text_size(px(25.)).font_semibold().child("Календарь"))
                    .child(div().mt(px(5.)).text_size(px(13.5)).text_color(rgb(palette::TEXT_2)).child(week_label)),
            )
            .child(nav);

        // day-of-week header row
        let day_head = h_flex()
            .border_1().border_color(rgb(palette::BORDER))
            .rounded_t(px(12.))
            .overflow_hidden()
            .child(div().w(px(54.)).flex_shrink_0().bg(rgb(palette::SURFACE)).border_r_1().border_color(rgb(palette::BORDER_2)))
            .children(week.iter().enumerate().map(|(i, d)| {
                let is_today = *d == today;
                v_flex().flex_1().items_center().py(px(11.)).bg(rgb(palette::SURFACE)).border_r_1().border_color(rgb(palette::BORDER_2))
                    .child(div().text_size(px(11.)).font_medium().text_color(rgb(palette::TEXT_3)).child(WD[i]))
                    .child(
                        div().mt(px(5.)).w(px(28.)).h(px(28.)).flex().items_center().justify_center()
                            .rounded_full().text_size(px(14.)).font_semibold()
                            .when(is_today, |x| x.bg(rgb(palette::ACCENT)).text_color(rgb(0xffffff)))
                            .when(!is_today, |x| x.text_color(rgb(0x3f3f46)))
                            .child(d.day().to_string()),
                    )
            }));

        // hour gutter
        let gutter = div().w(px(54.)).flex_shrink_0().border_r_1().border_color(rgb(palette::BORDER_2)).children(
            (0..HOURS).map(|i| {
                div().h(px(HOUR_H)).flex().justify_end().pr(px(8.))
                    .child(div().text_size(px(10.5)).text_color(rgb(palette::FAINT)).child(format!("{:02}:00", START_H + i)))
            }),
        );

        // day columns with blocks
        let columns = week.iter().enumerate().map(|(i, d)| {
            let is_today = *d == today;
            let grid = div().absolute().inset_0().children((0..HOURS).map(|_| {
                div().h(px(HOUR_H)).border_b_1().border_color(rgb(palette::HAIRLINE))
            }));
            let blocks = by_day[i].iter().map(|b| {
                let (id, pid, day, sh, eh, desc) =
                    (b.id, b.project_id, b.day, b.start_hm.clone(), b.end_hm.clone(), b.desc.clone());
                div()
                    .id(("blk", id as usize))
                    .absolute()
                    .left(px(4.)).right(px(4.)).top(px(b.top)).h(px(b.height))
                    .rounded(px(7.)).px(px(7.)).py(px(5.)).overflow_hidden()
                    .bg(rgb(b.soft)).text_color(rgb(b.text))
                    .border_l(px(3.)).border_color(rgb(b.main))
                    .text_size(px(11.))
                    .cursor_pointer()
                    .child(div().font_semibold().overflow_hidden().child(b.desc.clone()))
                    .child(div().mt(px(1.)).child(format_dur_ru(((parse_minutes(&b.end_hm).unwrap_or(0) - parse_minutes(&b.start_hm).unwrap_or(0)).max(0)) * 60)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.editing_id = Some(id);
                        this.form_project = Some(pid);
                        this.form_picker_open = false;
                        this.form_day = day;
                        let (s, e, d2) = (sh.clone(), eh.clone(), desc.clone());
                        this.desc.update(cx, |st, cx| st.set_value(d2, window, cx));
                        this.start_in.update(cx, |st, cx| st.set_value(s, window, cx));
                        this.end_in.update(cx, |st, cx| st.set_value(e, window, cx));
                        cx.notify();
                    }))
            });
            div()
                .relative()
                .flex_1()
                .h(px(TOTAL_H))
                .border_r_1().border_color(rgb(palette::HAIRLINE))
                .when(is_today, |x| x.bg(rgb(0xfcfcff)))
                .child(grid)
                .children(blocks)
        });

        let timeline = div()
            .border_1().border_color(rgb(palette::BORDER))
            .border_t_0()
            .rounded_b(px(12.))
            .overflow_hidden()
            .child(h_flex().items_start().child(gutter).children(columns));

        // --- form below ------------------------------------------------------
        let editing = self.editing_id.is_some();
        let sel_pid = self
            .form_project
            .or_else(|| projects.first().map(|p| p.id))
            .unwrap_or(0);

        let day_buttons = h_flex().gap(px(4.)).children((0..7).map(|i| {
            let on = self.form_day == i;
            div()
                .id(("fday", i))
                .px(px(10.)).py(px(7.)).rounded(px(8.)).cursor_pointer().text_size(px(12.5)).font_medium()
                .border_1().border_color(rgb(if on { palette::ACCENT } else { palette::BORDER }))
                .text_color(rgb(if on { palette::ACCENT_DK } else { palette::LABEL }))
                .when(on, |d| d.bg(rgb(palette::ACCENT_SOFT)))
                .child(WD[i])
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.form_day = i;
                    cx.notify();
                }))
        }));

        let proj_col = crate::ui::common::project_dropdown(
            "cal-proj",
            &projects,
            sel_pid,
            self.form_picker_open,
            220.0,
            |this: &mut Self, cx| {
                this.form_picker_open = !this.form_picker_open;
                cx.notify();
            },
            std::rc::Rc::new(|this: &mut Self, pid, cx| {
                this.form_project = Some(pid);
                this.form_picker_open = false;
                cx.notify();
            }),
            cx,
        );

        let mut buttons = h_flex().gap(px(8.)).child(
            div()
                .id("cal-save")
                .flex().items_center().gap(px(7.)).px(px(16.)).h(px(40.)).rounded(px(10.)).cursor_pointer()
                .bg(rgb(palette::ACCENT)).text_color(rgb(0xffffff)).text_size(px(13.5)).font_semibold()
                .child(Icon::new(if editing { IconName::Check } else { IconName::Plus }).xsmall().text_color(rgb(0xffffff)))
                .child(div().child(if editing { "Сохранить" } else { "Добавить" }))
                .on_click(cx.listener(|this, _, window, cx| this.save_form(window, cx))),
        );
        if editing {
            buttons = buttons
                .child(
                    div().id("cal-del").flex().items_center().gap(px(6.)).px(px(14.)).h(px(40.)).rounded(px(10.)).cursor_pointer()
                        .bg(rgb(palette::DANGER)).text_color(rgb(0xffffff)).text_size(px(13.5)).font_semibold()
                        .child(Icon::new(IconName::Delete).xsmall().text_color(rgb(0xffffff)))
                        .child(div().child("Удалить"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(id) = this.editing_id {
                                this.app.update(cx, |s, _| {
                                    let _ = s.db.delete_entry(id);
                                });
                            }
                            this.reset_form(window, cx);
                            cx.notify();
                        })),
                )
                .child(
                    div().id("cal-cancel").flex().items_center().px(px(14.)).h(px(40.)).rounded(px(10.)).cursor_pointer()
                        .text_color(rgb(palette::LABEL)).text_size(px(13.5)).font_medium()
                        .child("Отмена")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.reset_form(window, cx);
                            cx.notify();
                        })),
                );
        }

        let form = v_flex()
            .mt(px(18.)).gap(px(10.))
            .p(px(16.))
            .border_1().border_color(rgb(palette::BORDER)).rounded(px(14.)).bg(rgb(palette::CARD))
            .child(div().text_size(px(13.)).font_semibold().text_color(rgb(palette::LABEL)).child(if editing { "Редактировать запись" } else { "Новая запись" }))
            .child(
                h_flex().gap(px(8.)).items_start().flex_wrap()
                    .child(proj_col)
                    .child(div().flex_1().min_w(px(160.)).child(Input::new(&self.desc)))
                    .child(div().w(px(92.)).child(Input::new(&self.start_in)))
                    .child(div().w(px(92.)).child(Input::new(&self.end_in))),
            )
            .child(h_flex().items_center().justify_between().flex_wrap().gap(px(8.)).child(day_buttons).child(buttons));

        // --- page ------------------------------------------------------------
        div()
            .px(px(40.)).pt(px(34.)).pb(px(40.))
            .child(header)
            .child(day_head)
            .child(timeline)
            .child(form)
    }
}

fn parse_minutes(hm: &str) -> Option<i64> {
    parse_hm(hm).map(|(h, m)| h as i64 * 60 + m as i64)
}

//! Tracker tab: a work bar (description + project + clock + Start/Stop) and a
//! list of today's entries with a running highlight and per-entry replay.

use chrono::{Local, Utc};
use gpui::{div, prelude::*, px, rgb, Context, Entity, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::app::AppState;
use crate::models::{format_dur_ru, format_hms, local_hm, Id};
use crate::palette;
use crate::ui::common::dot;

pub struct TrackerView {
    app: Entity<AppState>,
    desc: Entity<InputState>,
    draft_project: Option<Id>,
    picker_open: bool,
}

impl TrackerView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let desc = cx.new(|cx| InputState::new(window, cx).placeholder("На чём работаете?"));
        if let Some(text) = app.read(cx).running.as_ref().map(|r| r.description.clone()) {
            desc.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        // Editing the bar while running live-updates the running entry.
        cx.subscribe(&desc, |this, inp, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let text = inp.read(cx).value().to_string();
                this.app.update(cx, |st, cx| st.set_running_desc(&text, cx));
            }
        })
        .detach();
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            desc,
            draft_project: None,
            picker_open: false,
        }
    }

    fn start_or_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let running = self.app.read(cx).running.is_some();
        if running {
            self.app.update(cx, |s, cx| s.stop(cx));
            self.desc.update(cx, |s, cx| s.set_value("", window, cx));
        } else {
            let pid = self.draft_project.or_else(|| {
                self.app
                    .read(cx)
                    .db
                    .list_projects()
                    .ok()
                    .and_then(|p| p.first().map(|x| x.id))
            });
            if let Some(pid) = pid {
                let text = self.desc.read(cx).value().to_string();
                self.app.update(cx, |s, cx| s.start(pid, &text, cx));
            }
        }
    }
}

impl Render for TrackerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let running = app.running.as_ref().map(|r| {
            (
                r.entry_id,
                r.description.clone(),
                r.project.clone(),
                palette::hex_to_u32(&r.color),
                r.start,
            )
        });
        let projects = app.db.list_projects().unwrap_or_default();
        let today = Local::now().date_naive();
        let entries = app.db.entries_for_day(today).unwrap_or_default();
        let total = app.db.day_total_secs(today).unwrap_or(0);
        let _ = app;

        let is_running = running.is_some();
        let elapsed = running
            .as_ref()
            .map(|r| (Utc::now() - r.4).num_seconds().max(0))
            .unwrap_or(0);
        let sel_id = self.draft_project.or_else(|| projects.first().map(|p| p.id));
        let sel = sel_id.and_then(|id| projects.iter().find(|p| p.id == id));
        let sel_color = sel.map(|p| palette::hex_to_u32(&p.color)).unwrap_or(palette::MUTED);
        let sel_name = sel.map(|p| p.name.clone()).unwrap_or_else(|| "Нет проектов".into());

        // --- the work bar ----------------------------------------------------
        let chip = div()
            .id("proj-chip")
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .py(px(8.))
            .border_1()
            .border_color(rgb(palette::BORDER))
            .rounded(px(9.))
            .cursor_pointer()
            .bg(rgb(0xfcfcfd))
            .text_size(px(13.))
            .font_medium()
            .text_color(rgb(0x3f3f46))
            .child(dot(sel_color, 8.))
            .child(div().child(sel_name))
            .child(Icon::new(IconName::ChevronDown).xsmall().text_color(rgb(palette::MUTED)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.picker_open = !this.picker_open;
                cx.notify();
            }));

        let toggle = div()
            .id("toggle-timer")
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .h(px(44.))
            .px(px(20.))
            .rounded(px(11.))
            .cursor_pointer()
            .bg(rgb(if is_running { palette::DANGER } else { palette::ACCENT }))
            .text_color(rgb(0xffffff))
            .child(if is_running {
                div().w(px(11.)).h(px(11.)).rounded(px(2.)).bg(rgb(0xffffff)).into_any_element()
            } else {
                Icon::new(IconName::Play).xsmall().text_color(rgb(0xffffff)).into_any_element()
            })
            .child(
                div()
                    .text_size(px(14.))
                    .font_semibold()
                    .child(if is_running { "Стоп" } else { "Старт" }),
            )
            .on_click(cx.listener(|this, _, window, cx| this.start_or_stop(window, cx)));

        let bar = h_flex()
            .items_center()
            .gap(px(12.))
            .p(px(14.))
            .border_1()
            .border_color(rgb(palette::BORDER))
            .rounded(px(14.))
            .bg(rgb(palette::CARD))
            .shadow_sm()
            .child(div().flex_1().min_w(px(0.)).child(Input::new(&self.desc)))
            .child(chip)
            .child(
                div()
                    .min_w(px(128.))
                    .text_size(px(27.))
                    .font_semibold()
                    .text_color(rgb(if is_running { palette::ACCENT } else { palette::MUTED }))
                    .child(format_hms(elapsed)),
            )
            .child(toggle);

        // inline project picker
        let picker = self.picker_open.then(|| {
            v_flex()
                .mt(px(6.))
                .p(px(6.))
                .border_1()
                .border_color(rgb(palette::BORDER))
                .rounded(px(11.))
                .bg(rgb(palette::CARD))
                .shadow_lg()
                .children(projects.iter().map(|p| {
                    let pid = p.id;
                    let selected = sel_id == Some(pid);
                    h_flex()
                        .id(("pick", pid as usize))
                        .items_center()
                        .gap(px(9.))
                        .px(px(10.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .cursor_pointer()
                        .when(selected, |d| d.bg(rgb(0xf7f7fb)))
                        .text_size(px(13.))
                        .child(dot(palette::hex_to_u32(&p.color), 9.))
                        .child(div().flex_1().child(p.name.clone()))
                        .when(selected, |d| {
                            d.child(Icon::new(IconName::Check).xsmall().text_color(rgb(palette::ACCENT)))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.draft_project = Some(pid);
                            this.picker_open = false;
                            cx.notify();
                        }))
                }))
        });

        // --- today's entries -------------------------------------------------
        let run_row = running.as_ref().map(|(_, desc, project, color, start)| {
            let secs = (Utc::now() - *start).num_seconds().max(0);
            h_flex()
                .items_center()
                .gap(px(14.))
                .px(px(18.))
                .py(px(15.))
                .border_b_1()
                .border_color(rgb(palette::HAIRLINE))
                .bg(rgb(0xfafaff))
                .child(dot(*color, 9.))
                .child(
                    v_flex().flex_1().min_w(px(0.)).child(
                        div()
                            .text_size(px(14.))
                            .font_medium()
                            .child(if desc.is_empty() { "Без названия".to_string() } else { desc.clone() }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette::TEXT_3))
                            .child(format!("{project} · идёт сейчас")),
                    ),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_semibold()
                        .text_color(rgb(*color))
                        .child(format_hms(secs)),
                )
        });

        let run_id = running.as_ref().map(|r| r.0);
        let rows = entries
            .iter()
            .filter(|e| Some(e.entry.id) != run_id)
            .map(|e| {
                let pid = e.project_id;
                let desc = e.entry.desc_or("Без названия");
                let project = e.project.clone();
                let color = palette::hex_to_u32(&e.color);
                let range = format!("{} – {}", local_hm(e.entry.start()), e.entry.end().map(local_hm).unwrap_or_default());
                let dur = format_dur_ru(e.entry.duration_secs(Utc::now()));
                let replay_desc = desc.clone();
                h_flex()
                    .items_center()
                    .gap(px(14.))
                    .px(px(18.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(rgb(palette::HAIRLINE_2))
                    .child(dot(color, 9.))
                    .child(
                        v_flex().flex_1().min_w(px(0.))
                            .child(div().text_size(px(14.)).font_medium().child(desc))
                            .child(div().text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child(format!("{project} · {range}"))),
                    )
                    .child(div().text_size(px(14.)).font_semibold().text_color(rgb(0x27272a)).child(dur))
                    .child(
                        div()
                            .id(("replay", e.entry.id as usize))
                            .w(px(30.)).h(px(30.))
                            .flex().items_center().justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .text_color(rgb(palette::MUTED))
                            .child(Icon::new(IconName::Play).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.app.update(cx, |s, cx| s.start(pid, &replay_desc, cx));
                            })),
                    )
            })
            .collect::<Vec<_>>();

        // --- page ------------------------------------------------------------
        div().max_w(px(840.)).mx_auto().px(px(40.)).pt(px(34.)).pb(px(60.))
            .child(
                v_flex().mb(px(22.))
                    .child(div().text_size(px(25.)).font_semibold().child("Трекер"))
                    .child(div().mt(px(5.)).text_size(px(13.5)).text_color(rgb(palette::TEXT_2)).child("Среда, 24 июня 2026")),
            )
            .child(bar)
            .children(picker)
            .child(
                h_flex().items_center().justify_between().mt(px(32.)).mb(px(12.))
                    .child(div().text_size(px(13.)).font_semibold().text_color(rgb(palette::LABEL)).child("ЗАПИСИ ЗА СЕГОДНЯ"))
                    .child(div().text_size(px(13.)).text_color(rgb(palette::MUTED)).child(format_dur_ru(total))),
            )
            .child(
                v_flex()
                    .border_1()
                    .border_color(rgb(palette::BORDER))
                    .rounded(px(14.))
                    .overflow_hidden()
                    .bg(rgb(palette::CARD))
                    .children(run_row)
                    .children(rows),
            )
    }
}

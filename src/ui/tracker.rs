//! Tracker tab: a work bar (description + project + clock + Start/Stop) and a
//! list of today's entries with a running highlight and per-entry replay.

use chrono::{Local, NaiveDate, Utc};
use gpui::{div, prelude::*, px, rgb, Context, Entity, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::app::AppState;
use crate::models::{format_dur_ru, format_hms, local_hm, local_hm_to_utc, parse_hm, Id};
use crate::palette;
use crate::ui::common::dot;

pub struct TrackerView {
    app: Entity<AppState>,
    desc: Entity<InputState>,
    draft_project: Option<Id>,
    picker_open: bool,
    // Inline edit of a finished entry in today's list.
    edit_id: Option<Id>,
    edit_pid: Id,
    edit_date: NaiveDate,
    edit_desc: Entity<InputState>,
    edit_start: Entity<InputState>,
    edit_end: Entity<InputState>,
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
        let edit_desc = cx.new(|cx| InputState::new(window, cx).placeholder("На чём работали?"));
        let edit_start = cx.new(|cx| InputState::new(window, cx).placeholder("09:00"));
        let edit_end = cx.new(|cx| InputState::new(window, cx).placeholder("10:30"));
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            desc,
            draft_project: None,
            picker_open: false,
            edit_id: None,
            edit_pid: 0,
            edit_date: Local::now().date_naive(),
            edit_desc,
            edit_start,
            edit_end,
        }
    }

    /// Enter inline-edit for a finished entry: populate the edit inputs.
    fn begin_edit(
        &mut self,
        id: Id,
        pid: Id,
        date: NaiveDate,
        desc: String,
        start_hm: String,
        end_hm: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_id = Some(id);
        self.edit_pid = pid;
        self.edit_date = date;
        self.edit_desc.update(cx, |s, cx| s.set_value(desc, window, cx));
        self.edit_start.update(cx, |s, cx| s.set_value(start_hm, window, cx));
        self.edit_end.update(cx, |s, cx| s.set_value(end_hm, window, cx));
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.edit_id = None;
        cx.notify();
    }

    fn save_edit(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.edit_id else { return };
        let (Some((sh, sm)), Some((eh, em))) = (
            parse_hm(&self.edit_start.read(cx).value()),
            parse_hm(&self.edit_end.read(cx).value()),
        ) else {
            return;
        };
        let (Some(start), Some(end)) = (
            local_hm_to_utc(self.edit_date, sh, sm),
            local_hm_to_utc(self.edit_date, eh, em),
        ) else {
            return;
        };
        let desc = self.edit_desc.read(cx).value().to_string();
        let desc = desc.trim();
        let desc_opt = (!desc.is_empty()).then_some(desc);
        let pid = self.edit_pid;
        self.app.update(cx, |s, _| {
            if let Err(e) = s.db.update_entry(id, pid, start, Some(end), desc_opt) {
                eprintln!("update_entry: {e:#}");
            }
        });
        self.edit_id = None;
        cx.notify();
    }

    fn delete_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.edit_id {
            self.app.update(cx, |s, _| {
                if let Err(e) = s.db.delete_entry(id) {
                    eprintln!("delete_entry: {e:#}");
                }
            });
        }
        self.edit_id = None;
        cx.notify();
    }

    /// The inline edit form rendered in place of a row being edited.
    fn edit_row(
        &self,
        projects: &[crate::models::Project],
        proj_ids: &[Id],
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ep = projects
            .iter()
            .find(|p| p.id == self.edit_pid)
            .or_else(|| projects.first());
        let ename = ep.map(|p| p.name.clone()).unwrap_or_else(|| "—".into());
        let ecolor = ep.map(|p| palette::hex_to_u32(&p.color)).unwrap_or(palette::MUTED);
        let ids = proj_ids.to_vec();

        let chip = h_flex()
            .id("edit-proj")
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
            .child(dot(ecolor, 8.))
            .child(div().child(ename))
            .child(Icon::new(IconName::ChevronDown).xsmall().text_color(rgb(palette::MUTED)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if ids.is_empty() {
                    return;
                }
                let idx = ids.iter().position(|x| *x == this.edit_pid).unwrap_or(0);
                this.edit_pid = ids[(idx + 1) % ids.len()];
                cx.notify();
            }));

        let buttons = h_flex()
            .gap(px(6.))
            .child(
                div()
                    .id("edit-save")
                    .flex().items_center().gap(px(6.)).px(px(14.)).h(px(38.)).rounded(px(9.))
                    .cursor_pointer()
                    .bg(rgb(palette::ACCENT)).text_color(rgb(0xffffff)).text_size(px(13.)).font_semibold()
                    .child(Icon::new(IconName::Check).xsmall().text_color(rgb(0xffffff)))
                    .child(div().child("Сохранить"))
                    .on_click(cx.listener(|this, _, _, cx| this.save_edit(cx))),
            )
            .child(
                div()
                    .id("edit-del")
                    .flex().items_center().justify_center().w(px(38.)).h(px(38.)).rounded(px(9.))
                    .cursor_pointer()
                    .bg(rgb(palette::DANGER)).text_color(rgb(0xffffff))
                    .child(Icon::new(IconName::Delete).xsmall().text_color(rgb(0xffffff)))
                    .on_click(cx.listener(|this, _, _, cx| this.delete_edit(cx))),
            )
            .child(
                div()
                    .id("edit-cancel")
                    .flex().items_center().px(px(12.)).h(px(38.)).rounded(px(9.))
                    .cursor_pointer()
                    .text_color(rgb(palette::LABEL)).text_size(px(13.)).font_medium()
                    .child("Отмена")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_edit(cx))),
            );

        h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(14.))
            .py(px(12.))
            .border_b_1()
            .border_color(rgb(palette::HAIRLINE_2))
            .bg(rgb(0xfafaff))
            .child(chip)
            .child(div().flex_1().min_w(px(140.)).child(Input::new(&self.edit_desc)))
            .child(div().w(px(72.)).child(Input::new(&self.edit_start)))
            .child(div().text_color(rgb(palette::MUTED)).child("–"))
            .child(div().w(px(72.)).child(Input::new(&self.edit_end)))
            .child(buttons)
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
        let proj_ids: Vec<Id> = projects.iter().map(|p| p.id).collect();
        let rows = entries
            .iter()
            .filter(|e| Some(e.entry.id) != run_id)
            .map(|e| {
                let id = e.entry.id;
                if self.edit_id == Some(id) {
                    return self.edit_row(&projects, &proj_ids, cx);
                }
                let pid = e.project_id;
                let raw_desc = e.entry.description.clone().unwrap_or_default();
                let desc = e.entry.desc_or("Без названия");
                let project = e.project.clone();
                let color = palette::hex_to_u32(&e.color);
                let start_hm = local_hm(e.entry.start());
                let end_hm = e.entry.end().map(local_hm).unwrap_or_default();
                let range = format!("{start_hm} – {end_hm}");
                let dur = format_dur_ru(e.entry.duration_secs(Utc::now()));
                let replay_desc = desc.clone();
                let date = e.entry.local_date();
                let (s_hm, e_hm, ed_desc) = (start_hm.clone(), end_hm.clone(), raw_desc.clone());
                h_flex()
                    .items_center()
                    .gap(px(14.))
                    .px(px(18.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(rgb(palette::HAIRLINE_2))
                    .child(dot(color, 9.))
                    .child(
                        div()
                            .id(("edit-row", id as usize))
                            .flex().flex_1().min_w(px(0.))
                            .cursor_pointer()
                            .child(
                                v_flex().flex_1().min_w(px(0.))
                                    .child(div().text_size(px(14.)).font_medium().child(desc))
                                    .child(div().text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child(format!("{project} · {range}"))),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.begin_edit(id, pid, date, ed_desc.clone(), s_hm.clone(), e_hm.clone(), window, cx);
                            })),
                    )
                    .child(div().text_size(px(14.)).font_semibold().text_color(rgb(0x27272a)).child(dur))
                    .child(
                        div()
                            .id(("replay", id as usize))
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

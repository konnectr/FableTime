//! Projects tab: a grid of project cards (color, client, weekly total, entry
//! count, a Mon–Sun sparkline, an editable per-project note) plus a "new
//! project" row.

use chrono::Local;
use gpui::{div, prelude::*, px, rgb, Context, Div, Entity, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use std::rc::Rc;

use crate::app::AppState;
use crate::db::{DayGroup, ProjectStat};
use crate::icons::Lucide;
use crate::models::{format_dur_ru, link_segments, monday_of, Id, Segment};
use crate::palette;
use crate::ui::common::{entry_row, EntryRow};

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
        Self { app, new_name, note_open_id: None, note_input, open_id: None, entry_note_open_id: None }
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
                let history = self.app.read(cx).db.project_history(id).unwrap_or_default();
                return self.detail_view(stat, history, cx).into_any_element();
            }
            self.open_id = None;
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
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_id = Some(pid);
                this.note_open_id = None;
                this.entry_note_open_id = None;
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
            .child(h_flex().justify_between().mt(px(7.)).children(day_labels));

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
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_id = None;
                this.entry_note_open_id = None;
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
                let note = it.note.clone().unwrap_or_default();
                let toggle = {
                    let n = note.clone();
                    Rc::new(move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.toggle_entry_note(eid, n.clone(), window, cx);
                    })
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
                groups = groups.child(entry_row(row, &self.note_input, toggle, None, None, cx));
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
            .child(strip);
        if let Some(note) = stat.project.note.clone().filter(|n| !n.trim().is_empty()) {
            page = page.child(self.detail_note(&note, cx));
        }
        page.child(groups)
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
}

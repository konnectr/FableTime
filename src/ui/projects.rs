//! Projects tab: a grid of project cards (color, client, weekly total, entry
//! count, a Mon–Sun sparkline) plus a "new project" row.

use chrono::Local;
use gpui::{div, prelude::*, px, rgb, Context, Div, Entity, Window};
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::app::AppState;
use crate::db::ProjectStat;
use crate::models::{format_dur_ru, monday_of};
use crate::palette;

pub struct ProjectsView {
    app: Entity<AppState>,
    new_name: Entity<InputState>,
}

impl ProjectsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("Название нового проекта"));
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self { app, new_name }
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

        let rows: Vec<Div> = stats
            .chunks(2)
            .map(|pair| {
                let mut row = h_flex().gap(px(14.)).items_stretch();
                for s in pair {
                    row = row.child(project_card(s));
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
                v_flex().mb(px(22.))
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
    }
}

fn project_card(stat: &ProjectStat) -> Div {
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

    div()
        .flex_1()
        .min_w(px(0.))
        .border_1()
        .border_color(rgb(palette::BORDER))
        .rounded(px(14.))
        .bg(rgb(palette::CARD))
        .p(px(18.))
        .shadow_sm()
        .child(
            h_flex().items_center().gap(px(10.))
                .child(div().w(px(11.)).h(px(11.)).flex_shrink_0().rounded(px(4.)).bg(rgb(color)))
                .child(
                    v_flex().flex_1().min_w(px(0.))
                        .child(div().text_size(px(15.)).font_semibold().child(stat.project.name.clone()))
                        .child(div().text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child(client)),
                ),
        )
        .child(
            h_flex().justify_between().items_end().mt(px(16.))
                .child(
                    v_flex()
                        .child(div().text_size(px(22.)).font_semibold().child(format_dur_ru(stat.week_secs)))
                        .child(div().text_size(px(11.5)).text_color(rgb(palette::MUTED)).child("за неделю")),
                )
                .child(
                    v_flex().items_end()
                        .child(div().text_size(px(14.)).font_semibold().text_color(rgb(0x3f3f46)).child(stat.entry_count.to_string()))
                        .child(div().text_size(px(11.5)).text_color(rgb(palette::MUTED)).child("записей")),
                ),
        )
        .child(h_flex().items_end().gap(px(5.)).h(px(38.)).mt(px(16.)).children(bars))
        .child(h_flex().justify_between().mt(px(7.)).children(day_labels))
}

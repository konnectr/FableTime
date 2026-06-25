//! Application shell: 38px top bar + 236px sidebar (4 tabs + Today card) + panel.

use chrono::Local;
use gpui::{div, prelude::*, px, rgb, Context, Entity, Window};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::app::AppState;
use crate::models::format_dur_ru;
use crate::palette;
use crate::ui::calendar::CalendarView;
use crate::ui::common::dot;
use crate::ui::export::ExportView;
use crate::ui::projects::ProjectsView;
use crate::ui::tracker::TrackerView;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tracker,
    Calendar,
    Projects,
    Export,
}

pub struct RootView {
    app: Entity<AppState>,
    active: Tab,
    tracker: Entity<TrackerView>,
    calendar: Entity<CalendarView>,
    projects: Entity<ProjectsView>,
    export: Entity<ExportView>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app = AppState::load(cx);
        let tracker = cx.new(|cx| TrackerView::new(app.clone(), window, cx));
        let calendar = cx.new(|cx| CalendarView::new(app.clone(), window, cx));
        let projects = cx.new(|cx| ProjectsView::new(app.clone(), window, cx));
        let export = cx.new(|cx| ExportView::new(app.clone(), window, cx));
        // Re-render the Today card on every tick / start / stop.
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        let active = match std::env::var("TIMETRACKER_TAB").ok().as_deref() {
            Some("calendar") => Tab::Calendar,
            Some("projects") => Tab::Projects,
            Some("export") => Tab::Export,
            _ => Tab::Tracker,
        };
        Self {
            app,
            active,
            tracker,
            calendar,
            projects,
            export,
        }
    }

    fn nav_item(
        &self,
        tab: Tab,
        icon: IconName,
        label: &'static str,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.active == tab;
        let fg = if active { palette::ACCENT_DK } else { palette::LABEL };
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(11.))
            .px(px(11.))
            .py(px(9.))
            .rounded(px(9.))
            .cursor_pointer()
            .text_color(rgb(fg))
            .when(active, |d| d.bg(rgb(palette::ACCENT_SOFT)))
            .child(Icon::new(icon).small().text_color(rgb(fg)))
            .child(div().text_size(px(13.5)).font_medium().child(label))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active = tab;
                cx.notify();
            }))
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let running = app.running.is_some();
        let total = app
            .db
            .day_total_secs(Local::now().date_naive())
            .unwrap_or(0);

        let card = v_flex()
            .p(px(14.))
            .border_1()
            .border_color(rgb(0xefeff1))
            .rounded(px(11.))
            .bg(rgb(palette::CARD))
            .child(
                div()
                    .text_size(px(11.))
                    .font_medium()
                    .text_color(rgb(palette::MUTED))
                    .child("Сегодня"),
            )
            .child(
                div()
                    .mt(px(5.))
                    .text_size(px(23.))
                    .font_semibold()
                    .text_color(rgb(palette::TEXT))
                    .child(format_dur_ru(total)),
            )
            .child(
                h_flex()
                    .mt(px(8.))
                    .gap(px(6.))
                    .items_center()
                    .child(dot(if running { palette::RUNNING } else { 0xd4d4d8 }, 6.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette::TEXT_2))
                            .child(if running { "Таймер идёт" } else { "Таймер остановлен" }),
                    ),
            );

        v_flex()
            .w(px(236.))
            .flex_shrink_0()
            .border_r_1()
            .border_color(rgb(palette::BORDER_2))
            .bg(rgb(palette::SURFACE))
            .px(px(14.))
            .py(px(18.))
            .child(
                v_flex()
                    .gap(px(3.))
                    .child(self.nav_item(Tab::Tracker, IconName::LayoutDashboard, "Трекер", "nav-tracker", cx))
                    .child(self.nav_item(Tab::Calendar, IconName::Calendar, "Календарь", "nav-calendar", cx))
                    .child(self.nav_item(Tab::Projects, IconName::FolderClosed, "Проекты", "nav-projects", cx))
                    .child(self.nav_item(Tab::Export, IconName::ArrowDown, "Экспорт", "nav-export", cx)),
            )
            .child(div().flex_1())
            .child(card)
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let logo = h_flex()
            .gap(px(8.))
            .items_center()
            .child(
                div()
                    .w(px(16.))
                    .h(px(16.))
                    .rounded(px(5.))
                    .bg(rgb(palette::ACCENT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(dot(0xffffff, 5.)),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .font_semibold()
                    .text_color(rgb(0x3f3f46))
                    .child("Time Tracker"),
            );

        let topbar = h_flex()
            .h(px(38.))
            .flex_shrink_0()
            .items_center()
            .px(px(14.))
            .border_b_1()
            .border_color(rgb(palette::BORDER_2))
            .bg(rgb(palette::SURFACE))
            .child(logo);

        let body = match self.active {
            Tab::Tracker => self.tracker.clone().into_any_element(),
            Tab::Calendar => self.calendar.clone().into_any_element(),
            Tab::Projects => self.projects.clone().into_any_element(),
            Tab::Export => self.export.clone().into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(rgb(palette::BG))
            .text_color(rgb(palette::TEXT))
            .child(topbar)
            .child(
                h_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .id("main-scroll")
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .overflow_y_scroll()
                            .child(body),
                    ),
            )
    }
}

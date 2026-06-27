//! Small shared UI helpers for the redesigned views.

use std::rc::Rc;

use gpui::{div, prelude::*, px, rgb, Context, Div, Render, SharedString};
use gpui_component::{h_flex, v_flex, Icon, IconName, Sizable, StyledExt};

use crate::models::{Id, Project};
use crate::palette;

/// Filled circle — project / status dot. `size` in px.
pub fn dot(color: u32, size: f32) -> Div {
    div()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .bg(rgb(color))
}

/// A reusable project picker: a chip showing the selected project and, when
/// `open`, an inline dropdown list directly beneath it (chip-width, in-flow so
/// it never gets clipped). Picker state lives in the parent view `V`; the chip
/// toggles it and each row picks via the provided callbacks.
pub fn project_dropdown<V: Render + 'static>(
    key: impl Into<SharedString>,
    projects: &[Project],
    selected: Id,
    open: bool,
    width: f32,
    on_toggle: impl Fn(&mut V, &mut Context<V>) + 'static,
    on_pick: Rc<dyn Fn(&mut V, Id, &mut Context<V>)>,
    cx: &mut Context<V>,
) -> Div {
    let key: SharedString = key.into();
    let sel = projects
        .iter()
        .find(|p| p.id == selected)
        .or_else(|| projects.first());
    let name = sel.map(|p| p.name.clone()).unwrap_or_else(|| "—".into());
    let color = sel.map(|p| palette::hex_to_u32(&p.color)).unwrap_or(palette::MUTED);

    let chip = h_flex()
        .id(SharedString::from(format!("{key}-chip")))
        .w_full()
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
        .child(dot(color, 8.))
        .child(div().flex_1().min_w(px(0.)).child(name))
        .child(Icon::new(IconName::ChevronDown).xsmall().text_color(rgb(palette::MUTED)))
        .on_click(cx.listener(move |this, _, _, cx| on_toggle(this, cx)));

    let list = open.then(|| {
        v_flex()
            .w_full()
            .mt(px(2.))
            .p(px(6.))
            .border_1()
            .border_color(rgb(palette::BORDER))
            .rounded(px(11.))
            .bg(rgb(palette::CARD))
            .shadow_lg()
            .children(projects.iter().map(|p| {
                let pid = p.id;
                let is_sel = selected == pid;
                let on_pick = on_pick.clone();
                h_flex()
                    .id(SharedString::from(format!("{key}-opt-{pid}")))
                    .items_center()
                    .gap(px(9.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(rgb(0xf7f7fb)))
                    .text_size(px(13.))
                    .child(dot(palette::hex_to_u32(&p.color), 9.))
                    .child(div().flex_1().child(p.name.clone()))
                    .when(is_sel, |d| {
                        d.child(Icon::new(IconName::Check).xsmall().text_color(rgb(palette::ACCENT)))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| on_pick(this, pid, cx)))
            }))
    });

    v_flex().w(px(width)).flex_shrink_0().child(chip).children(list)
}

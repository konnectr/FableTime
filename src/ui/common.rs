//! Small shared UI helpers for the redesigned views.

use std::rc::Rc;

use gpui::{div, prelude::*, px, rgb, AnyElement, Context, Div, Entity, Render, SharedString, Window};
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex, Icon, Sizable, StyledExt};
use gpui_component::IconName;

use crate::billing::PaidState;
use crate::icons::Lucide;
use crate::models::{Id, Project};
use crate::palette;

/// Display data for one time-entry row (a "task" line). Strings are
/// pre-formatted by the caller so this stays gpui-agnostic about time math.
pub struct EntryRow {
    pub id: Id,
    pub color: u32,
    pub desc: String,
    pub secondary: String, // e.g. "Проект · 09:00 – 10:30" or just "09:00 – 10:30"
    pub dur: String,       // "1ч 45м"
    pub note: String,      // raw note text, "" when none
    pub note_open: bool,   // is the note editor open for this entry
}

/// One entry row, rendered identically in the Tracker's today list and in a
/// project's history detail: dot · (desc + secondary [+ note preview]) · duration
/// · note button [· trailing], with the shared note editor below when open.
///
/// Callbacks are per-row `Rc` closures (built by the caller, capturing the id /
/// note they need) so the same visual works for any host view `V`:
/// - `on_toggle_note`: open/close the note editor (note button + preview click)
/// - `on_text_click`: optional — clicking the desc/secondary block (Tracker edit)
/// - `trailing`: optional extra action element (e.g. the Tracker replay button)
pub fn entry_row<V: Render + 'static>(
    row: EntryRow,
    note_input: &Entity<InputState>,
    on_toggle_note: Rc<dyn Fn(&mut V, &mut Window, &mut Context<V>)>,
    on_text_click: Option<Rc<dyn Fn(&mut V, &mut Window, &mut Context<V>)>>,
    trailing: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Div {
    let EntryRow { id, color, desc, secondary, dur, note, note_open } = row;
    let has_note = !note.trim().is_empty();
    let note_active = has_note || note_open;

    // Desc + secondary line (optionally clickable to edit).
    let mut text_block = div()
        .id(("erow-text", id as usize))
        .flex().flex_col().min_w(px(0.))
        .child(div().text_size(px(14.)).font_medium().truncate().child(desc))
        .child(div().text_size(px(12.)).text_color(rgb(palette::TEXT_3)).child(secondary));
    if let Some(on_click) = on_text_click {
        text_block = text_block
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)));
    }

    let mut text_col = v_flex().flex_1().min_w(px(0.)).child(text_block);
    if has_note && !note_open {
        let toggle = on_toggle_note.clone();
        text_col = text_col.child(
            div()
                .id(("erow-note-prev", id as usize))
                .flex().gap(px(6.)).mt(px(6.)).cursor_pointer()
                .child(Icon::new(Lucide::FileText).xsmall().text_color(rgb(palette::FAINT)))
                .child(
                    div().flex_1().min_w(px(0.)).truncate()
                        .text_size(px(12.5)).text_color(rgb(palette::NOTE_TEXT)).child(note.clone()),
                )
                .on_click(cx.listener(move |this, _, window, cx| toggle(this, window, cx))),
        );
    }

    let toggle_btn = on_toggle_note.clone();
    let mut main = h_flex()
        .items_center().gap(px(14.)).px(px(18.)).py(px(14.))
        .child(dot(color, 9.))
        .child(text_col)
        .child(div().text_size(px(14.)).font_semibold().text_color(rgb(0x27272a)).child(dur))
        .child(
            div()
                .id(("erow-note-btn", id as usize))
                .w(px(30.)).h(px(30.)).flex().items_center().justify_center()
                .rounded(px(8.)).cursor_pointer()
                .text_color(rgb(if note_active { palette::ACCENT } else { palette::NOTE_IDLE }))
                .hover(|s| s.bg(rgb(palette::HOVER_2)))
                .child(Icon::new(Lucide::FileText).small())
                .on_click(cx.listener(move |this, _, window, cx| toggle_btn(this, window, cx))),
        );
    if let Some(t) = trailing {
        main = main.child(t);
    }

    let mut root = v_flex().border_b_1().border_color(rgb(palette::HAIRLINE_2)).child(main);
    if note_open {
        root = root.child(
            div().px(px(18.)).pb(px(14.)).pl(px(41.)).child(Input::new(note_input)),
        );
    }
    root
}

/// Filled circle — project / status dot. `size` in px.
pub fn dot(color: u32, size: f32) -> Div {
    div()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .bg(rgb(color))
}

/// Small paid/partial/unpaid status chip for a billable project's time
/// entries — the `entry_row` `trailing` slot's payment-status badge.
pub fn paid_status_pill(state: PaidState) -> Div {
    let (bg, text, label) = match state {
        PaidState::Paid => (palette::PAID_SOFT, palette::PAID_TEXT, "Оплачено"),
        PaidState::Partial { .. } => (palette::UNPAID_SOFT, palette::UNPAID_TEXT, "Частично"),
        PaidState::Unpaid => (palette::BORDER_2, palette::TEXT_2, "К оплате"),
    };
    div()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(3.))
        .rounded(px(20.))
        .bg(rgb(bg))
        .text_size(px(10.5))
        .font_semibold()
        .text_color(rgb(text))
        .child(label)
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

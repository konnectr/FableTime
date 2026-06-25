//! Small shared UI helpers for the redesigned views.

use gpui::{div, prelude::*, px, rgb, Div};

/// Filled circle — project / status dot. `size` in px.
pub fn dot(color: u32, size: f32) -> Div {
    div()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .rounded_full()
        .bg(rgb(color))
}

// Some DB/model helpers (e.g. archive, in-memory open, decimal-hours) are
// intentional API surface not yet wired into the UI.
#![allow(dead_code)]

mod app;
mod db;
mod exporter;
mod models;
mod palette;
mod ui;

use gpui::{px, size, App, AppContext, Bounds, WindowBounds, WindowOptions};
use gpui_component_assets::Assets;

fn main() {
    // Bootstrap pattern from gpui-component's story/main.rs + getting-started:
    // configure assets, init components, then open a window whose root view is
    // wrapped in gpui_component::Root.
    let application = gpui_platform::application().with_assets(Assets);

    application.run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.activate(true);

        // Centered, sensibly-sized window, clamped to 85% of the display.
        let mut window_size = size(px(1100.), px(760.));
        if let Some(display) = cx.primary_display() {
            let ds = display.bounds().size;
            window_size.width = window_size.width.min(ds.width * 0.85);
            window_size.height = window_size.height.min(ds.height * 0.85);
        }
        let bounds = Bounds::centered(None, window_size, cx);

        cx.spawn(async move |cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(480.), px(360.))),
                ..Default::default()
            };
            let window = cx
                .open_window(options, |window, cx| {
                    let root_view = cx.new(|cx| ui::root::RootView::new(window, cx));
                    cx.new(|cx| gpui_component::Root::new(root_view, window, cx).bordered(false))
                })
                .expect("failed to open window");

            window
                .update(cx, |_, window, _| {
                    window.activate_window();
                    window.set_window_title("Time Tracker");
                })
                .ok();
        })
        .detach();
    });
}

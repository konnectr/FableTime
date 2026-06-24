mod app;
mod db;
mod models;
mod ui;

use gpui::{App, WindowOptions};
use gpui_component_assets::Assets;

fn main() {
    // Bootstrap pattern taken from gpui-component's own story/main.rs +
    // getting-started: configure assets, init components, then open a window
    // whose root view is wrapped in gpui_component::Root.
    let application = gpui_platform::application().with_assets(Assets);

    application.run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.activate(true);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let root_view = cx.new(|cx| ui::root::RootView::new(window, cx));
                cx.new(|cx| gpui_component::Root::new(root_view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}

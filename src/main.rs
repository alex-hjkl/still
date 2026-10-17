mod api;
mod artwork;
mod browser;
mod connection;
mod credentials;
mod diagnostics;
mod failure;
mod folders;
mod icons;
mod input;
mod keyboard;
mod layout;
mod likes;
mod media;
mod player;
mod queue;
mod requests;
mod settings;
mod slider;
mod theme;
mod ui;

use gpui::{
    App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem, WindowBounds, WindowOptions,
    actions, px, size,
};

actions!(still, [Quit]);

fn main() {
    diagnostics::init();
    let store = settings::SettingsStore::default();
    let settings = settings::Settings::from_store(store);
    let [width, height] = settings.value().window_size;
    Application::new()
        .with_assets(icons::Icons)
        .run(move |cx: &mut App| {
            input::init(cx);
            cx.on_action(|_: &Quit, cx| {
                diagnostics::event("Quit requested");
                cx.quit();
            });
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.set_menus(vec![Menu {
                name: "Still".into(),
                items: vec![MenuItem::action("Quit Still", Quit)],
            }]);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    diagnostics::event("Last window closed");
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(780.), px(640.))),
                    app_id: Some("com.still.player".into()),
                    ..Default::default()
                },
                move |window, cx| {
                    window.set_window_title("Still — Navidrome");
                    let view = cx.new(|cx| ui::PlayerView::new(window, cx, settings));
                    #[cfg(feature = "ui-tests")]
                    ui::start_ui_test(view.clone(), window, cx);
                    view
                },
            )
            .expect("Could not open the GPUI window");
            cx.activate(true);
        });
}

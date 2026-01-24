use super::{components::table::GenericTableDelegate, window::get_window_options, workspace::*};
use domain::dirty::gpui::State;
use gpui::*;
use gpui_component::{table::TableState, *};
use gpui_component_assets::Assets;
use injection::cross_cutting::InjectedServices;

actions!(window, [Enter, Backspace]);

pub async fn gpui_app(services: InjectedServices, state: State) {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let window_options = get_window_options(cx);
        gpui_component::init(cx);

        cx.bind_keys([
            KeyBinding::new("enter", Enter, None),
            KeyBinding::new("backspace", Backspace, None),
        ]);

        cx.open_window(window_options, |window, cx| {
            let all_tables: Vec<(String, Entity<TableState<GenericTableDelegate>>)> = vec![];

            let workspace_view = Workspace::view(cx, services.clone(), state.clone(), all_tables);
            cx.new(|cx| Root::new(workspace_view, window, cx))
        })
        .unwrap();
    });
}

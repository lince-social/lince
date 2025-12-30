use super::{state::*, workspace::*};
use crate::{
    infrastructure::cross_cutting::InjectedServices, presentation::gpui::window::get_window_options,
};
use gpui::*;
use gpui_component::*;
use gpui_component_assets::Assets;

actions!(window, [AddTodo, Backspace, ClearInput]);

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

        let state_entity = cx.new(|_| state.clone());
        let state_model = StateModel {
            inner: state_entity.clone(),
        };
        cx.set_global(state_model);

        cx.bind_keys([
            KeyBinding::new("enter", AddTodo, None),
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("escape", ClearInput, None),
        ]);

        cx.open_window(window_options, |window, cx| {
            let workspace_view = Workspace::view(cx, services, state);
            cx.new(|cx| Root::new(workspace_view, window, cx))
        })
        .unwrap();
    });
}

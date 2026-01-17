use super::{state::*, workspace::*};
use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::gpui::{components::table::GenericTableDelegate, window::get_window_options},
};
use gpui::*;
use gpui_component::{table::TableState, *};
use gpui_component_assets::Assets;

actions!(window, []);

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

        // cx.bind_keys([
        //     KeyBinding::new("escape", ClearInput, None),
        // ]);

        cx.open_window(window_options, |window, cx| {
            let all_tables: Vec<(String, Entity<TableState<GenericTableDelegate>>)> = state
                .tables
                .iter()
                .cloned()
                .map(|(name, table)| {
                    let table_state = cx.new(|cx| {
                        TableState::new(GenericTableDelegate::new(table), window, cx)
                            .col_resizable(true)
                            .col_movable(true)
                            .sortable(true)
                            .col_selectable(true)
                            .row_selectable(true)
                    });

                    (name, table_state)
                })
                .collect();

            let workspace_view = Workspace::view(cx, services, state, all_tables);
            cx.new(|cx| Root::new(workspace_view, window, cx))
        })
        .unwrap();
    });
}

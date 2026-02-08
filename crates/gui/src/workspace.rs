use crate::components::operation::Operation;

use super::{
    components::{collection::CollectionList, table::GenericTableDelegate},
    themes::catppuccin_mocha::mantle,
};
use application::operation::operation_execute;
use domain::{clean::collection::Collection, dirty::gpui::State};
use gpui::*;
use gpui_component::table::TableState;
use injection::cross_cutting::InjectedServices;
use utils::log;

pub struct Workspace {
    // pub focus_handle: FocusHandle,
    pub state: State,
    pub services: InjectedServices,
    pub collection_list: Entity<CollectionList>,
    pub table_entities: Vec<(String, Entity<TableState<GenericTableDelegate>>)>,
    pub operation: Entity<Operation>,
}

impl Workspace {
    fn new(
        cx: &mut Context<Self>,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(String, Entity<TableState<GenericTableDelegate>>)>,
    ) -> Self {
        let weak = cx.weak_entity();
        let collection_list =
            cx.new(|_| CollectionList::new(state.collections.clone(), weak.clone()));
        let focus_handle = cx.focus_handle();
        let operation = cx.new(|_| Operation::new(weak.clone(), focus_handle.clone()));

        let workspace = Self {
            state,
            services,
            collection_list,
            table_entities,
            operation,
        };

        workspace
    }

    pub fn view(
        cx: &mut App,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(String, Entity<TableState<GenericTableDelegate>>)>,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(cx, services, state, table_entities))
    }
    pub fn on_collection_selected(&mut self, collection_id: u32, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = services
                .repository
                .collection
                .set_active(&collection_id.to_string())
                .await
            {
                utils::log!(e, "failed to set active collection");
            }

            let rows = match services.repository.collection.get_all().await {
                Ok(rows) => rows,
                Err(e) => {
                    log!(e, "failed fetch collections");
                    vec![(Collection::error(), Vec::new())]
                }
            };

            let tables = match services.repository.collection.get_active_view_data().await {
                Ok((tables, _)) => tables,
                Err(e) => {
                    log!(e, "failed to fetch table data");
                    vec![]
                }
            };

            this.update(cx, move |owner, cx| {
                owner.state.collections = rows.clone();
                owner.state.tables = tables.clone();

                owner.collection_list.update(cx, move |bar, _| {
                    bar.collections = rows.clone();
                });

                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }
    pub fn on_view_selected(&mut self, cx: &mut Context<Self>, collection_id: u32, view_id: u32) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            match services
                .repository
                .collection
                .toggle_by_view_id(collection_id, view_id)
                .await
            {
                Ok(collections) => {
                    let tables = match services.repository.collection.get_active_view_data().await {
                        Ok((tables, _)) => tables,
                        Err(e) => {
                            log!(e, "failed to fetch table data");
                            vec![]
                        }
                    };

                    this.update(cx, move |owner, cx| {
                        owner.state.collections = collections.clone();
                        owner.state.tables = tables.clone();

                        owner.collection_list.update(cx, |bar, _| {
                            bar.collections = collections;
                        });

                        cx.notify();
                    })
                    .unwrap();
                }
                Err(e) => {
                    log!(e, "failed to set active view");
                }
            }
        })
        .detach();
    }
}

impl Workspace {
    pub fn send_operation(&mut self, cx: &mut Context<Self>, operation: String) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            match operation_execute(services.clone(), operation.clone()).await {
                Ok(_operationresult) => {
                    let tables = match services.repository.collection.get_active_view_data().await {
                        Ok((tables, _)) => tables,
                        Err(e) => {
                            log!(e, "failed to fetch table data");
                            vec![]
                        }
                    };

                    this.update(cx, move |owner, cx| {
                        owner.state.tables = tables.clone();
                        cx.notify();
                    })
                    .unwrap();
                }
                Err(e) => {
                    log!(e, "Failed to get collections");
                }
            }
        })
        .detach();
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.table_entities = self
            .state
            .tables
            .iter()
            .cloned()
            .map(|(name, table)| {
                let services = self.services.clone();
                let table_state = cx.new(|app_cx| {
                    TableState::new(GenericTableDelegate::new(table, name.clone(), services, app_cx), window, app_cx)
                        .col_resizable(true)
                        .col_movable(true)
                        .sortable(true)
                        .col_selectable(false)  // Disable to allow cell editing
                        .row_selectable(false)  // Disable to allow cell editing
                });
                (name, table_state)
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .bg(mantle())
            .text_color(rgb(0xffffff))
            .size_full()
            .p_3()
            .child(self.operation.clone())
            .child(self.collection_list.clone())
            .children(
                self.table_entities
                    .iter()
                    .map(|(_name, entity)| entity.clone()),
            )
    }
}

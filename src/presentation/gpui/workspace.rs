use crate::{
    domain::clean::collection::Collection,
    infrastructure::cross_cutting::InjectedServices,
    log,
    presentation::gpui::{
        components::{collection::CollectionList, table::GenericTableDelegate},
        state::State,
        themes::catppuccin_mocha::mantle,
    },
};
use gpui::*;
use gpui_component::table::TableState;

pub struct Workspace {
    pub focus_handle: FocusHandle,
    pub state: State,
    pub services: InjectedServices,
    pub collection_list: Entity<CollectionList>,
    pub table_entities: Vec<(String, Entity<TableState<GenericTableDelegate>>)>,
}

impl Workspace {
    fn new(
        cx: &mut Context<Self>,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(String, Entity<TableState<GenericTableDelegate>>)>,
    ) -> Self {
        let weak = cx.weak_entity();
        let collection_list = cx.new(|_| CollectionList::new(state.collections.clone(), weak));

        Self {
            focus_handle: cx.focus_handle(),
            state,
            services,
            collection_list,
            table_entities,
        }
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
                log!(e, "failed to set active collection");
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
                    this.update(cx, move |owner, cx| {
                        owner.state.collections = collections.clone();

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

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Check if table entities need to be recreated based on current state
        let current_table_names: Vec<String> = self
            .state
            .tables
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let entity_table_names: Vec<String> = self
            .table_entities
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        // If the table names don't match or counts are different, recreate entities
        if current_table_names != entity_table_names {
            self.table_entities = self
                .state
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
        }

        div()
            .flex()
            .flex_col()
            .gap_4()
            .bg(mantle())
            .size_full()
            .p_3()
            .track_focus(&self.focus_handle(cx))
            .child(self.collection_list.clone())
            .children(
                self.table_entities
                    .iter()
                    .map(|(_name, entity)| entity.clone()),
            )
            .text_color(rgb(0xffffff))
            .child("Hellour")
    }
}

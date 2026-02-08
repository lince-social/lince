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

// Default position for newly pinned views
const DEFAULT_PIN_POSITION_X: f64 = 300.0;
const DEFAULT_PIN_POSITION_Y: f64 = 200.0;

pub struct Workspace {
    // pub focus_handle: FocusHandle,
    pub state: State,
    pub services: InjectedServices,
    pub collection_list: Entity<CollectionList>,
    pub table_entities: Vec<(String, Entity<TableState<GenericTableDelegate>>)>,
    pub pinned_table_entities: Vec<(u32, String, Entity<TableState<GenericTableDelegate>>)>,
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
            cx.new(|_| CollectionList::new(state.collections.clone(), state.views_with_pin_info.clone(), weak.clone()));
        let focus_handle = cx.focus_handle();
        let operation = cx.new(|_| Operation::new(weak.clone(), focus_handle.clone()));

        let workspace = Self {
            state,
            services,
            collection_list,
            table_entities,
            pinned_table_entities: vec![],
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

            let pinned_views = match services.repository.collection.get_pinned_views().await {
                Ok(views) => views,
                Err(e) => {
                    log!(e, "failed to fetch pinned views");
                    vec![]
                }
            };

            let pinned_tables = match services.repository.collection.get_pinned_view_data().await {
                Ok(tables) => tables,
                Err(e) => {
                    log!(e, "failed to fetch pinned table data");
                    vec![]
                }
            };

            let views_with_pin_info = match services.repository.collection.get_views_with_pin_info().await {
                Ok(info) => info,
                Err(e) => {
                    log!(e, "failed to fetch views with pin info");
                    vec![]
                }
            };

            this.update(cx, move |owner, cx| {
                owner.state.collections = rows.clone();
                owner.state.tables = tables.clone();
                owner.state.pinned_views = pinned_views;
                owner.state.pinned_tables = pinned_tables;
                owner.state.views_with_pin_info = views_with_pin_info.clone();

                owner.collection_list.update(cx, move |bar, _| {
                    bar.collections = rows.clone();
                    bar.views_with_pin_info = views_with_pin_info;
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

                    let pinned_tables = match services.repository.collection.get_pinned_view_data().await {
                        Ok(tables) => tables,
                        Err(e) => {
                            log!(e, "failed to fetch pinned table data");
                            vec![]
                        }
                    };

                    let views_with_pin_info = match services.repository.collection.get_views_with_pin_info().await {
                        Ok(info) => info,
                        Err(e) => {
                            log!(e, "failed to fetch views with pin info");
                            vec![]
                        }
                    };

                    this.update(cx, move |owner, cx| {
                        owner.state.collections = collections.clone();
                        owner.state.tables = tables.clone();
                        owner.state.pinned_tables = pinned_tables;
                        owner.state.views_with_pin_info = views_with_pin_info.clone();

                        owner.collection_list.update(cx, |bar, _| {
                            bar.collections = collections.clone();
                            bar.views_with_pin_info = views_with_pin_info;
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

                    let pinned_tables = match services.repository.collection.get_pinned_view_data().await {
                        Ok(tables) => tables,
                        Err(e) => {
                            log!(e, "failed to fetch pinned table data");
                            vec![]
                        }
                    };

                    let views_with_pin_info = match services.repository.collection.get_views_with_pin_info().await {
                        Ok(info) => info,
                        Err(e) => {
                            log!(e, "failed to fetch views with pin info");
                            vec![]
                        }
                    };

                    this.update(cx, move |owner, cx| {
                        owner.state.tables = tables.clone();
                        owner.state.pinned_tables = pinned_tables;
                        owner.state.views_with_pin_info = views_with_pin_info;
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

    pub fn pin_view(&mut self, view_id: u32, position_x: f64, position_y: f64, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = services
                .repository
                .collection
                .pin_view(view_id, position_x, position_y)
                .await
            {
                log!(e, "failed to pin view");
                return;
            }

            let pinned_views = match services.repository.collection.get_pinned_views().await {
                Ok(views) => views,
                Err(e) => {
                    log!(e, "failed to fetch pinned views");
                    vec![]
                }
            };

            let pinned_tables = match services.repository.collection.get_pinned_view_data().await {
                Ok(tables) => tables,
                Err(e) => {
                    log!(e, "failed to fetch pinned table data");
                    vec![]
                }
            };

            let views_with_pin_info = match services.repository.collection.get_views_with_pin_info().await {
                Ok(info) => info,
                Err(e) => {
                    log!(e, "failed to fetch views with pin info");
                    vec![]
                }
            };

            this.update(cx, move |owner, cx| {
                owner.state.pinned_views = pinned_views;
                owner.state.pinned_tables = pinned_tables;
                owner.state.views_with_pin_info = views_with_pin_info;
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    pub fn unpin_view(&mut self, view_id: u32, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = services.repository.collection.unpin_view(view_id).await {
                log!(e, "failed to unpin view");
                return;
            }

            let pinned_views = match services.repository.collection.get_pinned_views().await {
                Ok(views) => views,
                Err(e) => {
                    log!(e, "failed to fetch pinned views");
                    vec![]
                }
            };

            let pinned_tables = match services.repository.collection.get_pinned_view_data().await {
                Ok(tables) => tables,
                Err(e) => {
                    log!(e, "failed to fetch pinned table data");
                    vec![]
                }
            };

            let views_with_pin_info = match services.repository.collection.get_views_with_pin_info().await {
                Ok(info) => info,
                Err(e) => {
                    log!(e, "failed to fetch views with pin info");
                    vec![]
                }
            };

            this.update(cx, move |owner, cx| {
                owner.state.pinned_views = pinned_views;
                owner.state.pinned_tables = pinned_tables;
                owner.state.views_with_pin_info = views_with_pin_info;
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    pub fn update_view_position(&mut self, view_id: u32, position_x: f64, position_y: f64, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |_this, _cx| {
            if let Err(e) = services
                .repository
                .collection
                .update_view_position(view_id, position_x, position_y)
                .await
            {
                log!(e, "failed to update view position");
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

        // Create table entities for pinned views
        self.pinned_table_entities = self
            .state
            .pinned_views
            .iter()
            .zip(self.state.pinned_tables.iter())
            .map(|(pinned_view, (table_name, table))| {
                let table_state = cx.new(|cx| {
                    TableState::new(GenericTableDelegate::new(table.clone()), window, cx)
                        .col_resizable(true)
                        .col_movable(true)
                        .sortable(true)
                        .col_selectable(true)
                        .row_selectable(true)
                });
                (pinned_view.view_id, table_name.clone(), table_state)
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
            // Add pinned views with higher z-index
            .children(
                self.pinned_table_entities
                    .iter()
                    .map(|(view_id, name, entity)| {
                        use super::themes::catppuccin_mocha::{yellow, base, red, maroon};
                        
                        // Find the view to get position
                        let pinned_view = self.state.pinned_views.iter().find(|v| v.view_id == *view_id);
                        let position_x = pinned_view.map(|v| v.position_x).unwrap_or(DEFAULT_PIN_POSITION_X);
                        let position_y = pinned_view.map(|v| v.position_y).unwrap_or(DEFAULT_PIN_POSITION_Y);
                        let view_id_for_close = *view_id;
                        let weak = cx.weak_entity();
                        
                        div()
                            .absolute()
                            .left(px(position_x as f32))
                            .top(px(position_y as f32))
                            .bg(mantle())
                            .border_2()
                            .border_color(yellow()) // Yellow border for pinned views
                            .rounded_lg()
                            .shadow_lg()
                            .min_w(px(300.0))
                            .max_w(px(600.0))
                            .min_h(px(200.0))
                            .max_h(px(400.0))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            // Title bar
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .bg(yellow())
                                    .text_color(base())
                                    .p_2()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .items_center()
                                            .child("📌")
                                            .child(div().text_sm().font_weight(FontWeight::BOLD).child(name.clone()))
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_sm()
                                            .bg(red())
                                            .hover(|s| s.bg(maroon()))
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .child("✕")
                                            .on_mouse_up(MouseButton::Left, move |_evt, _win, cx| {
                                                if let Some(ws) = weak.upgrade() {
                                                    ws.update(cx, |ws, cx| {
                                                        ws.unpin_view(view_id_for_close, cx);
                                                    });
                                                }
                                            })
                                    )
                            )
                            // Table content
                            .child(
                                div()
                                    .p_2()
                                    .overflow_hidden()
                                    .flex_1()
                                    .child(entity.clone())
                            )
                    }),
            )
    }
}

use crate::{
    components::{
        command_notifications::CommandNotifications, command_watcher::CommandWatcher,
        operation::Operation,
    },
    themes::catppuccin_macchiato::{crust, green, surface0},
};
use std::collections::HashMap;

use super::{
    components::{
        collection::CollectionList,
        modal_frame::{
            ModalConstraints, ModalFrameDrag, ModalInteraction, ModalRect, ResizeEdges, apply_drag,
            begin_drag_with_interaction,
        },
        table::CustomTable,
    },
    themes::catppuccin_mocha::mantle,
};
use application::operation::operation_execute;
use domain::{clean::collection::Collection, dirty::gpui::State};
use gpui::*;
use gpui_component::scroll::ScrollableElement;
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
    pub table_entities: Vec<(String, Entity<CustomTable>)>,
    pub pinned_table_entities: Vec<(u32, String, Entity<CustomTable>)>,
    pub command_watcher: Entity<CommandWatcher>,
    pub command_notifications: Entity<CommandNotifications>,
    pub operation: Entity<Operation>,
    tables_need_recreation: bool,
    pinned_sizes: HashMap<u32, (f32, f32)>,
    pinned_drag: Option<(u32, ModalFrameDrag)>,
}

impl Workspace {
    fn new(
        cx: &mut Context<Self>,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(String, Entity<CustomTable>)>,
    ) -> Self {
        let weak = cx.weak_entity();
        let collection_list = cx.new(|_| {
            CollectionList::new(
                state.collections.clone(),
                state.views_with_pin_info.clone(),
                weak.clone(),
            )
        });
        let focus_handle = cx.focus_handle();
        let operation = cx.new(|_| Operation::new(weak.clone(), focus_handle.clone()));
        let command_watcher = cx.new(CommandWatcher::new);
        let command_notifications = cx.new(|cx| CommandNotifications::new(services.clone(), cx));

        let mut workspace = Self {
            state,
            services,
            collection_list,
            table_entities,
            pinned_table_entities: vec![],
            command_watcher,
            command_notifications,
            operation,
            tables_need_recreation: false,
            pinned_sizes: HashMap::new(),
            pinned_drag: None,
        };

        // If state has tables/pinned tables, we need to create entities for them
        let has_data =
            !workspace.state.tables.is_empty() || !workspace.state.pinned_tables.is_empty();
        if has_data {
            workspace.tables_need_recreation = true;
        }

        workspace
    }

    // Helper method to recreate table entities when data changes
    fn recreate_table_entities(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.table_entities = self
            .state
            .tables
            .iter()
            .cloned()
            .map(|(name, table)| {
                let services = self.services.clone();
                let table_entity =
                    cx.new(|app_cx| CustomTable::new(table, name.clone(), services, app_cx));
                (name, table_entity)
            })
            .collect();

        // Create table entities for pinned views
        self.pinned_table_entities = self
            .state
            .pinned_views
            .iter()
            .zip(self.state.pinned_tables.iter())
            .map(|(pinned_view, (table_name, table))| {
                let services = self.services.clone();
                let table_entity = cx.new(|app_cx| {
                    CustomTable::new(table.clone(), table_name.clone(), services, app_cx)
                });
                (pinned_view.view_id, table_name.clone(), table_entity)
            })
            .collect();
        self.pinned_sizes = self
            .state
            .pinned_views
            .iter()
            .map(|v| (v.view_id, (v.width as f32, v.height as f32)))
            .collect();
    }

    pub fn view(
        cx: &mut App,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(String, Entity<CustomTable>)>,
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

            let (tables, special_views) =
                match services.repository.collection.get_active_view_data().await {
                    Ok((tables, special_views)) => (tables, special_views),
                    Err(e) => {
                        log!(e, "failed to fetch table data");
                        (vec![], vec![])
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

            let views_with_pin_info = match services
                .repository
                .collection
                .get_views_with_pin_info()
                .await
            {
                Ok(info) => info,
                Err(e) => {
                    log!(e, "failed to fetch views with pin info");
                    vec![]
                }
            };

            this.update(cx, move |owner, cx| {
                owner.state.collections = rows.clone();
                owner.state.tables = tables.clone();
                owner.state.special_views = special_views;
                owner.state.pinned_views = pinned_views;
                owner.state.pinned_tables = pinned_tables;
                owner.state.views_with_pin_info = views_with_pin_info.clone();

                owner.collection_list.update(cx, move |bar, _| {
                    bar.collections = rows.clone();
                    bar.views_with_pin_info = views_with_pin_info;
                });

                // Mark tables for recreation since data changed
                owner.tables_need_recreation = true;
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
                    let (tables, special_views) =
                        match services.repository.collection.get_active_view_data().await {
                            Ok((tables, special_views)) => (tables, special_views),
                            Err(e) => {
                                log!(e, "failed to fetch table data");
                                (vec![], vec![])
                            }
                        };

                    let pinned_tables =
                        match services.repository.collection.get_pinned_view_data().await {
                            Ok(tables) => tables,
                            Err(e) => {
                                log!(e, "failed to fetch pinned table data");
                                vec![]
                            }
                        };

                    let views_with_pin_info = match services
                        .repository
                        .collection
                        .get_views_with_pin_info()
                        .await
                    {
                        Ok(info) => info,
                        Err(e) => {
                            log!(e, "failed to fetch views with pin info");
                            vec![]
                        }
                    };

                    this.update(cx, move |owner, cx| {
                        owner.state.collections = collections.clone();
                        owner.state.tables = tables.clone();
                        owner.state.special_views = special_views;
                        owner.state.pinned_tables = pinned_tables;
                        owner.state.views_with_pin_info = views_with_pin_info.clone();

                        owner.collection_list.update(cx, |bar, _| {
                            bar.collections = collections.clone();
                            bar.views_with_pin_info = views_with_pin_info;
                        });

                        // Mark tables for recreation since data changed
                        owner.tables_need_recreation = true;
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
                    let (tables, special_views) =
                        match services.repository.collection.get_active_view_data().await {
                            Ok((tables, special_views)) => (tables, special_views),
                            Err(e) => {
                                log!(e, "failed to fetch table data");
                                (vec![], vec![])
                            }
                        };

                    let pinned_tables =
                        match services.repository.collection.get_pinned_view_data().await {
                            Ok(tables) => tables,
                            Err(e) => {
                                log!(e, "failed to fetch pinned table data");
                                vec![]
                            }
                        };

                    let views_with_pin_info = match services
                        .repository
                        .collection
                        .get_views_with_pin_info()
                        .await
                    {
                        Ok(info) => info,
                        Err(e) => {
                            log!(e, "failed to fetch views with pin info");
                            vec![]
                        }
                    };

                    this.update(cx, move |owner, cx| {
                        owner.state.tables = tables.clone();
                        owner.state.special_views = special_views;
                        owner.state.pinned_tables = pinned_tables;
                        owner.state.views_with_pin_info = views_with_pin_info;

                        // Mark tables for recreation since data changed
                        owner.tables_need_recreation = true;
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

    pub fn pin_view(
        &mut self,
        view_id: u32,
        position_x: f64,
        position_y: f64,
        cx: &mut Context<Self>,
    ) {
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

            let views_with_pin_info = match services
                .repository
                .collection
                .get_views_with_pin_info()
                .await
            {
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

            let views_with_pin_info = match services
                .repository
                .collection
                .get_views_with_pin_info()
                .await
            {
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

    pub fn update_view_position(
        &mut self,
        view_id: u32,
        position_x: f64,
        position_y: f64,
        cx: &mut Context<Self>,
    ) {
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

    pub fn update_view_size(
        &mut self,
        view_id: u32,
        width: f64,
        height: f64,
        cx: &mut Context<Self>,
    ) {
        let services = self.services.clone();
        cx.spawn(async move |_this, _cx| {
            if let Err(e) = services
                .repository
                .collection
                .update_view_size(view_id, width, height)
                .await
            {
                log!(e, "failed to update view size");
            }
        })
        .detach();
    }

    fn begin_pinned_drag(
        &mut self,
        view_id: u32,
        event: &MouseDownEvent,
        interaction: ModalInteraction,
        position_x: f32,
        position_y: f32,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) {
        let rect = ModalRect {
            x: position_x,
            y: position_y,
            width,
            height,
        };
        let x = f32::from(event.position.x);
        let y = f32::from(event.position.y);
        let drag = begin_drag_with_interaction(rect, x, y, interaction);
        self.pinned_drag = Some((view_id, drag));
        cx.notify();
    }

    fn update_pinned_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let Some((view_id, drag)) = self.pinned_drag else {
            return;
        };
        let rect = apply_drag(
            drag,
            f32::from(event.position.x),
            f32::from(event.position.y),
            ModalConstraints {
                min_width: 300.0,
                min_height: 220.0,
                max_width: 1800.0,
                max_height: 1400.0,
            },
        );
        if let Some(v) = self
            .state
            .pinned_views
            .iter_mut()
            .find(|v| v.view_id == view_id)
        {
            v.position_x = rect.x as f64;
            v.position_y = rect.y as f64;
            v.width = rect.width as f64;
            v.height = rect.height as f64;
        }
        self.pinned_sizes.insert(view_id, (rect.width, rect.height));
        cx.notify();
    }

    fn end_pinned_drag(&mut self, cx: &mut Context<Self>) {
        let Some((view_id, _)) = self.pinned_drag.take() else {
            return;
        };
        if let Some(v) = self
            .state
            .pinned_views
            .iter()
            .find(|v| v.view_id == view_id)
            .cloned()
        {
            self.update_view_position(view_id, v.position_x, v.position_y, cx);
            self.update_view_size(view_id, v.width, v.height, cx);
        }
        cx.notify();
    }

    fn has_active_special_view(&self, query: &str) -> bool {
        self.state
            .special_views
            .iter()
            .any(|active| active == query)
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.tables_need_recreation {
            self.recreate_table_entities(window, cx);
            self.tables_need_recreation = false;
        }

        let mode_label = self
            .operation
            .read(cx)
            .editing_mode_widget_label(window)
            .or_else(|| {
                self.table_entities
                    .iter()
                    .find_map(|(_, entity)| entity.read(cx).editing_mode_widget_label(window))
            })
            .or_else(|| {
                self.pinned_table_entities
                    .iter()
                    .find_map(|(_, _, entity)| entity.read(cx).editing_mode_widget_label(window))
            })
            .unwrap_or("Normal");

        let bar = div()
            .h(rems(1.6))
            .bg(surface0())
            .relative()
            .flex()
            .items_center()
            .justify_start()
            .px_2()
            .gap_3()
            .m_0()
            .text_sm()
            .font_weight(FontWeight::BOLD)
            .child(div().w_full().h_full().child(self.operation.clone()))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .items_center()
                    .h(rems(1.2))
                    .w(rems(3.4))
                    .rounded_xs()
                    .bg(green())
                    .text_color(crust())
                    .child(mode_label),
            );

        let main = div()
            .flex()
            .flex_col()
            .gap_4()
            .w_full()
            .p_3()
            .child(self.collection_list.clone());
        let main = if self.has_active_special_view("command_buffer") {
            main.child(self.command_watcher.clone())
        } else {
            main
        }
        .children(
            self.table_entities
                .iter()
                .map(|(_name, entity)| entity.clone()),
        )
        .children(
            self.pinned_table_entities
                .iter()
                .map(|(view_id, name, entity)| {
                    use super::themes::catppuccin_mocha::{base, maroon, red, yellow};

                    let pinned_view = self
                        .state
                        .pinned_views
                        .iter()
                        .find(|v| v.view_id == *view_id);
                    let position_x = pinned_view
                        .map(|v| v.position_x)
                        .unwrap_or(DEFAULT_PIN_POSITION_X);
                    let position_y = pinned_view
                        .map(|v| v.position_y)
                        .unwrap_or(DEFAULT_PIN_POSITION_Y);
                    let view_id_for_close = *view_id;
                    let view_id_for_drag = *view_id;
                    let position_x_f32 = position_x as f32;
                    let position_y_f32 = position_y as f32;
                    let weak = cx.weak_entity();

                    let (width, height) = self
                        .pinned_sizes
                        .get(view_id)
                        .copied()
                        .unwrap_or((500.0, 400.0));
                    div()
                        .absolute()
                        .left(px(position_x as f32))
                        .top(px(position_y as f32))
                        .bg(mantle())
                        .border_1()
                        .border_color(yellow()) // Yellow border for pinned views
                        .rounded_lg()
                        .shadow_lg()
                        .w(px(width))
                        .h(px(height))
                        .relative()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                            this.update_pinned_drag(event, cx);
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                                this.end_pinned_drag(cx);
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .w(px(10.0))
                                .h_full()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: true,
                                                    right: false,
                                                    top: false,
                                                    bottom: false,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .right(px(0.0))
                                .top(px(0.0))
                                .w(px(10.0))
                                .h_full()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: true,
                                                    top: false,
                                                    bottom: false,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .w_full()
                                .h(px(10.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: false,
                                                    top: true,
                                                    bottom: false,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .bottom(px(0.0))
                                .w_full()
                                .h(px(10.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: false,
                                                    top: false,
                                                    bottom: true,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .w(px(14.0))
                                .h(px(14.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: true,
                                                    right: false,
                                                    top: true,
                                                    bottom: false,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .right(px(0.0))
                                .top(px(0.0))
                                .w(px(14.0))
                                .h(px(14.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: true,
                                                    top: true,
                                                    bottom: false,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .bottom(px(0.0))
                                .w(px(14.0))
                                .h(px(14.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: true,
                                                    right: false,
                                                    top: false,
                                                    bottom: true,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .right(px(0.0))
                                .bottom(px(0.0))
                                .w(px(14.0))
                                .h(px(14.0))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: true,
                                                    top: false,
                                                    bottom: true,
                                                }),
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .bg(yellow())
                                .border_b_2()
                                .border_color(yellow())
                                .text_color(base())
                                .p_2()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_pinned_drag(
                                                view_id_for_drag,
                                                event,
                                                ModalInteraction::Move,
                                                position_x_f32,
                                                position_y_f32,
                                                width,
                                                height,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .items_center()
                                        .child("📌")
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::BOLD)
                                                .child(name.clone()),
                                        ),
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
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h(px(0.0))
                                .overflow_y_scrollbar()
                                .child(entity.clone()),
                        )
                }),
        );

        let scrollable_main = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_y_scrollbar()
            .child(main);

        div()
            .bg(mantle())
            .text_color(rgb(0xffffff))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .child(scrollable_main)
            .child(bar)
            .child(self.command_notifications.clone())
    }
}

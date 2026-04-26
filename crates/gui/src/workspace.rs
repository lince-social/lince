use crate::{
    components::{
        command_notifications::CommandNotifications, command_watcher::CommandWatcher,
        creation_modal::CreationModal, operation::Operation,
    },
    keybinding_mode::{Mode, global_mode_is_vim, set_global_mode},
    themes::catppuccin_macchiato::{crust, green, surface0},
};
use std::{collections::HashMap, str::FromStr};

use super::{
    components::{collection::CollectionList, table::CustomTable},
    themes::catppuccin_macchiato::mantle,
};
use application::operation::operation_execute;
use domain::{
    clean::collection::Collection,
    dirty::{
        gpui::State,
        operation::{DatabaseTable, OperationActions},
    },
};
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use injection::cross_cutting::InjectedServices;
use utils::log;

pub struct Workspace {
    // pub focus_handle: FocusHandle,
    pub state: State,
    pub services: InjectedServices,
    pub collection_list: Entity<CollectionList>,
    pub table_entities: Vec<(u32, String, Entity<CustomTable>)>,
    pub command_watcher: Entity<CommandWatcher>,
    pub command_notifications: Entity<CommandNotifications>,
    pub operation: Entity<Operation>,
    pub creation_modal: Option<Entity<CreationModal>>,
    pub creation_view_entities: Vec<(String, Entity<CreationModal>)>,
    tables_need_recreation: bool,
    main_scroll_handle: ScrollHandle,
}

impl Workspace {
    fn refresh_global_keybinding_mode(&self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(configuration) = services.repository.configuration.get_active().await {
                let mode = Mode::from_db(configuration.keybinding_mode);
                let _ = this.update(cx, |workspace, cx| {
                    set_global_mode(cx, mode);
                    workspace.operation.update(cx, |operation, _| {
                        operation.schedule_refocus();
                    });
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn new(
        cx: &mut Context<Self>,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(u32, String, Entity<CustomTable>)>,
    ) -> Self {
        let weak = cx.weak_entity();
        let collection_list =
            cx.new(|_| CollectionList::new(state.collections.clone(), weak.clone()));
        let focus_handle = cx.focus_handle();
        let operation = cx.new(|_| Operation::new(weak.clone(), focus_handle.clone()));
        operation.update(cx, |operation, _| {
            operation.schedule_refocus();
        });
        let command_watcher = cx.new(CommandWatcher::new);
        let command_notifications = cx.new(|cx| CommandNotifications::new(services.clone(), cx));

        let mut workspace = Self {
            state,
            services,
            collection_list,
            table_entities,
            command_watcher,
            command_notifications,
            operation,
            creation_modal: None,
            creation_view_entities: vec![],
            tables_need_recreation: false,
            main_scroll_handle: ScrollHandle::new(),
        };

        // If state has tables/pinned tables, we need to create entities for them
        let has_data = !workspace.state.tables.is_empty();
        if has_data {
            workspace.tables_need_recreation = true;
        }
        workspace.refresh_creation_view_entities(cx);
        workspace.refresh_global_keybinding_mode(cx);

        workspace
    }

    fn clamp_main_scroll_offset(&self, y: Pixels) -> Pixels {
        let max_offset = self.main_scroll_handle.max_offset().height;
        y.max(-max_offset).min(px(0.0))
    }

    fn scroll_main_to(&self, y: Pixels) {
        let mut offset = self.main_scroll_handle.offset();
        offset.y = self.clamp_main_scroll_offset(y);
        self.main_scroll_handle.set_offset(offset);
    }

    fn scroll_main_by(&self, delta: Pixels) {
        let offset = self.main_scroll_handle.offset();
        self.scroll_main_to(offset.y + delta);
    }

    fn handle_main_scroll_keybinding(
        &self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_step = px(48.0);
        let page_step = self.main_scroll_handle.bounds().size.height * 0.9;
        let handled = match event.keystroke.key.as_str() {
            "up" | "arrowup" => {
                self.scroll_main_by(line_step);
                true
            }
            "down" | "arrowdown" => {
                self.scroll_main_by(-line_step);
                true
            }
            "pageup" => {
                self.scroll_main_by(page_step);
                true
            }
            "pagedown" => {
                self.scroll_main_by(-page_step);
                true
            }
            "home" => {
                self.scroll_main_to(px(0.0));
                true
            }
            "end" => {
                self.scroll_main_to(-self.main_scroll_handle.max_offset().height);
                true
            }
            _ => false,
        };

        if handled {
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        }
    }

    // Helper method to recreate table entities when data changes
    fn recreate_table_entities(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.table_entities = self
            .state
            .tables
            .iter()
            .cloned()
            .map(|(collection_view_id, name, table)| {
                let services = self.services.clone();
                let table_entity = cx.new(|app_cx| {
                    CustomTable::new(
                        table,
                        name.clone(),
                        Some(collection_view_id),
                        services,
                        app_cx,
                    )
                });
                (collection_view_id, name, table_entity)
            })
            .collect();
    }

    pub fn view(
        cx: &mut App,
        services: InjectedServices,
        state: State,
        table_entities: Vec<(u32, String, Entity<CustomTable>)>,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(cx, services, state, table_entities))
    }
    pub fn on_collection_selected(&mut self, collection_id: u32, cx: &mut Context<Self>) {
        self.operation.update(cx, |operation, _| {
            operation.schedule_refocus();
        });

        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = application::write::set_active_collection(
                services.clone(),
                &collection_id.to_string(),
            )
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

            this.update(cx, move |owner, cx| {
                owner.state.collections = rows.clone();
                owner.state.tables = tables.clone();
                owner.state.special_views = special_views;

                owner.collection_list.update(cx, move |bar, _| {
                    bar.collections = rows.clone();
                });
                owner.operation.update(cx, |operation, _| {
                    operation.schedule_refocus();
                });

                // Mark tables for recreation since data changed
                owner.tables_need_recreation = true;
                owner.refresh_creation_view_entities(cx);
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }
    pub fn on_view_selected(&mut self, cx: &mut Context<Self>, collection_id: u32, view_id: u32) {
        self.operation.update(cx, |operation, _| {
            operation.schedule_refocus();
        });

        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            match application::write::toggle_view(services.clone(), collection_id, view_id).await {
                Ok(()) => {
                    let collections = match services.repository.collection.get_all().await {
                        Ok(rows) => rows,
                        Err(e) => {
                            log!(e, "failed to fetch collections");
                            vec![]
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

                    this.update(cx, move |owner, cx| {
                        owner.state.collections = collections.clone();
                        owner.state.tables = tables.clone();
                        owner.state.special_views = special_views;

                        owner.collection_list.update(cx, |bar, _| {
                            bar.collections = collections.clone();
                        });
                        owner.operation.update(cx, |operation, _| {
                            operation.schedule_refocus();
                        });

                        // Mark tables for recreation since data changed
                        owner.tables_need_recreation = true;
                        owner.refresh_creation_view_entities(cx);
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
    pub fn close_creation_modal(&mut self, cx: &mut Context<Self>) {
        self.creation_modal = None;
        cx.notify();
    }

    pub fn open_creation_modal(&mut self, table: DatabaseTable, cx: &mut Context<Self>) {
        self.open_creation_modal_with_initial_values(table, HashMap::new(), cx);
    }

    pub fn open_collection_view_creation_modal(
        &mut self,
        collection_id: u32,
        cx: &mut Context<Self>,
    ) {
        let mut initial_values = HashMap::new();
        initial_values.insert("collection_id".to_string(), collection_id.to_string());
        self.open_creation_modal_with_initial_values(
            DatabaseTable::CollectionView,
            initial_values,
            cx,
        );
    }

    fn open_creation_modal_with_initial_values(
        &mut self,
        table: DatabaseTable,
        initial_values: HashMap<String, String>,
        cx: &mut Context<Self>,
    ) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let columns = match services
                .repository
                .table
                .get_columns(table.as_table_name().to_string())
                .await
            {
                Ok(columns) => columns,
                Err(e) => {
                    log!(e, "failed to fetch table columns");
                    vec![]
                }
            };

            this.update(cx, move |owner, cx| {
                let fields = columns
                    .into_iter()
                    .filter(|column| column.to_lowercase() != "id")
                    .collect::<Vec<_>>();
                let weak = cx.weak_entity();
                let modal_services = owner.services.clone();
                owner.creation_modal = Some(cx.new(|app_cx| {
                    CreationModal::new(
                        weak,
                        modal_services,
                        table,
                        fields,
                        initial_values.clone(),
                        app_cx,
                    )
                }));
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    pub fn create_row_from_modal(
        &mut self,
        table: DatabaseTable,
        values: HashMap<String, String>,
        source: Option<WeakEntity<CreationModal>>,
        cx: &mut Context<Self>,
    ) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = application::write::table_insert_row(
                services.clone(),
                table.as_table_name().to_string(),
                values,
            )
            .await
            {
                log!(e, "failed to create row from modal");
                return;
            }

            let (tables, special_views) =
                match services.repository.collection.get_active_view_data().await {
                    Ok((tables, special_views)) => (tables, special_views),
                    Err(e) => {
                        log!(e, "failed to fetch table data");
                        (vec![], vec![])
                    }
                };

            this.update(cx, move |owner, cx| {
                owner.state.tables = tables.clone();
                owner.state.special_views = special_views;
                owner.tables_need_recreation = true;
                owner.refresh_creation_view_entities(cx);
                if let Some(source) = source.as_ref() {
                    let _ = source.update(cx, |modal, cx| {
                        modal.clear_inputs(cx);
                    });
                }
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    pub fn send_operation(&mut self, cx: &mut Context<Self>, operation: String) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            match operation_execute(services.clone(), operation.clone()).await {
                Ok(operationresult) => {
                    let (tables, special_views) =
                        match services.repository.collection.get_active_view_data().await {
                            Ok((tables, special_views)) => (tables, special_views),
                            Err(e) => {
                                log!(e, "failed to fetch table data");
                                (vec![], vec![])
                            }
                        };

                    this.update(cx, move |owner, cx| {
                        owner.state.tables = tables.clone();
                        owner.state.special_views = special_views;
                        owner.tables_need_recreation = true;
                        owner.refresh_creation_view_entities(cx);
                        owner.refresh_global_keybinding_mode(cx);
                        for instruction in operationresult {
                            if instruction.action == OperationActions::Create
                                && let Some(table) = instruction.table
                            {
                                owner.open_creation_modal(table, cx);
                            }
                        }
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

    fn has_active_special_view(&self, query: &str) -> bool {
        self.state
            .special_views
            .iter()
            .any(|active| active == query)
    }

    fn parse_creation_view_query(query: &str) -> Option<DatabaseTable> {
        let normalized = query.trim().to_lowercase().replace(['-', ' '], "_");
        let table_name = normalized
            .strip_prefix("create_view_")
            .or_else(|| normalized.strip_prefix("creation_view_"))
            .or_else(|| normalized.strip_prefix("create_modal_"))
            .or_else(|| normalized.strip_prefix("creation_modal_"))
            .or_else(|| normalized.strip_prefix("cv_"))?;
        DatabaseTable::from_str(table_name).ok()
    }

    fn refresh_creation_view_entities(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let active_creation_views = self
            .state
            .special_views
            .iter()
            .filter_map(|query| {
                Self::parse_creation_view_query(query).map(|table| (query.clone(), table))
            })
            .collect::<Vec<_>>();
        cx.spawn(async move |this, cx| {
            let mut active_definitions = Vec::new();
            for (query, table) in active_creation_views {
                let columns = match services
                    .repository
                    .table
                    .get_columns(table.as_table_name().to_string())
                    .await
                {
                    Ok(columns) => columns,
                    Err(e) => {
                        log!(e, "failed to fetch table columns");
                        vec![]
                    }
                };
                let fields = columns
                    .into_iter()
                    .filter(|column| column.to_lowercase() != "id")
                    .collect::<Vec<_>>();
                active_definitions.push((query, table, fields));
            }

            this.update(cx, move |owner, cx| {
                let weak = cx.weak_entity();
                owner.creation_view_entities = active_definitions
                    .into_iter()
                    .map(|(query, table, fields)| {
                        let weak = weak.clone();
                        let services = owner.services.clone();
                        (
                            query,
                            cx.new(|app_cx| {
                                CreationModal::new_view(
                                    weak,
                                    services,
                                    table,
                                    fields,
                                    HashMap::new(),
                                    app_cx,
                                )
                            }),
                        )
                    })
                    .collect();
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.tables_need_recreation {
            self.recreate_table_entities(window, cx);
            self.tables_need_recreation = false;
        }

        let mode_label = if global_mode_is_vim(cx) {
            self.operation
                .read(cx)
                .editing_mode_widget_label(window)
                .or_else(|| {
                    self.table_entities.iter().find_map(|(_, _, entity)| {
                        entity.read(cx).editing_mode_widget_label(window)
                    })
                })
                .or(Some("Insert"))
        } else {
            None
        };
        let show_consultation_button = self
            .operation
            .read(cx)
            .show_keybinding_consultation_button(window);

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
            .child(div().w_full().h_full().child(self.operation.clone()));
        let bar = if let Some(mode_label) = mode_label {
            bar.child(
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
            )
        } else {
            bar
        };
        let bar = if show_consultation_button {
            bar.child(
                div()
                    .flex()
                    .justify_center()
                    .items_center()
                    .h(rems(1.2))
                    .px_2()
                    .rounded_xs()
                    .bg(green())
                    .text_color(crust())
                    .text_xs()
                    .child("Keys"),
            )
        } else {
            bar
        };

        let mut active_views = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .gap_4()
            .w_full()
            .children(
                self.table_entities
                    .iter()
                    .map(|(_, _name, entity)| entity.clone()),
            )
            .children(
                self.creation_view_entities
                    .iter()
                    .map(|(_, entity)| entity.clone()),
            );
        if self.has_active_special_view("command_buffer") {
            active_views = active_views.child(self.command_watcher.clone());
        }

        let main = div()
            .flex()
            .flex_col()
            .gap_4()
            .w_full()
            .p_3()
            .child(self.collection_list.clone())
            .child(active_views);
        let _ = window;

        let scrollable_main = div()
            .id("main-scroll-area")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .track_scroll(&self.main_scroll_handle)
            .overflow_y_scroll()
            .vertical_scrollbar(&self.main_scroll_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_main_scroll_keybinding(event, window, cx);
            }))
            .child(main);

        let creation_modal_overlay = self.creation_modal.as_ref().map(|entity| {
            div()
                .absolute()
                .inset_0()
                .bg(rgba(0x00000099))
                .flex()
                .items_center()
                .justify_center()
                .child(entity.clone())
                .into_any_element()
        });

        div()
            .bg(mantle())
            .text_color(rgb(0xffffff))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .child(scrollable_main)
            .child(bar)
            .children(creation_modal_overlay)
            .child(self.command_notifications.clone())
    }
}

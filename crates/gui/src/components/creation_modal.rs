use std::collections::{HashMap, HashSet};

use domain::{clean::table::Table, dirty::operation::DatabaseTable};
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, px,
};
use gpui_component::scroll::ScrollableElement;
use injection::cross_cutting::InjectedServices;

use crate::{
    themes::catppuccin_macchiato::{
        blue, crust, green, mantle, red, surface0, surface1, text, yellow,
    },
    workspace::Workspace,
};

#[derive(Clone)]
struct AutocompleteItem {
    row: HashMap<String, String>,
    active_output_column: String,
    label: String,
}

pub struct CreationModal {
    workspace: WeakEntity<Workspace>,
    services: InjectedServices,
    table: DatabaseTable,
    table_name: String,
    modal: bool,
    columns: Vec<String>,
    values: Vec<String>,
    cursors: Vec<usize>,
    active_field_ix: usize,
    focus_handle: FocusHandle,
    has_focused: bool,
    autocomplete_rows_by_table: HashMap<String, Table>,
    autocomplete_loading_tables: HashSet<String>,
    autocomplete_items: Vec<AutocompleteItem>,
    autocomplete_selected_ix: Option<usize>,
}

impl Focusable for CreationModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl CreationModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        services: InjectedServices,
        table: DatabaseTable,
        columns: Vec<String>,
        initial_values: HashMap<String, String>,
        cx: &mut App,
    ) -> Self {
        Self::new_with_mode(
            workspace,
            services,
            table,
            columns,
            initial_values,
            true,
            cx,
        )
    }

    pub fn new_view(
        workspace: WeakEntity<Workspace>,
        services: InjectedServices,
        table: DatabaseTable,
        columns: Vec<String>,
        initial_values: HashMap<String, String>,
        cx: &mut App,
    ) -> Self {
        Self::new_with_mode(
            workspace,
            services,
            table,
            columns,
            initial_values,
            false,
            cx,
        )
    }

    fn new_with_mode(
        workspace: WeakEntity<Workspace>,
        services: InjectedServices,
        table: DatabaseTable,
        columns: Vec<String>,
        initial_values: HashMap<String, String>,
        modal: bool,
        cx: &mut App,
    ) -> Self {
        let mut values = vec![String::new(); columns.len()];
        let mut cursors = vec![0; columns.len()];
        for (ix, column) in columns.iter().enumerate() {
            if let Some(initial) = initial_values.get(column) {
                values[ix] = initial.clone();
                cursors[ix] = initial.chars().count();
            }
        }
        Self {
            workspace,
            services,
            table,
            table_name: table.as_table_name().to_string(),
            modal,
            columns,
            values,
            cursors,
            active_field_ix: 0,
            focus_handle: cx.focus_handle(),
            has_focused: false,
            autocomplete_rows_by_table: HashMap::new(),
            autocomplete_loading_tables: HashSet::new(),
            autocomplete_items: vec![],
            autocomplete_selected_ix: None,
        }
    }

    fn active_value_and_cursor_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        if self.active_field_ix >= self.values.len() || self.active_field_ix >= self.cursors.len() {
            return None;
        }
        let value = self.values.get_mut(self.active_field_ix)?;
        let cursor = self.cursors.get_mut(self.active_field_ix)?;
        Some((value, cursor))
    }

    fn move_active_field(&mut self, delta: isize) {
        if self.columns.is_empty() {
            self.active_field_ix = 0;
            return;
        }
        let len = self.columns.len() as isize;
        let next = (self.active_field_ix as isize + delta).clamp(0, len - 1);
        self.active_field_ix = next as usize;
    }

    fn insert_char(&mut self, ch: char) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        let current = (*cursor).min(value.chars().count());
        let byte_ix = value
            .char_indices()
            .nth(current)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        value.insert(byte_ix, ch);
        *cursor += 1;
    }

    fn delete_backward(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        if *cursor == 0 {
            return;
        }
        let start_char = *cursor - 1;
        let start = value
            .char_indices()
            .nth(start_char)
            .map(|(ix, _)| ix)
            .unwrap_or(0);
        let end = value
            .char_indices()
            .nth(*cursor)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        value.replace_range(start..end, "");
        *cursor = start_char;
    }

    fn delete_forward(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        let len = value.chars().count();
        if *cursor >= len {
            return;
        }
        let start = value
            .char_indices()
            .nth(*cursor)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        let end = value
            .char_indices()
            .nth(*cursor + 1)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        value.replace_range(start..end, "");
    }

    fn move_left(&mut self) {
        let Some((_, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = (*cursor).saturating_sub(1);
    }

    fn move_right(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = (*cursor + 1).min(value.chars().count());
    }

    fn move_home(&mut self) {
        let Some((_, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = 0;
    }

    fn move_end(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = value.chars().count();
    }

    fn display_field_value(&self, field_ix: usize) -> String {
        let Some(value) = self.values.get(field_ix) else {
            return String::new();
        };
        if field_ix != self.active_field_ix {
            return value.clone();
        }
        let cursor = self
            .cursors
            .get(field_ix)
            .copied()
            .unwrap_or(0)
            .min(value.chars().count());
        let before = value.chars().take(cursor).collect::<String>();
        let after = value.chars().skip(cursor).collect::<String>();
        format!("{before}|{after}")
    }

    fn close_modal(&self, cx: &mut Context<Self>) {
        if !self.modal {
            return;
        }
        let weak = self.workspace.clone();
        let _ = weak.update(cx, |ws, cx| {
            ws.close_creation_modal(cx);
        });
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let mut values = HashMap::new();
        for (column, value) in self.columns.iter().zip(self.values.iter()) {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                values.insert(column.clone(), trimmed);
            }
        }
        let table = self.table;
        let weak = self.workspace.clone();
        let source = cx.weak_entity();
        let _ = weak.update(cx, |ws, cx| {
            ws.create_row_from_modal(table, values, Some(source), cx);
        });
    }

    pub fn clear_inputs(&mut self, cx: &mut Context<Self>) {
        self.values.iter_mut().for_each(String::clear);
        self.cursors.iter_mut().for_each(|cursor| *cursor = 0);
        self.active_field_ix = 0;
        self.autocomplete_items.clear();
        self.autocomplete_selected_ix = None;
        cx.notify();
    }

    fn all_fields_have_values(&self) -> bool {
        if self.values.is_empty() {
            return false;
        }
        self.values.iter().all(|value| !value.trim().is_empty())
    }

    fn autocomplete_spec_for_field(&self, field_ix: usize) -> Option<(String, String)> {
        let field = self.columns.get(field_ix)?.to_lowercase();
        match (self.table, field.as_str()) {
            (DatabaseTable::Karma, "condition_id") => {
                Some(("karma_condition".to_string(), "id".to_string()))
            }
            (DatabaseTable::Karma, "consequence_id") => {
                Some(("karma_consequence".to_string(), "id".to_string()))
            }
            (DatabaseTable::CollectionView, "collection_id") => {
                Some(("collection".to_string(), "id".to_string()))
            }
            (DatabaseTable::CollectionView, "view_id") => {
                Some(("view".to_string(), "id".to_string()))
            }
            _ => Some((self.table_name.clone(), field)),
        }
    }

    fn ensure_autocomplete_rows_loaded(&mut self, source_table: &str, cx: &mut Context<Self>) {
        if self.autocomplete_rows_by_table.contains_key(source_table)
            || self.autocomplete_loading_tables.contains(source_table)
        {
            return;
        }

        let source_table = source_table.to_string();
        self.autocomplete_loading_tables
            .insert(source_table.clone());
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let query = format!("SELECT * FROM {source_table}");
            let rows = match services
                .repository
                .collection
                .execute_queries(vec![query])
                .await
            {
                Ok(mut tables) => tables.pop().map(|(_, rows)| rows).unwrap_or_default(),
                Err(_) => vec![],
            };

            let _ = this.update(cx, move |modal, cx| {
                modal.autocomplete_loading_tables.remove(&source_table);
                modal
                    .autocomplete_rows_by_table
                    .insert(source_table.clone(), rows);
                modal.recompute_autocomplete(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn row_matches_query(row: &HashMap<String, String>, query: &str) -> bool {
        row.values()
            .any(|value| value.to_lowercase().contains(query))
    }

    fn format_row_label(row: &HashMap<String, String>) -> String {
        let mut pairs = row
            .iter()
            .filter(|(_, value)| !value.trim().is_empty() && value != &&"NULL".to_string())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        if pairs.is_empty() {
            return "(empty row)".to_string();
        }
        pairs
            .into_iter()
            .take(4)
            .map(|(key, value)| format!("{key}: {value}"))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn recompute_autocomplete(&mut self, cx: &mut Context<Self>) {
        self.autocomplete_items.clear();
        self.autocomplete_selected_ix = None;

        let Some((source_table, active_output_column)) =
            self.autocomplete_spec_for_field(self.active_field_ix)
        else {
            return;
        };

        let query = self
            .values
            .get(self.active_field_ix)
            .map(|value| value.trim().to_lowercase())
            .unwrap_or_default();

        if query.is_empty() {
            return;
        }

        if !self.autocomplete_rows_by_table.contains_key(&source_table) {
            self.ensure_autocomplete_rows_loaded(&source_table, cx);
            return;
        }

        if let Some(rows) = self.autocomplete_rows_by_table.get(&source_table) {
            self.autocomplete_items = rows
                .iter()
                .filter(|row| Self::row_matches_query(row, &query))
                .take(25)
                .map(|row| AutocompleteItem {
                    row: row.clone(),
                    active_output_column: active_output_column.clone(),
                    label: Self::format_row_label(row),
                })
                .collect();
        }

        if !self.autocomplete_items.is_empty() {
            self.autocomplete_selected_ix = Some(0);
        }
    }

    fn autocomplete_select_next(&mut self) {
        if self.autocomplete_items.is_empty() {
            self.autocomplete_selected_ix = None;
            return;
        }
        let current = self.autocomplete_selected_ix.unwrap_or(0);
        self.autocomplete_selected_ix = Some((current + 1) % self.autocomplete_items.len());
    }

    fn autocomplete_select_prev(&mut self) {
        if self.autocomplete_items.is_empty() {
            self.autocomplete_selected_ix = None;
            return;
        }
        let current = self.autocomplete_selected_ix.unwrap_or(0);
        self.autocomplete_selected_ix = Some(if current == 0 {
            self.autocomplete_items.len() - 1
        } else {
            current - 1
        });
    }

    fn apply_selected_completion(&mut self) {
        let Some(selected_ix) = self.autocomplete_selected_ix else {
            return;
        };
        let Some(item) = self.autocomplete_items.get(selected_ix).cloned() else {
            return;
        };

        if let Some(active_value) = item.row.get(&item.active_output_column)
            && let Some((value, cursor)) = self.active_value_and_cursor_mut()
        {
            *value = active_value.clone();
            *cursor = value.chars().count();
        }

        for (ix, column) in self.columns.iter().enumerate() {
            if ix == self.active_field_ix {
                continue;
            }
            let current = self.values.get(ix).map(|v| v.trim()).unwrap_or_default();
            if !current.is_empty() {
                continue;
            }
            if let Some(suggested) = item.row.get(column)
                && let Some(value) = self.values.get_mut(ix)
            {
                *value = suggested.clone();
                if let Some(cursor) = self.cursors.get_mut(ix) {
                    *cursor = value.chars().count();
                }
            }
        }

        self.autocomplete_items.clear();
        self.autocomplete_selected_ix = None;
    }

    fn active_source_table(&self) -> Option<String> {
        self.autocomplete_spec_for_field(self.active_field_ix)
            .map(|(table, _)| table)
    }
}

impl Render for CreationModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.has_focused {
            cx.focus_self(window);
            self.has_focused = true;
            self.recompute_autocomplete(cx);
        }
        let viewport = window.viewport_size();
        let viewport_width = f32::from(viewport.width);
        let viewport_height = f32::from(viewport.height);
        let container_width = (viewport_width - 24.0).clamp(280.0, 480.0);
        let container_max_height = (viewport_height - 24.0).max(260.0);
        let body_max_height = (container_max_height - 120.0).max(120.0);

        let title = format!("Create in {}", self.table_name);
        let component_id = if self.modal {
            self.table as u32
        } else {
            self.table as u32 + 1000
        };

        let autocomplete_items = self.autocomplete_items.clone();
        let autocomplete_selected_ix = self.autocomplete_selected_ix;
        let active_loading = self
            .active_source_table()
            .map(|table| self.autocomplete_loading_tables.contains(&table))
            .unwrap_or(false);

        let container = div()
            .id(("creation_component", component_id))
            .w(px(container_width))
            .max_h(px(container_max_height))
            .bg(mantle())
            .border_2()
            .border_color(green())
            .rounded_lg()
            .shadow_lg()
            .overflow_hidden()
            .focusable()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let modifiers = event.keystroke.modifiers;
                let with_save_mod = modifiers.control || modifiers.alt;
                let has_autocomplete = !this.autocomplete_items.is_empty();

                if this.modal && key == "escape" {
                    this.close_modal(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }

                if (with_save_mod && key == "s") || (key == "enter" && modifiers.control) {
                    this.submit(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }

                match key {
                    "tab" => {
                        if modifiers.control || modifiers.shift {
                            this.move_active_field(-1);
                        } else {
                            this.move_active_field(1);
                        }
                        this.recompute_autocomplete(cx);
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "down" | "arrowdown" => {
                        if has_autocomplete {
                            this.autocomplete_select_next();
                        } else {
                            this.move_active_field(1);
                            this.recompute_autocomplete(cx);
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "up" | "arrowup" => {
                        if has_autocomplete {
                            this.autocomplete_select_prev();
                        } else {
                            this.move_active_field(-1);
                            this.recompute_autocomplete(cx);
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "left" | "arrowleft" => {
                        this.move_left();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "right" | "arrowright" => {
                        this.move_right();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "home" => {
                        this.move_home();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "end" => {
                        this.move_end();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "backspace" => {
                        this.delete_backward();
                        this.recompute_autocomplete(cx);
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "delete" => {
                        this.delete_forward();
                        this.recompute_autocomplete(cx);
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "enter" => {
                        if has_autocomplete {
                            this.apply_selected_completion();
                            this.recompute_autocomplete(cx);
                        } else if this.all_fields_have_values() {
                            this.submit(cx);
                        } else {
                            let last_field_ix = this.columns.len().saturating_sub(1);
                            if this.active_field_ix >= last_field_ix {
                                this.submit(cx);
                            } else {
                                this.move_active_field(1);
                                this.recompute_autocomplete(cx);
                            }
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "n" if modifiers.control => {
                        this.autocomplete_select_next();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "p" if modifiers.control => {
                        this.autocomplete_select_prev();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    _ => {
                        if let Some(key_char) = &event.keystroke.key_char
                            && let Some(ch) = key_char.chars().next()
                        {
                            if ch == '\t' {
                                if modifiers.control || modifiers.shift {
                                    this.move_active_field(-1);
                                } else {
                                    this.move_active_field(1);
                                }
                                this.recompute_autocomplete(cx);
                                window.prevent_default();
                                cx.stop_propagation();
                            } else if !modifiers.control && !modifiers.alt && !modifiers.platform {
                                this.insert_char(ch);
                                this.recompute_autocomplete(cx);
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }
                    }
                }

                cx.notify();
            }))
            .child(
                div()
                    .bg(green())
                    .text_color(crust())
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(title))
                    .children(if self.modal {
                        Some(
                            div()
                                .bg(red())
                                .hover(|s| s.bg(yellow()))
                                .text_color(crust())
                                .rounded_sm()
                                .px_2()
                                .py_1()
                                .child("Close")
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.close_modal(cx);
                                    }),
                                )
                                .into_any_element(),
                        )
                    } else {
                        None
                    }),
            )
            .child(
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .max_h(px(body_max_height))
                    .overflow_y_scrollbar()
                    .children(
                        self.columns
                            .iter()
                            .enumerate()
                            .map(|(ix, column)| {
                                let field_ix = ix;
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_xs().text_color(text()).child(column.clone()))
                                    .child(
                                        div()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if self.active_field_ix == ix {
                                                blue()
                                            } else {
                                                surface1()
                                            })
                                            .bg(surface0())
                                            .px_2()
                                            .py_1()
                                            .text_color(text())
                                            .child(self.display_field_value(ix))
                                            .on_mouse_up(
                                                MouseButton::Left,
                                                cx.listener(move |this, _event, window, cx| {
                                                    this.active_field_ix = field_ix;
                                                    this.recompute_autocomplete(cx);
                                                    cx.focus_self(window);
                                                    cx.notify();
                                                }),
                                            ),
                                    )
                                    .children(if self.active_field_ix == ix && !autocomplete_items.is_empty() {
                                        Some(
                                            div()
                                                .mt_1()
                                                .bg(surface0())
                                                .border_1()
                                                .border_color(blue())
                                                .rounded_md()
                                                .max_h(px(140.0))
                                                .overflow_y_scrollbar()
                                                .children(
                                                    autocomplete_items
                                                        .iter()
                                                        .enumerate()
                                                        .map(|(option_ix, option)| {
                                                            let is_selected = autocomplete_selected_ix
                                                                == Some(option_ix);
                                                            div()
                                                                .px_2()
                                                                .py_1()
                                                                .text_xs()
                                                                .bg(if is_selected {
                                                                    blue()
                                                                } else {
                                                                    surface0()
                                                                })
                                                                .text_color(if is_selected {
                                                                    crust()
                                                                } else {
                                                                    text()
                                                                })
                                                                .child(option.label.clone())
                                                                .on_mouse_up(
                                                                    MouseButton::Left,
                                                                    cx.listener(move |this, _event, _window, cx| {
                                                                        this.autocomplete_selected_ix =
                                                                            Some(option_ix);
                                                                        this.apply_selected_completion();
                                                                        this.recompute_autocomplete(cx);
                                                                        cx.notify();
                                                                    }),
                                                                )
                                                        })
                                                        .collect::<Vec<_>>(),
                                                )
                                                .into_any_element(),
                                        )
                                    } else {
                                        None
                                    })
                            })
                            .collect::<Vec<_>>(),
                    )
                    .children(if active_loading {
                        Some(
                            div()
                                .text_xs()
                                .text_color(text())
                                .child("Loading autocomplete...")
                                .into_any_element(),
                        )
                    } else {
                        None
                    }),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(surface1())
                    .p_3()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(text())
                            .child("Ctrl+S submit | Enter apply/select | Ctrl+N/Ctrl+P navigate"),
                    )
                    .child(
                        div()
                            .bg(green())
                            .hover(|s| s.bg(yellow()))
                            .text_color(crust())
                            .rounded_sm()
                            .px_3()
                            .py_1()
                            .child("Create")
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.submit(cx);
                                }),
                            ),
                    ),
            );

        container.into_any_element()
    }
}
